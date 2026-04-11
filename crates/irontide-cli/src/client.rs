//! HTTP + WebSocket client for the `irontide-api` daemon.
//!
//! This module is the sole control-plane entry point for every CLI mode
//! (batch, REPL, TUI). It owns the `reqwest::Client`, the base URL, and
//! a small set of CLI-side DTOs that mirror the engine's HTTP response
//! shapes.
//!
//! # DTO design (option A)
//!
//! The engine types `TorrentSummary`, `TorrentStats`, `TorrentInfo`,
//! `FileInfo`, and `PeerInfo` derive `Serialize` but not `Deserialize`.
//! Rather than add a `Deserialize` impl to every engine struct (re-opens
//! the engine surface area finalized in T1), we define minimal mirror
//! DTOs here that cover only the fields the CLI actually displays. Each
//! DTO has a `source` comment pointing at the engine type it mirrors so
//! future maintainers can audit drift.
//!
//! # Hash field handling
//!
//! The engine's `Id20` / `Id32` implementations serialize via
//! `serialize_bytes`, which serde_json encodes as an integer array
//! (`[170, 244, ...]`, not a hex string). The DTOs therefore decode
//! hashes as `Vec<u8>` and expose `info_hash_hex()` helpers that the
//! caller uses for display / command dispatch. `TorrentSummary` on the
//! wire is the exception — it already serializes `info_hash` as a hex
//! `String` on the engine side.

use std::time::Duration;

use futures::stream::{Stream, StreamExt};
use reqwest::StatusCode;
use serde::Deserialize;

use crate::error::CliError;

// ─────────────────────────────────────────────────────────────────────────────
// DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// Mirror of `irontide::session::TorrentSummary` (session/src/types.rs:666).
///
/// The only "live" fields the CLI needs for `list` output. `info_hash`
/// is already a hex string on the wire (the engine pre-hexes it in
/// `From<&TorrentStats> for TorrentSummary`), so no byte-array decoding
/// is required.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TorrentSummaryDto {
    /// Hex-encoded v1 info hash (empty for v2-only torrents).
    pub(crate) info_hash: String,
    /// Display name.
    pub(crate) name: String,
    /// Current torrent state (serialized as the enum variant name).
    pub(crate) state: String,
    /// Download progress fraction in `[0.0, 1.0]`.
    #[serde(default)]
    pub(crate) progress: f64,
    /// Current download rate in bytes/sec.
    #[serde(default)]
    pub(crate) download_rate: u64,
    /// Current upload rate in bytes/sec.
    #[serde(default)]
    pub(crate) upload_rate: u64,
    /// Total torrent size in bytes.
    #[serde(default)]
    pub(crate) total_size: u64,
    /// Number of currently connected peers.
    #[serde(default)]
    pub(crate) num_peers: usize,
    /// Time the torrent was added (POSIX seconds).
    #[serde(default)]
    pub(crate) added_time: i64,
}

/// Mirror of the relevant subset of
/// `irontide::session::TorrentStats` (session/src/types.rs:358).
///
/// Field selection is driven by `progress::render_human` / `render_json`
/// plus the `info` command's human output. `info_hashes` is mirrored via
/// [`InfoHashesDto`] so the CLI can recover the hex `info_hash` at any
/// time (the engine does not pre-hex it on this response).
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TorrentStatsDto {
    /// Display name.
    pub(crate) name: String,
    /// Current torrent state (enum variant name).
    pub(crate) state: String,
    /// Progress fraction in `[0.0, 1.0]`.
    #[serde(default)]
    pub(crate) progress: f64,
    /// Progress in parts-per-million (0..=1_000_000).
    #[serde(default)]
    pub(crate) progress_ppm: u32,
    /// Verified payload bytes. Mapped from the engine's `total_done`
    /// (not its `downloaded`, which is session-lifetime payload only — see
    /// `TorrentStats` in `irontide-session/src/types.rs`). The CLI prefers the
    /// verified count because it's what users want to see as "downloaded".
    #[serde(default, rename = "total_done")]
    pub(crate) downloaded: u64,
    /// Uploaded bytes. Mapped from the engine's `total_upload` (session-wide
    /// cumulative) rather than `uploaded` which resets on restart.
    #[serde(default, rename = "total_upload")]
    pub(crate) uploaded: u64,
    /// Total torrent size in bytes (engine field `total`).
    #[serde(default, rename = "total")]
    pub(crate) total: u64,
    /// Current download rate in bytes/sec.
    #[serde(default)]
    pub(crate) download_rate: u64,
    /// Current upload rate in bytes/sec.
    #[serde(default)]
    pub(crate) upload_rate: u64,
    /// Verified pieces.
    #[serde(default)]
    pub(crate) pieces_have: u32,
    /// Total piece count.
    #[serde(default)]
    pub(crate) pieces_total: u32,
    /// Connected peers.
    #[serde(default)]
    pub(crate) peers_connected: usize,
    /// Known-but-not-connected peers.
    #[serde(default)]
    pub(crate) peers_available: usize,
    /// Paused flag.
    #[serde(default)]
    pub(crate) is_paused: bool,
    /// Wanted-complete flag.
    #[serde(default)]
    pub(crate) is_finished: bool,
    /// Full-seed flag.
    #[serde(default)]
    pub(crate) is_seeding: bool,
    /// M159: user-explicit seed-only mode.
    #[serde(default)]
    pub(crate) user_seed_mode: bool,
    /// Identity (v1/v2 info hashes).
    pub(crate) info_hashes: InfoHashesDto,
}

impl TorrentStatsDto {
    /// Hex-encoded v1 info hash for display / command dispatch.
    ///
    /// Returns an empty string for v2-only torrents — matches the
    /// engine's `From<&TorrentStats> for TorrentSummary` convention.
    #[allow(dead_code)] // used by T5 (commands.rs info dispatch)
    pub(crate) fn info_hash_hex(&self) -> String {
        self.info_hashes.v1_hex().unwrap_or_default()
    }
}

/// Mirror of `irontide::session::TorrentInfo` (session/src/types.rs:1246).
///
/// The engine response contains `info_hash` (Id20) as a byte array; the
/// CLI does not use that field (the caller already knows the hash), so
/// it's intentionally omitted.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TorrentInfoDto {
    /// Display name.
    #[allow(dead_code)] // read by progress renderer callers
    pub(crate) name: String,
    /// Total size across all files.
    #[allow(dead_code)]
    pub(crate) total_length: u64,
    /// Piece size in bytes.
    #[allow(dead_code)]
    pub(crate) piece_length: u64,
    /// Number of pieces in the torrent.
    #[allow(dead_code)]
    pub(crate) num_pieces: u32,
    /// File list.
    pub(crate) files: Vec<FileInfoDto>,
    /// BEP 27 private flag.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) private: bool,
}

/// Mirror of `irontide::session::FileInfo` (session/src/types.rs:1237).
///
/// Path is decoded as a `String` (the engine serializes `PathBuf` as a
/// UTF-8 string on unix-like platforms). This avoids dragging in serde's
/// `PathBuf` codec peculiarities on the CLI side.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FileInfoDto {
    /// File path relative to the torrent root.
    pub(crate) path: String,
    /// File length in bytes.
    pub(crate) length: u64,
}

/// Mirror of `irontide::session::PeerInfo` (session/src/types.rs:1189).
///
/// The engine's `addr: SocketAddr` serializes via `Display` + `serde`,
/// producing `"1.2.3.4:6881"`. We decode it as a plain string so the
/// CLI never needs to parse sockets.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct PeerInfoDto {
    /// Remote peer address (`"1.2.3.4:6881"`).
    #[allow(dead_code)] // used by commands.rs::cmd_info peer table
    pub(crate) addr: String,
    /// Client identification string (from extension handshake).
    #[allow(dead_code)]
    pub(crate) client: String,
    /// Whether the peer is choking us.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) peer_choking: bool,
    /// Whether the peer is interested in our data.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) peer_interested: bool,
    /// Whether we are choking the peer.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) am_choking: bool,
    /// Whether we are interested in the peer's data.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) am_interested: bool,
    /// Current download rate from this peer in bytes/sec.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) download_rate: u64,
    /// Current upload rate to this peer in bytes/sec.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) upload_rate: u64,
    /// Number of pieces the peer has.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) num_pieces: u32,
    /// Snubbed flag.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) snubbed: bool,
    /// Seconds since the peer connection was established.
    #[serde(default)]
    #[allow(dead_code)]
    pub(crate) connected_duration_secs: u64,
}

/// Mirror of `irontide::core::InfoHashes` (core/src/info_hashes.rs:14).
///
/// The engine serializes `Id20` / `Id32` as byte arrays (not hex
/// strings) because their serde impls call `serialize_bytes`. The DTO
/// decodes them as `Option<Vec<u8>>` and exposes hex helpers.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct InfoHashesDto {
    /// v1 info hash (SHA-1, 20 bytes) — `None` for v2-only torrents.
    #[serde(default)]
    pub(crate) v1: Option<Vec<u8>>,
    /// v2 info hash (SHA-256, 32 bytes) — `None` for v1-only torrents.
    #[serde(default)]
    #[allow(dead_code)] // exposed for future v2-aware code paths
    pub(crate) v2: Option<Vec<u8>>,
}

impl InfoHashesDto {
    /// Hex-encode the v1 hash if present.
    #[allow(dead_code)]
    pub(crate) fn v1_hex(&self) -> Option<String> {
        self.v1.as_deref().map(hex_encode)
    }
}

/// Simple hex encoder (lowercase, no prefix).
///
/// Kept inline to avoid pulling `hex` crate for this single call site.
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(HEX[(b >> 4) as usize]));
        out.push(char::from(HEX[(b & 0x0f) as usize]));
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// ApiClient
// ─────────────────────────────────────────────────────────────────────────────

/// Base URL used when no flag or env var is supplied.
const DEFAULT_API_URL: &str = "http://127.0.0.1:9080";

/// Short per-request timeout (seconds) for the CLI control plane.
///
/// The daemon is typically localhost — 5s is more than enough, and
/// keeps a wedged daemon from freezing the CLI indefinitely.
const REQUEST_TIMEOUT_SECS: u64 = 5;

/// HTTP / WebSocket control-plane client for `irontide-api`.
pub(crate) struct ApiClient {
    /// Base URL, e.g. `http://127.0.0.1:9080` — no trailing slash.
    base_url: String,
    /// Shared reqwest client (connection-pooled).
    http: reqwest::Client,
}

impl ApiClient {
    /// Create a new client targeting the given base URL.
    ///
    /// A trailing slash on `base_url` is stripped so that path
    /// concatenation is always consistent.
    pub(crate) fn new(base_url: impl Into<String>) -> Self {
        let mut base_url = base_url.into();
        while base_url.ends_with('/') {
            base_url.pop();
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { base_url, http }
    }

    /// Resolve the daemon endpoint in priority order:
    ///
    /// 1. explicit `--api-url` flag value
    /// 2. `IRONTIDE_API_URL` env var
    /// 3. `http://127.0.0.1:9080` default
    pub(crate) fn resolve_url(flag: Option<&str>) -> String {
        if let Some(flag) = flag
            && !flag.is_empty()
        {
            return flag.to_owned();
        }
        if let Ok(env) = std::env::var("IRONTIDE_API_URL")
            && !env.is_empty()
        {
            return env;
        }
        DEFAULT_API_URL.to_owned()
    }

    /// Current base URL (for error messages).
    #[allow(dead_code)] // used by T5
    pub(crate) fn base_url(&self) -> &str {
        &self.base_url
    }

    // ── internal helpers ──────────────────────────────────────────────

    /// Build an absolute URL by appending `path` (which must start with `/`).
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Convert a `reqwest::Error` into a `CliError`, classifying
    /// connection-time failures as `DaemonUnreachable` and everything
    /// else (mid-request) as a generic `Other`.
    fn map_reqwest_error(&self, err: reqwest::Error) -> CliError {
        if err.is_connect() || err.is_timeout() || err.is_request() {
            CliError::DaemonUnreachable {
                url: self.base_url.clone(),
                source: err,
            }
        } else {
            CliError::Other(anyhow::Error::new(err))
        }
    }

    /// Read the response body into a UTF-8 string.
    async fn read_text(resp: reqwest::Response) -> Result<String, CliError> {
        resp.text()
            .await
            .map_err(|e| CliError::Other(anyhow::anyhow!("failed to read response body: {e}")))
    }

    /// Consume a response, returning `Ok(body)` on success or a typed
    /// error otherwise (`NotFound` for 404, `HttpStatus` for other 4xx/5xx).
    async fn expect_ok(resp: reqwest::Response, ctx: &str) -> Result<String, CliError> {
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            let _body = Self::read_text(resp).await.unwrap_or_default();
            return Err(CliError::NotFound(ctx.to_owned()));
        }
        if !status.is_success() {
            let body = Self::read_text(resp).await.unwrap_or_default();
            return Err(CliError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }
        Self::read_text(resp).await
    }

    /// Consume a response, discarding the body on success. Works for both
    /// 204 No Content endpoints and 2xx JSON endpoints where the caller
    /// only cares about reachability (e.g. the health-check `ping`).
    async fn expect_success_discard_body(
        resp: reqwest::Response,
        ctx: &str,
    ) -> Result<(), CliError> {
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            let _ = Self::read_text(resp).await;
            return Err(CliError::NotFound(ctx.to_owned()));
        }
        if !status.is_success() {
            let body = Self::read_text(resp).await.unwrap_or_default();
            return Err(CliError::HttpStatus {
                status: status.as_u16(),
                body,
            });
        }
        Ok(())
    }

    // ── public API ────────────────────────────────────────────────────

    /// Health-check the daemon by hitting `GET /api/v1/session/stats`.
    ///
    /// Returns `Ok(())` if the daemon responds with a 2xx, otherwise the
    /// classified error. Used by the main binary to detect an offline
    /// daemon and emit exit code 3.
    pub(crate) async fn ping(&self) -> Result<(), CliError> {
        let resp = self
            .http
            .get(self.url("/api/v1/session/stats"))
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        Self::expect_success_discard_body(resp, "session stats").await
    }

    /// `GET /api/v1/torrents` — list all active torrents.
    pub(crate) async fn list_torrents(&self) -> Result<Vec<TorrentSummaryDto>, CliError> {
        let resp = self
            .http
            .get(self.url("/api/v1/torrents"))
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        let body = Self::expect_ok(resp, "torrent list").await?;
        let list: Vec<TorrentSummaryDto> = serde_json::from_str(&body)?;
        Ok(list)
    }

    /// `GET /api/v1/torrents/{hash}` — detailed stats for one torrent.
    pub(crate) async fn get_torrent(&self, hash: &str) -> Result<TorrentStatsDto, CliError> {
        validate_hash(hash)?;
        let url = self.url(&format!("/api/v1/torrents/{hash}"));
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        let body = Self::expect_ok(resp, hash).await?;
        let stats: TorrentStatsDto = serde_json::from_str(&body)?;
        Ok(stats)
    }

    /// `POST /api/v1/torrents` with a JSON `{ "uri": "magnet:?..." }` body.
    pub(crate) async fn add_magnet(&self, uri: &str) -> Result<InfoHashesDto, CliError> {
        let body = serde_json::json!({ "uri": uri });
        let resp = self
            .http
            .post(self.url("/api/v1/torrents"))
            .json(&body)
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        let text = Self::expect_ok(resp, "add magnet").await?;
        let hashes: InfoHashesDto = serde_json::from_str(&text)?;
        Ok(hashes)
    }

    /// `POST /api/v1/torrents` with a raw `.torrent` body.
    pub(crate) async fn add_torrent_bytes(&self, bytes: &[u8]) -> Result<InfoHashesDto, CliError> {
        let resp = self
            .http
            .post(self.url("/api/v1/torrents"))
            .header("content-type", "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        let text = Self::expect_ok(resp, "add torrent").await?;
        let hashes: InfoHashesDto = serde_json::from_str(&text)?;
        Ok(hashes)
    }

    /// `DELETE /api/v1/torrents/{hash}`.
    pub(crate) async fn remove_torrent(&self, hash: &str) -> Result<(), CliError> {
        validate_hash(hash)?;
        let resp = self
            .http
            .delete(self.url(&format!("/api/v1/torrents/{hash}")))
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        Self::expect_success_discard_body(resp, hash).await
    }

    /// `POST /api/v1/torrents/{hash}/pause`.
    pub(crate) async fn pause(&self, hash: &str) -> Result<(), CliError> {
        validate_hash(hash)?;
        let resp = self
            .http
            .post(self.url(&format!("/api/v1/torrents/{hash}/pause")))
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        Self::expect_success_discard_body(resp, hash).await
    }

    /// `POST /api/v1/torrents/{hash}/resume`.
    pub(crate) async fn resume(&self, hash: &str) -> Result<(), CliError> {
        validate_hash(hash)?;
        let resp = self
            .http
            .post(self.url(&format!("/api/v1/torrents/{hash}/resume")))
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        Self::expect_success_discard_body(resp, hash).await
    }

    /// `POST /api/v1/torrents/{hash}/seed_mode` (T2 endpoint).
    pub(crate) async fn set_seed_mode(&self, hash: &str, enabled: bool) -> Result<(), CliError> {
        validate_hash(hash)?;
        let body = serde_json::json!({ "enabled": enabled });
        let resp = self
            .http
            .post(self.url(&format!("/api/v1/torrents/{hash}/seed_mode")))
            .json(&body)
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        Self::expect_success_discard_body(resp, hash).await
    }

    /// `GET /api/v1/torrents/{hash}/info` — file list + piece metadata.
    pub(crate) async fn torrent_info(&self, hash: &str) -> Result<TorrentInfoDto, CliError> {
        validate_hash(hash)?;
        let resp = self
            .http
            .get(self.url(&format!("/api/v1/torrents/{hash}/info")))
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        let body = Self::expect_ok(resp, hash).await?;
        let info: TorrentInfoDto = serde_json::from_str(&body)?;
        Ok(info)
    }

    /// `GET /api/v1/torrents/{hash}/peers`.
    pub(crate) async fn torrent_peers(&self, hash: &str) -> Result<Vec<PeerInfoDto>, CliError> {
        validate_hash(hash)?;
        let resp = self
            .http
            .get(self.url(&format!("/api/v1/torrents/{hash}/peers")))
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;
        let body = Self::expect_ok(resp, hash).await?;
        let peers: Vec<PeerInfoDto> = serde_json::from_str(&body)?;
        Ok(peers)
    }

    /// Subscribe to the WebSocket event stream at `/api/v1/events`.
    ///
    /// Returns a stream of raw JSON message strings. The caller chooses
    /// whether to typed-decode them — at the CLI layer the REPL/TUI
    /// currently just pretty-prints them, so we don't introduce a
    /// wire-level enum here.
    ///
    /// Handles both `http://` → `ws://` and `https://` → `wss://`
    /// scheme conversions automatically.
    #[allow(dead_code)] // consumed by T6/T7 (REPL/TUI)
    pub(crate) async fn subscribe_events(
        &self,
    ) -> Result<impl Stream<Item = Result<String, CliError>>, CliError> {
        // Build the ws:// or wss:// URL from the current base URL.
        let ws_url = if let Some(rest) = self.base_url.strip_prefix("http://") {
            format!("ws://{rest}/api/v1/events")
        } else if let Some(rest) = self.base_url.strip_prefix("https://") {
            format!("wss://{rest}/api/v1/events")
        } else {
            return Err(CliError::InvalidInput(format!(
                "unsupported base URL scheme: {}",
                self.base_url
            )));
        };

        let (ws_stream, _resp) = tokio_tungstenite::connect_async(&ws_url)
            .await
            .map_err(|e| CliError::WebSocket(format!("{ws_url}: {e}")))?;

        // `ws_stream` is a `WebSocketStream` which yields
        // `Result<Message, tungstenite::Error>`. Map each item to
        // `Result<String, CliError>` — discarding pings/pongs, converting
        // text frames directly, and decoding binary frames as UTF-8.
        let mapped = ws_stream.filter_map(|msg| async move {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    Some(Ok(text.to_string()))
                }
                Ok(tokio_tungstenite::tungstenite::Message::Binary(bytes)) => {
                    match String::from_utf8(bytes.to_vec()) {
                        Ok(text) => Some(Ok(text)),
                        Err(e) => Some(Err(CliError::WebSocket(format!("non-UTF-8 frame: {e}")))),
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => None,
                Ok(_) => None, // Ping/Pong/Frame — ignore.
                Err(e) => Some(Err(CliError::WebSocket(e.to_string()))),
            }
        });

        Ok(mapped)
    }
}

/// Validate that `hash` looks like a SHA-1 info hash (40 lowercase hex chars).
///
/// The CLI-level hash-prefix resolver (`commands::resolve_hash`) is
/// responsible for expanding prefixes into full hashes before calling
/// the client. Raw client calls therefore require a full hash so that
/// an accidental prefix can't produce a silent 404.
fn validate_hash(hash: &str) -> Result<(), CliError> {
    if hash.len() != 40 {
        return Err(CliError::InvalidInput(format!(
            "expected 40-char hex info hash, got {} chars",
            hash.len()
        )));
    }
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CliError::InvalidInput(format!(
            "info hash contains non-hex characters: {hash}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// All `resolve_url` cases exercised in a single test to avoid
    /// racing on the shared `IRONTIDE_API_URL` env var. `cargo test`
    /// runs tests in parallel by default and multiple tests toggling
    /// the same process-wide env var would be a latent data race even
    /// under `unsafe`. Serializing them into one function makes the
    /// behaviour deterministic without pulling in `serial_test`.
    #[test]
    fn resolve_url_precedence() {
        // Flag wins regardless of env.
        // SAFETY: Single-threaded test, protected by being the only
        // test that touches IRONTIDE_API_URL.
        unsafe {
            std::env::set_var("IRONTIDE_API_URL", "http://env.invalid:4242");
        }
        assert_eq!(
            ApiClient::resolve_url(Some("http://example.invalid:1234")),
            "http://example.invalid:1234"
        );

        // No flag → env.
        assert_eq!(ApiClient::resolve_url(None), "http://env.invalid:4242");

        // No flag, no env → default.
        // SAFETY: single-threaded test.
        unsafe {
            std::env::remove_var("IRONTIDE_API_URL");
        }
        assert_eq!(ApiClient::resolve_url(None), "http://127.0.0.1:9080");

        // Empty flag is treated as absent.
        assert_eq!(ApiClient::resolve_url(Some("")), "http://127.0.0.1:9080");
    }

    #[test]
    fn new_strips_trailing_slashes() {
        let c = ApiClient::new("http://host:9080///");
        assert_eq!(c.base_url, "http://host:9080");
    }

    #[test]
    fn url_joins_path_correctly() {
        let c = ApiClient::new("http://host:9080");
        assert_eq!(
            c.url("/api/v1/torrents"),
            "http://host:9080/api/v1/torrents"
        );
    }

    #[test]
    fn validate_hash_accepts_canonical() {
        assert!(validate_hash("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d").is_ok());
    }

    #[test]
    fn validate_hash_rejects_short() {
        let err = validate_hash("aabb").unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }

    #[test]
    fn validate_hash_rejects_non_hex() {
        let err = validate_hash("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").unwrap_err();
        assert!(matches!(err, CliError::InvalidInput(_)));
    }

    #[test]
    fn info_hashes_dto_v1_hex_encodes_correctly() {
        let raw = r#"{"v1":[170,244,198,29,220,197,232,162,218,190,222,15,59,72,44,217,174,169,67,77],"v2":null}"#;
        let dto: InfoHashesDto = serde_json::from_str(raw).expect("parse");
        assert_eq!(
            dto.v1_hex().as_deref(),
            Some("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d")
        );
    }

    #[test]
    fn torrent_stats_dto_round_trip() {
        let raw = r#"{
            "name":"ubuntu-24.04.iso",
            "state":"Downloading",
            "progress":0.42,
            "progress_ppm":420000,
            "total_done":42000,
            "total_upload":1000,
            "total":100000,
            "download_rate":1024,
            "upload_rate":512,
            "pieces_have":10,
            "pieces_total":20,
            "peers_connected":5,
            "peers_available":10,
            "is_paused":false,
            "is_finished":false,
            "is_seeding":false,
            "user_seed_mode":false,
            "info_hashes":{"v1":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20],"v2":null}
        }"#;
        let dto: TorrentStatsDto = serde_json::from_str(raw).expect("parse");
        assert_eq!(dto.name, "ubuntu-24.04.iso");
        assert_eq!(dto.state, "Downloading");
        assert_eq!(dto.downloaded, 42_000);
        assert_eq!(dto.total, 100_000);
        assert_eq!(
            dto.info_hash_hex(),
            "0102030405060708090a0b0c0d0e0f1011121314"
        );
    }
}
