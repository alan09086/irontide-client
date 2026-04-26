//! qBt v2 piece-introspection endpoints (M171 Lane B).
//!
//! - `GET /api/v2/torrents/pieceStates?hash=X` returns a JSON array of
//!   per-piece qBt state codes (`0`/`1`/`2`).
//! - `GET /api/v2/torrents/pieceHashes?hash=X&offset=...&limit=...`
//!   returns a paginated JSON array of per-piece hash hex strings. v1 /
//!   hybrid torrents yield SHA-1 (40-char hex); v2-only yield SHA-256
//!   (64-char hex).
//!
//! Pagination on `pieceHashes` is essential: a 4 TiB torrent has ~1M
//! pieces, which would be ~42 MiB of JSON without it. Defaults cap at
//! 4096 hashes per request; `limit` is clamped server-side to 16384 so
//! a malicious `?limit=9999999` query can't exhaust memory.

use axum::extract::{Query, State};
use irontide::core::Id20;
use serde::Deserialize;

use super::response::{QbtError, QbtResponse};
use super::state::QbtState;
use super::torrents::HashQuery;

/// Default page size when `limit=` is omitted.
const DEFAULT_PIECE_HASH_LIMIT: u32 = 4096;

/// Hard cap on `limit=` — larger values are silently truncated.
const MAX_PIECE_HASH_LIMIT: u32 = 16_384;

/// Query parameters for the paginated `pieceHashes` endpoint.
#[derive(Deserialize)]
pub struct PieceHashesQuery {
    /// Torrent info-hash (40 hex chars).
    pub hash: String,
    /// Zero-based index of the first hash to return (default 0).
    #[serde(default)]
    pub offset: Option<u32>,
    /// Maximum number of hashes to return
    /// (default [`DEFAULT_PIECE_HASH_LIMIT`], capped at
    /// [`MAX_PIECE_HASH_LIMIT`]).
    #[serde(default)]
    pub limit: Option<u32>,
}

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
    let id =
        Id20::from_hex(&q.hash).map_err(|e| QbtError::BadRequest(format!("invalid hash: {e}")))?;

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

    Ok(QbtResponse::Json(serde_json::to_value(&states).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

/// `GET /api/v2/torrents/pieceHashes?hash=X&offset=...&limit=...`.
///
/// `offset` defaults to `0`; `limit` defaults to
/// [`DEFAULT_PIECE_HASH_LIMIT`] and is capped at
/// [`MAX_PIECE_HASH_LIMIT`]. An `offset` past the end of the hash list
/// returns an empty array (not a 404).
///
/// # Errors
/// - `QbtError::BadRequest` if `hash` is not a 40-char hex string.
/// - `QbtError::NotFound` if the hash is unknown, or the torrent has
///   no metadata yet.
/// - `QbtError::Internal` on session/serialisation failures.
pub async fn hashes(
    State(state): State<QbtState>,
    Query(q): Query<PieceHashesQuery>,
) -> Result<QbtResponse, QbtError> {
    let id =
        Id20::from_hex(&q.hash).map_err(|e| QbtError::BadRequest(format!("invalid hash: {e}")))?;
    let offset = q.offset.unwrap_or(0);
    let limit = q
        .limit
        .unwrap_or(DEFAULT_PIECE_HASH_LIMIT)
        .min(MAX_PIECE_HASH_LIMIT);

    // We need to distinguish unknown-hash (→ 404) from
    // known-hash-but-empty-hash-list-due-to-paging (→ 200 []). The
    // session returns `TorrentNotFound` only for the former; an
    // unknown torrent therefore becomes 404 here, while an out-of-
    // range offset on a real torrent becomes 200 [] (the engine slices
    // and returns an empty page).
    let hashes = state
        .session
        .get_piece_hashes(id, offset, limit)
        .await
        .map_err(|_| QbtError::NotFound)?;

    Ok(QbtResponse::Json(serde_json::to_value(&hashes).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_constants_are_sensible() {
        const _: () = assert!(DEFAULT_PIECE_HASH_LIMIT > 0);
        const _: () = assert!(DEFAULT_PIECE_HASH_LIMIT <= MAX_PIECE_HASH_LIMIT);
        // 4096 default × 40-char SHA-1 hex ≈ 160 KiB per page —
        // comfortably under 1 MiB on the wire.
        const _: () = assert!(DEFAULT_PIECE_HASH_LIMIT <= 4096);
        // 16384 max × 64-char SHA-256 hex ≈ 1 MiB — our hard ceiling.
        const _: () = assert!(MAX_PIECE_HASH_LIMIT <= 16_384);
    }
}
