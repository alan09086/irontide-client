//! qBt v2 piece-introspection endpoints (M171 Lane B).
//!
//! - `GET /api/v2/torrents/pieceStates?hash=X` returns a JSON array of
//!   per-piece qBt state codes (`0`/`1`/`2`).
//! - `GET /api/v2/torrents/pieceHashes?hash=X&offset=...&limit=...`
//!   (added in B4) returns a paginated JSON array of piece hash hex
//!   strings.

use axum::extract::{Query, State};
use irontide::core::Id20;

use super::response::{QbtError, QbtResponse};
use super::state::QbtState;
use super::torrents::HashQuery;

/// `GET /api/v2/torrents/pieceStates?hash=X`.
///
/// # Errors
/// - `QbtError::BadRequest` if `hash` is not a 40-char hex string.
/// - `QbtError::NotFound` if the hash is unknown, or the torrent has
///   no metadata yet (piece count unknown).
/// - `QbtError::Internal` on session/serialisation failures.
pub async fn states(
    State(state): State<QbtState>,
    Query(q): Query<HashQuery>,
) -> Result<QbtResponse, QbtError> {
    let id = Id20::from_hex(&q.hash)
        .map_err(|e| QbtError::BadRequest(format!("invalid hash: {e}")))?;

    let states = state
        .session
        .get_piece_states(id)
        .await
        .map_err(|_| QbtError::NotFound)?;

    // E0.9: a fresh magnet with unresolved metadata reports "no pieces"
    // → 404. Without this guard the endpoint would return `[]`, which
    // clients can't distinguish from a zero-piece torrent.
    if states.is_empty() {
        return Err(QbtError::NotFound);
    }

    Ok(QbtResponse::Json(
        serde_json::to_value(&states)
            .map_err(|e| QbtError::Internal(format!("serialise: {e}")))?,
    ))
}
