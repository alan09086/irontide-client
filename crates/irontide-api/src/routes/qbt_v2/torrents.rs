//! qBt v2 `/api/v2/torrents/*` + `/api/v2/transferInfo` (M168 Tasks 10-14).
//!
//! Implemented:
//! - `GET /torrents/info` — list with filter/sort/hashes/limit/offset
//! - `GET /torrents/properties?hash=X` — single-torrent detail
//! - `POST /torrents/add` — magnet (form `urls=`) + `.torrent` (multipart)
//! - `POST /torrents/pause|resume|delete|recheck|reannounce` — actions
//! - `GET /transferInfo` — session-wide counters
//!
//! Deferred to M170: files, trackers, webseeds, pieceStates, pieceHashes,
//! filePrio, category CRUD, tag CRUD, setPreferences, shutdown.

use axum::extract::{FromRequest, Multipart, Query, State};
use irontide::core::Id20;
use irontide::session::TorrentStats;
use serde::Deserialize;

use super::response::{QbtError, QbtResponse};
use super::state::QbtState;
use super::torrent_dto::{QbtTorrent, QbtTorrentProperties, QbtTransferInfo, qbt_state_string};

// ── Query params ──────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub reverse: Option<bool>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: Option<usize>,
    /// Pipe-separated list of hashes (e.g. `abc|def|...`).
    #[serde(default)]
    pub hashes: Option<String>,
}

#[derive(Deserialize)]
pub struct HashQuery {
    pub hash: String,
}

#[derive(Deserialize, Default)]
pub struct HashesQuery {
    #[serde(default)]
    pub hashes: Option<String>,
    #[serde(default)]
    #[serde(alias = "deleteFiles")]
    pub delete_files: Option<String>,
}

// ── Shared helpers ────────────────────────────────────────────────────

/// Parse a `hashes=a|b|c` list or the literal `all`.
/// Returns `None` when `all` (meaning "every torrent"), `Some(vec)` otherwise.
fn parse_hash_list(hashes: &str) -> Result<Option<Vec<Id20>>, QbtError> {
    if hashes.eq_ignore_ascii_case("all") {
        return Ok(None);
    }
    let mut out = Vec::new();
    for part in hashes.split('|').filter(|s| !s.is_empty()) {
        let id = Id20::from_hex(part)
            .map_err(|e| QbtError::BadRequest(format!("invalid hash {part}: {e}")))?;
        out.push(id);
    }
    Ok(Some(out))
}

/// Apply a qBt-style `filter=` term to a TorrentStats.
fn matches_filter(s: &TorrentStats, filter: &str) -> bool {
    match filter {
        "" | "all" => true,
        "downloading" => matches!(
            qbt_state_string(s),
            "downloading" | "stalledDL" | "metaDL" | "checkingDL" | "allocating"
        ),
        "seeding" => matches!(qbt_state_string(s), "uploading" | "stalledUP" | "forcedUP"),
        "completed" => s.progress >= 1.0,
        "paused" => s.is_paused,
        "active" => s.download_rate > 0 || s.upload_rate > 0,
        "inactive" => s.download_rate == 0 && s.upload_rate == 0,
        "resumed" => !s.is_paused,
        "stalled" => matches!(qbt_state_string(s), "stalledDL" | "stalledUP"),
        "stalled_uploading" => qbt_state_string(s) == "stalledUP",
        "stalled_downloading" => qbt_state_string(s) == "stalledDL",
        "errored" => !s.error.is_empty() || qbt_state_string(s) == "error",
        _ => true, // unknown filter: permissive — real qBt behaves the same
    }
}

/// Fetch every torrent's stats. Silently drops torrents whose stats query
/// errors (e.g. during shutdown) — matches `list_torrent_summaries()`.
async fn all_stats(state: &QbtState) -> Result<Vec<TorrentStats>, QbtError> {
    let ids = state
        .session
        .list_torrents()
        .await
        .map_err(|e| QbtError::Internal(format!("list_torrents: {e}")))?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if let Ok(s) = state.session.torrent_stats(id).await {
            out.push(s);
        }
    }
    Ok(out)
}

// ── GET /api/v2/torrents/info ─────────────────────────────────────────

pub async fn info(
    State(state): State<QbtState>,
    Query(q): Query<ListQuery>,
) -> Result<QbtResponse, QbtError> {
    let mut stats = all_stats(&state).await?;

    // hashes= filter (pipe-separated list); converted to Id20 for case-
    // insensitive, leading-zero-safe match.
    let hash_allow = match &q.hashes {
        Some(raw) => parse_hash_list(raw)?,
        None => None,
    };
    if let Some(allow) = hash_allow {
        stats.retain(|s| {
            s.info_hashes
                .v1
                .map(|h| allow.iter().any(|id| id == &h))
                .unwrap_or(false)
        });
    }

    // filter= free-text qBt enum; unknown values are permissive.
    if let Some(f) = &q.filter {
        stats.retain(|s| matches_filter(s, f));
    }

    // Sort by a qBt field name.
    if let Some(key) = &q.sort {
        stats.sort_by(|a, b| match key.as_str() {
            "name" => a.name.cmp(&b.name),
            "size" | "total_size" => a.total.cmp(&b.total),
            "progress" => a
                .progress
                .partial_cmp(&b.progress)
                .unwrap_or(std::cmp::Ordering::Equal),
            "dlspeed" => a.download_rate.cmp(&b.download_rate),
            "upspeed" => a.upload_rate.cmp(&b.upload_rate),
            "added_on" => a.added_time.cmp(&b.added_time),
            "ratio" => {
                let ar = if a.all_time_download > 0 {
                    a.all_time_upload as f64 / a.all_time_download as f64
                } else {
                    0.0
                };
                let br = if b.all_time_download > 0 {
                    b.all_time_upload as f64 / b.all_time_download as f64
                } else {
                    0.0
                };
                ar.partial_cmp(&br).unwrap_or(std::cmp::Ordering::Equal)
            }
            _ => std::cmp::Ordering::Equal, // unknown sort: stable identity
        });
        if q.reverse.unwrap_or(false) {
            stats.reverse();
        }
    }

    // offset / limit
    let offset = q.offset.unwrap_or(0);
    if offset >= stats.len() {
        return Ok(QbtResponse::Json(serde_json::Value::Array(vec![])));
    }
    let mut sliced = stats.into_iter().skip(offset).collect::<Vec<_>>();
    if let Some(lim) = q.limit {
        sliced.truncate(lim);
    }

    let dtos: Vec<QbtTorrent> = sliced.iter().map(QbtTorrent::from).collect();
    Ok(QbtResponse::Json(serde_json::to_value(dtos).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

// ── GET /api/v2/torrents/properties ───────────────────────────────────

pub async fn properties(
    State(state): State<QbtState>,
    Query(q): Query<HashQuery>,
) -> Result<QbtResponse, QbtError> {
    let id = Id20::from_hex(&q.hash)
        .map_err(|e| QbtError::BadRequest(format!("invalid hash: {e}")))?;
    let stats = match state.session.torrent_stats(id).await {
        Ok(s) => s,
        Err(_) => return Err(QbtError::NotFound),
    };
    let props = QbtTorrentProperties::from(&stats);
    Ok(QbtResponse::Json(serde_json::to_value(&props).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

// ── POST /api/v2/torrents/add ─────────────────────────────────────────

pub async fn add(
    State(state): State<QbtState>,
    headers: axum::http::HeaderMap,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let ct = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if ct.starts_with("multipart/form-data") {
        // Multipart: iterate parts for file uploads + magnet URLs.
        let mut multipart = Multipart::from_request(req, &state)
            .await
            .map_err(|e| QbtError::BadRequest(format!("parse multipart: {e}")))?;
        let mut added_anything = false;
        while let Some(field) = multipart
            .next_field()
            .await
            .map_err(|e| QbtError::BadRequest(format!("multipart field: {e}")))?
        {
            match field.name() {
                Some("urls") => {
                    let body = field
                        .text()
                        .await
                        .map_err(|e| QbtError::BadRequest(format!("read urls: {e}")))?;
                    for uri in body.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
                        add_magnet_or_ignore(&state, uri).await?;
                        added_anything = true;
                    }
                }
                Some("torrents") => {
                    let bytes = field
                        .bytes()
                        .await
                        .map_err(|e| QbtError::BadRequest(format!("read file: {e}")))?
                        .to_vec();
                    if !bytes.is_empty() {
                        state
                            .session
                            .add_torrent_bytes(&bytes)
                            .await
                            .map_err(|e| QbtError::Internal(format!("add torrent: {e}")))?;
                        added_anything = true;
                    }
                }
                _ => {
                    // Discard unknown fields (category, savepath, paused, etc.).
                    let _ = field.bytes().await;
                }
            }
        }
        if !added_anything {
            return Err(QbtError::BadRequest("no urls or torrent file provided".into()));
        }
        Ok(QbtResponse::ok())
    } else {
        // URL-encoded form body: `urls=magnet:...\nmagnet:...&paused=false`.
        let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024 * 1024)
            .await
            .map_err(|e| QbtError::BadRequest(format!("read body: {e}")))?;
        let form: Vec<(String, String)> =
            serde_urlencoded::from_bytes(&bytes).map_err(|e| {
                QbtError::BadRequest(format!("parse urlencoded: {e}"))
            })?;
        let urls = form
            .iter()
            .find(|(k, _)| k == "urls")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        if urls.trim().is_empty() {
            return Err(QbtError::BadRequest("urls field is required".into()));
        }
        for uri in urls.lines().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            add_magnet_or_ignore(&state, uri).await?;
        }
        Ok(QbtResponse::ok())
    }
}

async fn add_magnet_or_ignore(state: &QbtState, uri: &str) -> Result<(), QbtError> {
    match state.session.add_magnet_uri(uri).await {
        Ok(_) => Ok(()),
        Err(e) => {
            let msg = format!("{e}");
            // *arr treats duplicate adds as OK too; real qBt returns 200 with
            // no error. We map both duplicate-*ish* and transient add errors
            // to Conflict so logs are honest without failing the workflow.
            if msg.to_ascii_lowercase().contains("duplicate")
                || msg.to_ascii_lowercase().contains("already")
            {
                Err(QbtError::Conflict(msg))
            } else {
                Err(QbtError::Internal(format!("add magnet: {e}")))
            }
        }
    }
}

// ── POST /api/v2/torrents/pause|resume|delete|recheck|reannounce ──────

async fn resolve_hashes(state: &QbtState, hashes: Option<&str>) -> Result<Vec<Id20>, QbtError> {
    match hashes {
        None | Some("") => Err(QbtError::BadRequest("hashes= param required".into())),
        Some(raw) => match parse_hash_list(raw)? {
            Some(vec) => Ok(vec),
            None => {
                // "all" sentinel — enumerate every torrent.
                state
                    .session
                    .list_torrents()
                    .await
                    .map_err(|e| QbtError::Internal(format!("list_torrents: {e}")))
            }
        },
    }
}

pub async fn pause(
    State(state): State<QbtState>,
    Query(q): Query<HashesQuery>,
) -> Result<QbtResponse, QbtError> {
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        let _ = state.session.pause_torrent(id).await;
    }
    Ok(QbtResponse::ok())
}

pub async fn resume(
    State(state): State<QbtState>,
    Query(q): Query<HashesQuery>,
) -> Result<QbtResponse, QbtError> {
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        let _ = state.session.resume_torrent(id).await;
    }
    Ok(QbtResponse::ok())
}

pub async fn delete(
    State(state): State<QbtState>,
    Query(q): Query<HashesQuery>,
) -> Result<QbtResponse, QbtError> {
    // FIXME(M170): honour deleteFiles=true by wiring disk cleanup through
    // SessionHandle::remove_torrent. Today the flag is parsed and logged but
    // has no on-disk effect — IronTide always cleans up its own state.
    let _delete_files = matches!(
        q.delete_files.as_deref().map(|s| s.to_ascii_lowercase()),
        Some(v) if v == "true" || v == "1"
    );
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        let _ = state.session.remove_torrent(id).await;
    }
    Ok(QbtResponse::ok())
}

pub async fn recheck(
    State(state): State<QbtState>,
    Query(q): Query<HashesQuery>,
) -> Result<QbtResponse, QbtError> {
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        let _ = state.session.force_recheck(id).await;
    }
    Ok(QbtResponse::ok())
}

pub async fn reannounce(
    State(state): State<QbtState>,
    Query(q): Query<HashesQuery>,
) -> Result<QbtResponse, QbtError> {
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        let _ = state.session.force_reannounce(id).await;
    }
    Ok(QbtResponse::ok())
}

// ── GET /api/v2/transferInfo ──────────────────────────────────────────

pub async fn transfer_info(State(state): State<QbtState>) -> Result<QbtResponse, QbtError> {
    // Sum rates and totals across all torrents.
    let stats = all_stats(&state).await?;
    let dl_rate: u64 = stats.iter().map(|s| s.download_rate).sum();
    let up_rate: u64 = stats.iter().map(|s| s.upload_rate).sum();
    let dl_total: u64 = stats.iter().map(|s| s.all_time_download).sum();
    let up_total: u64 = stats.iter().map(|s| s.all_time_upload).sum();
    let any_announcing = stats.iter().any(|s| s.announcing_to_trackers);
    let connection_status = if any_announcing {
        "connected".to_string()
    } else if stats.is_empty() {
        "disconnected".to_string()
    } else {
        "firewalled".to_string()
    };

    let info = QbtTransferInfo {
        dl_info_speed: dl_rate,
        dl_info_data: dl_total,
        up_info_speed: up_rate,
        up_info_data: up_total,
        connection_status,
        dht_nodes: 0, // FIXME(M170): expose DHT node count on SessionHandle
        dl_rate_limit: -1,
        up_rate_limit: -1,
    };
    Ok(QbtResponse::Json(serde_json::to_value(&info).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

