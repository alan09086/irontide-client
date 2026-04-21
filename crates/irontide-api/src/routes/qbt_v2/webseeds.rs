//! qBt v2 `GET /api/v2/torrents/webseeds?hash=X` (M171 Lane B).
//!
//! Returns an array of `{"url": "..."}` objects — one per configured
//! web-seed URL. BEP 19 (`url-list`) and BEP 17 (`httpseeds`) URLs are
//! merged into a single list, with BEP 19 entries first (mirrors the
//! wire order in the .torrent file).
//!
//! The endpoint returns an empty array when a torrent has no web seeds.
//! A 404 is returned when the hash is unknown; the handler is silent
//! about "metadata not yet resolved" because magnet-only torrents
//! naturally produce an empty list until metadata arrives — matching
//! qBt's behaviour.

use axum::extract::{Query, State};
use irontide::core::Id20;

use super::response::{QbtError, QbtResponse};
use super::state::QbtState;
use super::torrents::HashQuery;

/// `GET /api/v2/torrents/webseeds?hash=X`.
///
/// # Errors
/// - `QbtError::BadRequest` if `hash` is not a 40-char hex string.
/// - `QbtError::NotFound` if the hash is unknown.
/// - `QbtError::Internal` on session/serialisation failures.
pub async fn list(
    State(state): State<QbtState>,
    Query(q): Query<HashQuery>,
) -> Result<QbtResponse, QbtError> {
    let id = Id20::from_hex(&q.hash)
        .map_err(|e| QbtError::BadRequest(format!("invalid hash: {e}")))?;

    let urls = state
        .session
        .get_web_seeds(id)
        .await
        .map_err(|_| QbtError::NotFound)?;

    let rows: Vec<serde_json::Value> = urls
        .into_iter()
        .map(|u| serde_json::json!({ "url": u }))
        .collect();

    Ok(QbtResponse::Json(serde_json::Value::Array(rows)))
}
