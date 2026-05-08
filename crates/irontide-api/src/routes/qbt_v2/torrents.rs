#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: qBt wire format — torrents/transferInfo follow qBittorrent's signed-i64 encoding for unsigned counters"
)]

//! qBt v2 `/api/v2/torrents/*` + `/api/v2/transferInfo` (M168 Tasks 10-14, M170 wiring).
//!
//! Implemented:
//! - `GET /torrents/info` — list with filter/sort/hashes/limit/offset
//!   (M170: `category=` filter over `TorrentStats::category`).
//! - `GET /torrents/properties?hash=X` — single-torrent detail.
//! - `POST /torrents/add` — magnet (form `urls=`) + `.torrent` (multipart)
//!   (M170: `category` / `savepath` / `paused` plumbed through
//!   [`SessionAddTorrentParams`](irontide::session::SessionAddTorrentParams)).
//! - `POST /torrents/pause|resume|delete|recheck|reannounce` — actions
//!   (M170: `/delete` honours `deleteFiles=true` via
//!   [`remove_torrent_with_files`](irontide::session::SessionHandle::remove_torrent_with_files)).
//! - `GET /transferInfo` — session-wide counters.
//!
//! Deferred to M171: trackers, webseeds, pieceStates, pieceHashes, filePrio,
//! tag CRUD, setPreferences, shutdown. Files endpoint + category CRUD are
//! sibling modules inside M170 (Lanes B + C).

use std::path::PathBuf;

use axum::extract::{FromRequest, Multipart, Query, State};
use irontide::core::Id20;
use irontide::session::{SessionAddTorrentParams, TorrentStats};
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

#[derive(Deserialize, Default, Clone, Debug)]
pub struct HashesQuery {
    #[serde(default)]
    pub hashes: Option<String>,
    #[serde(default)]
    #[serde(alias = "deleteFiles")]
    pub delete_files: Option<String>,
}

/// v0.173.1 Class B fix: accept `hashes=` / `deleteFiles=` from EITHER the
/// URL query string OR an `application/x-www-form-urlencoded` request body,
/// matching qBt `WebUI` v2 parity. Real `*arr` clients (Radarr / Sonarr /
/// Prowlarr / Lidarr) POST these params in the body; `axum::extract::Query`
/// only reads the URL, so a strict `Query<HashesQuery>` handler rejects the
/// `*arr` flow with 400.
///
/// Resolution order (URL query wins on conflict, but empty-string values in
/// the query do NOT shadow non-empty body values — qBt's loose clients
/// sometimes emit `?hashes=` with an empty value):
///
/// 1. Parse the URL query; if it carries a non-empty `hashes` or
///    `delete_files`, return that.
/// 2. Otherwise consume the body (capped at 64 KiB, the same cap used by
///    the category / tag form parsers) and parse it as form-urlencoded.
/// 3. If both query and body are empty or absent, return
///    [`QbtError::BadRequest`] so the caller gets 400 instead of a silent
///    no-op.
///
/// Reuses the `to_bytes(req.into_body(), 64 * 1024)` + `serde_urlencoded::
/// from_bytes(...)` pattern already proven in
/// [`super::categories`] / [`super::tags`].
///
/// # Errors
/// - [`QbtError::BadRequest`] when the body exceeds the 64 KiB cap, is not
///   valid form-urlencoded, or when neither query nor body supplies a
///   recognised field.
pub(super) async fn extract_hashes_params(
    req: axum::extract::Request,
) -> Result<HashesQuery, QbtError> {
    let query_parsed = req
        .uri()
        .query()
        .and_then(|raw| serde_urlencoded::from_str::<HashesQuery>(raw).ok());
    if let Some(ref q) = query_parsed {
        let has_hashes = q.hashes.as_deref().is_some_and(|s| !s.is_empty());
        let has_delete = q.delete_files.as_deref().is_some_and(|s| !s.is_empty());
        if has_hashes || has_delete {
            return Ok(q.clone());
        }
    }

    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
        .await
        .map_err(|e| QbtError::BadRequest(format!("read body: {e}")))?;
    if !bytes.is_empty() {
        let body_parsed: HashesQuery = serde_urlencoded::from_bytes(&bytes)
            .map_err(|e| QbtError::BadRequest(format!("parse body: {e}")))?;
        let has_hashes = body_parsed.hashes.as_deref().is_some_and(|s| !s.is_empty());
        let has_delete = body_parsed
            .delete_files
            .as_deref()
            .is_some_and(|s| !s.is_empty());
        if has_hashes || has_delete {
            return Ok(body_parsed);
        }
    }

    Err(QbtError::BadRequest(
        "hashes= parameter required in query or form body".into(),
    ))
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

/// qBt "true"/"1" → `true`, anything else → `false` (case-insensitive).
fn parse_bool_flag(raw: &str) -> bool {
    let v = raw.trim().to_ascii_lowercase();
    v == "true" || v == "1"
}

/// Apply a qBt-style `filter=` term to a `TorrentStats`.
fn matches_filter(s: &TorrentStats, filter: &str) -> bool {
    match filter {
        // These are distinct from `_` because `_` is the permissive fallback
        // for unknown filter values (qBt parity), while these are explicit
        // no-op states.
        #[allow(
            clippy::match_same_arms,
            reason = "explicit qBt filter values distinct from unknown fallback"
        )]
        "" | "all" => true,
        "downloading" => matches!(
            qbt_state_string(s),
            "downloading" | "stalledDL" | "metaDL" | "checkingDL" | "allocating"
        ),
        "seeding" => matches!(qbt_state_string(s), "uploading" | "stalledUP" | "forcedUP"),
        "completed" => s.progress >= 1.0,
        "paused" => s.is_paused || s.is_queued,
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

/// # Errors
///
/// Returns an error if fetching torrent stats or serialization fails.
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
                .is_some_and(|h| allow.iter().any(|id| id == &h))
        });
    }

    // category= filter (M170). qBt convention: empty string matches only
    // uncategorised torrents; a named value matches exactly.
    if let Some(cat) = &q.category {
        if cat.is_empty() {
            stats.retain(|s| s.category.is_none());
        } else {
            stats.retain(|s| s.category.as_deref() == Some(cat.as_str()));
        }
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

/// # Errors
///
/// Returns an error if the hash is invalid or the torrent is not found.
pub async fn properties(
    State(state): State<QbtState>,
    Query(q): Query<HashQuery>,
) -> Result<QbtResponse, QbtError> {
    let id =
        Id20::from_hex(&q.hash).map_err(|e| QbtError::BadRequest(format!("invalid hash: {e}")))?;
    let Ok(stats) = state.session.torrent_stats(id).await else {
        return Err(QbtError::NotFound);
    };
    let props = QbtTorrentProperties::from(&stats);
    Ok(QbtResponse::Json(serde_json::to_value(&props).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

// ── POST /api/v2/torrents/add ─────────────────────────────────────────

/// Staged form inputs for the M170 add path.
///
/// Multipart field order is not guaranteed by the client, so we drain
/// every relevant field before dispatching any add — otherwise a `urls=`
/// part that arrives before `category=` would be processed with the wrong
/// category.
#[derive(Default)]
struct AddFormState {
    /// Magnet URIs (one per non-blank line of every `urls=` part).
    magnet_uris: Vec<String>,
    /// Raw `.torrent` bodies (multipart `torrents=` repeats per file).
    torrent_files: Vec<Vec<u8>>,
    /// Optional category label (resolved against the session registry).
    category: Option<String>,
    /// Optional explicit save path — wins over a category's `save_path`.
    savepath: Option<String>,
    /// Whether the torrent should start paused.
    paused: bool,
}

impl AddFormState {
    fn has_source(&self) -> bool {
        !self.magnet_uris.is_empty() || !self.torrent_files.is_empty()
    }
}

/// # Errors
///
/// Returns an error if the request body is malformed or adding the torrent fails.
pub async fn add(
    State(state): State<QbtState>,
    headers: axum::http::HeaderMap,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let ct = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let form = if ct.starts_with("multipart/form-data") {
        parse_multipart_add(&state, req).await?
    } else {
        parse_urlencoded_add(req).await?
    };

    if !form.has_source() {
        return Err(QbtError::BadRequest(
            "urls or torrent file is required".into(),
        ));
    }

    for uri in &form.magnet_uris {
        add_one(&state, build_params_magnet(uri, &form)).await?;
    }
    for bytes in &form.torrent_files {
        add_one(&state, build_params_bytes(bytes.clone(), &form)).await?;
    }
    Ok(QbtResponse::ok())
}

/// Accumulate all multipart fields into an [`AddFormState`] before any
/// torrent is added. Unknown field names are drained + discarded so the
/// multipart stream doesn't stall waiting for a consumer.
async fn parse_multipart_add(
    state: &QbtState,
    req: axum::extract::Request,
) -> Result<AddFormState, QbtError> {
    let mut multipart = Multipart::from_request(req, state)
        .await
        .map_err(|e| QbtError::BadRequest(format!("parse multipart: {e}")))?;
    let mut out = AddFormState::default();
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
                for uri in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    out.magnet_uris.push(uri.to_owned());
                }
            }
            Some("torrents") => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| QbtError::BadRequest(format!("read file: {e}")))?
                    .to_vec();
                if !bytes.is_empty() {
                    out.torrent_files.push(bytes);
                }
            }
            Some("category") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| QbtError::BadRequest(format!("read category: {e}")))?;
                if !v.is_empty() {
                    out.category = Some(v);
                }
            }
            Some("savepath") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| QbtError::BadRequest(format!("read savepath: {e}")))?;
                if !v.is_empty() {
                    out.savepath = Some(v);
                }
            }
            Some("paused") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| QbtError::BadRequest(format!("read paused: {e}")))?;
                out.paused = parse_bool_flag(&v);
            }
            _ => {
                // Drain unknown fields so the parser can advance.
                let _ = field.bytes().await;
            }
        }
    }
    Ok(out)
}

/// Drain a URL-encoded form body. Binary `.torrent` uploads cannot be
/// encoded this way, so `torrent_files` is always empty here.
async fn parse_urlencoded_add(req: axum::extract::Request) -> Result<AddFormState, QbtError> {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024 * 1024)
        .await
        .map_err(|e| QbtError::BadRequest(format!("read body: {e}")))?;
    let form: Vec<(String, String)> = serde_urlencoded::from_bytes(&bytes)
        .map_err(|e| QbtError::BadRequest(format!("parse urlencoded: {e}")))?;
    let mut out = AddFormState::default();
    for (k, v) in form {
        match k.as_str() {
            "urls" => {
                for uri in v.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    out.magnet_uris.push(uri.to_owned());
                }
            }
            "category" if !v.is_empty() => {
                out.category = Some(v);
            }
            "savepath" if !v.is_empty() => {
                out.savepath = Some(v);
            }
            "paused" => out.paused = parse_bool_flag(&v),
            _ => {}
        }
    }
    Ok(out)
}

fn build_params_magnet(uri: &str, form: &AddFormState) -> SessionAddTorrentParams {
    apply_form_knobs(SessionAddTorrentParams::magnet(uri), form)
}

fn build_params_bytes(bytes: Vec<u8>, form: &AddFormState) -> SessionAddTorrentParams {
    apply_form_knobs(SessionAddTorrentParams::bytes(bytes), form)
}

/// `savepath` wins over `category` when both are present; both affect the
/// download dir (category resolves via the registry inside the session).
/// The category label is still recorded on the torrent even when an
/// explicit savepath is used, matching qBt's behaviour.
fn apply_form_knobs(
    mut params: SessionAddTorrentParams,
    form: &AddFormState,
) -> SessionAddTorrentParams {
    if let Some(path) = &form.savepath {
        params = params.with_download_dir(PathBuf::from(path));
    }
    if let Some(name) = &form.category {
        params = params.with_category(name.clone());
    }
    params.paused(form.paused)
}

/// Map a session error onto a qBt-shaped HTTP response. Category misses
/// and deletion-race collisions both become 409 Conflict; duplicate adds
/// match the M168 convention (409 with the session's own message).
async fn add_one(state: &QbtState, params: SessionAddTorrentParams) -> Result<(), QbtError> {
    match state.session.add_torrent(params).await {
        Ok(_) => Ok(()),
        Err(irontide::session::Error::CategoryNotFound(name)) => Err(QbtError::Conflict(format!(
            "category '{name}' does not exist"
        ))),
        Err(irontide::session::Error::TorrentBeingRemoved(_)) => Err(QbtError::Conflict(
            "torrent is being removed, try again".into(),
        )),
        Err(e) => {
            let msg = format!("{e}");
            let low = msg.to_ascii_lowercase();
            if low.contains("duplicate") || low.contains("already") {
                Err(QbtError::Conflict(msg))
            } else {
                Err(QbtError::Internal(format!("add torrent: {e}")))
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

/// v0.173.1 Class B + C fix: `pause` now accepts `hashes=` from either
/// URL query or form body (Class B), and logs session errors at warn
/// level instead of swallowing them with `let _ = ...` (Class C). Per qBt
/// `WebUI` v2 bulk-idempotency semantics we still return 200 OK — individual
/// torrent errors must not take down a whole bulk action — but the caller
/// and operator now have a visible failure signal.
///
/// # Errors
///
/// Returns an error if the request parameters are malformed.
pub async fn pause(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let q = extract_hashes_params(req).await?;
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        if let Err(e) = state.session.pause_torrent(id).await {
            tracing::warn!(%id, error = %e,
                "pause_torrent failed — reported to client as 200 per qBt bulk idempotency");
        }
    }
    Ok(QbtResponse::ok())
}

/// v0.173.1 Class B + C fix: form-body acceptance + logged session
/// errors on `resume`. See [`pause`] for the full rationale.
///
/// # Errors
///
/// Returns an error if the request parameters are malformed.
pub async fn resume(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let q = extract_hashes_params(req).await?;
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        if let Err(e) = state.session.resume_torrent(id).await {
            tracing::warn!(%id, error = %e,
                "resume_torrent failed — reported to client as 200 per qBt bulk idempotency");
        }
    }
    Ok(QbtResponse::ok())
}

/// v0.173.1 Class B + C fix: form-body acceptance + logged session
/// errors on `delete`. See [`pause`] for the full rationale. Preserves
/// M170's `deleteFiles=true` semantics: when honoured we call
/// [`SessionHandle::remove_torrent_with_files`] (prunes the on-disk file
/// tree), otherwise the plain [`SessionHandle::remove_torrent`].
///
/// [`SessionHandle::remove_torrent_with_files`]: irontide::session::SessionHandle::remove_torrent_with_files
/// [`SessionHandle::remove_torrent`]: irontide::session::SessionHandle::remove_torrent
///
/// # Errors
///
/// Returns an error if the request parameters are malformed.
pub async fn delete(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let q = extract_hashes_params(req).await?;
    // Missing flag → preserve files (qBt default + guards M168 behaviour).
    let delete_files = q.delete_files.as_deref().is_some_and(parse_bool_flag);
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        let result = if delete_files {
            state.session.remove_torrent_with_files(id).await
        } else {
            state.session.remove_torrent(id).await
        };
        if let Err(e) = result {
            tracing::warn!(%id, delete_files, error = %e,
                "delete_torrent failed — reported to client as 200 per qBt bulk idempotency");
        }
    }
    // Always 200 — qBt is lenient about per-torrent errors on bulk delete.
    Ok(QbtResponse::ok())
}

/// v0.173.1 Class B + C fix: form-body acceptance + logged session
/// errors on `recheck`. See [`pause`] for the full rationale.
///
/// # Errors
///
/// Returns an error if the request parameters are malformed.
pub async fn recheck(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let q = extract_hashes_params(req).await?;
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        if let Err(e) = state.session.force_recheck(id).await {
            tracing::warn!(%id, error = %e,
                "recheck_torrent failed — reported to client as 200 per qBt bulk idempotency");
        }
    }
    Ok(QbtResponse::ok())
}

/// v0.173.1 Class B + C fix: form-body acceptance + logged session
/// errors on `reannounce`. See [`pause`] for the full rationale.
///
/// # Errors
///
/// Returns an error if the request parameters are malformed.
pub async fn reannounce(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let q = extract_hashes_params(req).await?;
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        if let Err(e) = state.session.force_reannounce(id).await {
            tracing::warn!(%id, error = %e,
                "reannounce_torrent failed — reported to client as 200 per qBt bulk idempotency");
        }
    }
    Ok(QbtResponse::ok())
}

// ── POST /api/v2/torrents/{topPrio,bottomPrio,increasePrio,decreasePrio} ──

/// # Errors
///
/// Returns an error if the request parameters are malformed.
pub async fn top_prio(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let q = extract_hashes_params(req).await?;
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        if let Err(e) = state.session.queue_position_top(id).await {
            tracing::warn!(%id, error = %e, "queue_position_top failed");
        }
    }
    Ok(QbtResponse::ok())
}

/// # Errors
///
/// Returns an error if the request parameters are malformed.
pub async fn bottom_prio(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let q = extract_hashes_params(req).await?;
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        if let Err(e) = state.session.queue_position_bottom(id).await {
            tracing::warn!(%id, error = %e, "queue_position_bottom failed");
        }
    }
    Ok(QbtResponse::ok())
}

/// # Errors
///
/// Returns an error if the request parameters are malformed.
pub async fn increase_prio(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let q = extract_hashes_params(req).await?;
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        if let Err(e) = state.session.queue_position_up(id).await {
            tracing::warn!(%id, error = %e, "queue_position_up failed");
        }
    }
    Ok(QbtResponse::ok())
}

/// # Errors
///
/// Returns an error if the request parameters are malformed.
pub async fn decrease_prio(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let q = extract_hashes_params(req).await?;
    let targets = resolve_hashes(&state, q.hashes.as_deref()).await?;
    for id in targets {
        if let Err(e) = state.session.queue_position_down(id).await {
            tracing::warn!(%id, error = %e, "queue_position_down failed");
        }
    }
    Ok(QbtResponse::ok())
}

// ── GET /api/v2/transferInfo ──────────────────────────────────────────

/// # Errors
///
/// Returns an error if fetching torrent stats fails.
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
        dht_nodes: state.session.dht_node_count().await.unwrap_or(0) as u64,
        dl_rate_limit: -1,
        up_rate_limit: -1,
    };
    Ok(QbtResponse::Json(serde_json::to_value(&info).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

// ── Unit tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, header};

    use super::*;

    /// v0.173.1 Class B: when the URL query carries non-empty `hashes=`,
    /// it wins over the body (qBt `WebUI` v2 convention: query is more
    /// "explicit" than a form-urlencoded body).
    #[tokio::test]
    async fn extract_hashes_params_prefers_query_when_present() {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v2/torrents/pause?hashes=ABC")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("hashes=XYZ"))
            .unwrap();
        let q = extract_hashes_params(req).await.expect("extract ok");
        assert_eq!(q.hashes.as_deref(), Some("ABC"));
    }

    /// v0.173.1 Class B: empty query → fall through to the form body.
    /// This is the *arr integration path.
    #[tokio::test]
    async fn extract_hashes_params_falls_back_to_form_body() {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v2/torrents/pause")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("hashes=XYZ&deleteFiles=true"))
            .unwrap();
        let q = extract_hashes_params(req).await.expect("extract ok");
        assert_eq!(q.hashes.as_deref(), Some("XYZ"));
        assert_eq!(q.delete_files.as_deref(), Some("true"));
    }

    /// v0.173.1 Class B: neither query nor body supplies a recognised
    /// field → 400, not a silent no-op.
    #[tokio::test]
    async fn extract_hashes_params_missing_returns_400() {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v2/torrents/pause")
            .body(Body::empty())
            .unwrap();
        let err = extract_hashes_params(req).await.expect_err("extract err");
        assert!(matches!(err, QbtError::BadRequest(_)));
    }

    /// v0.173.1 Class B: a literal `hashes=` (empty value) in the query
    /// must NOT shadow a non-empty `hashes=` in the body. Some qBt clients
    /// emit empty-value query params when they also carry a body.
    #[tokio::test]
    async fn extract_hashes_params_empty_string_in_query_falls_through_to_body() {
        let req = Request::builder()
            .method("POST")
            .uri("/api/v2/torrents/pause?hashes=")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from("hashes=XYZ"))
            .unwrap();
        let q = extract_hashes_params(req).await.expect("extract ok");
        assert_eq!(
            q.hashes.as_deref(),
            Some("XYZ"),
            "empty-string query hashes must not shadow non-empty body hashes"
        );
    }

    /// v0.173.1 Class B: body over the 64 KiB cap is rejected as a 400,
    /// preventing a hostile client from tying up a worker with a giant
    /// body read.
    #[tokio::test]
    async fn extract_hashes_params_oversized_body_returns_400() {
        // 64 KiB cap — give it 65 KiB of payload so to_bytes errors out.
        let mut big = String::from("hashes=");
        big.push_str(&"a".repeat(65 * 1024));
        let req = Request::builder()
            .method("POST")
            .uri("/api/v2/torrents/pause")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(big))
            .unwrap();
        let err = extract_hashes_params(req).await.expect_err("extract err");
        assert!(matches!(err, QbtError::BadRequest(_)));
    }
}
