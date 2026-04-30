#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: qBt files DTO — file sizes follow qBt's signed-i64 wire format"
)]

//! qBt v2 `GET /api/v2/torrents/files?hash=X` (M170 Lane B).
//!
//! Returns an array of `QbtFile` rows describing each file within a torrent.
//! Matches the qBt `WebUI` v2 schema so `*arr` post-download import loops can
//! enumerate the files they asked `IronTide` to download.
//!
//! # Data sources
//! - `torrent_stats`: probes the hash, gates 404 responses, reports
//!   `has_metadata` so magnet-only torrents don't leak an empty array.
//! - `torrent_info`: file paths + lengths, `piece_length` + `num_pieces` for the
//!   `piece_range` computation.
//! - `torrent_file`: v1 metainfo used to read BEP 47 `attr` per file — pad
//!   files (attr = "p") are filtered out to mirror qBt's behaviour. Hybrid
//!   v1+v2 torrents expose `attr` here. v2-only torrents return `None` from
//!   `torrent_file`, in which case no `attr` data is available and every file
//!   in `torrent_info` is reported.
//! - `file_progress`: bytes-downloaded per file; divided by length to yield
//!   the `progress` fraction.

use axum::extract::{Query, State};
use irontide::core::Id20;
use serde::Serialize;

use super::response::{QbtError, QbtResponse};
use super::state::QbtState;
use super::torrents::HashQuery;

/// A single row in the `/api/v2/torrents/files` response.
///
/// Field names and serialisation order match qBt `WebUI` v2 verbatim so clients
/// that treat the response as a schema (not just JSON) are happy.
#[derive(Debug, Clone, Serialize)]
pub struct QbtFile {
    /// Zero-based index into the torrent's (pad-filtered) file list.
    pub index: usize,
    /// Forward-slash-joined relative path.
    pub name: String,
    /// File size in bytes.
    pub size: u64,
    /// Fraction of this file that is verified on disk, in [0.0, 1.0].
    pub progress: f64,
    /// Per-file priority. Hardcoded to `1` (Normal) until M171 wires
    /// `set_file_priority` into the qBt surface.
    pub priority: i64,
    /// `true` once `progress >= 1.0`.
    pub is_seed: bool,
    /// Inclusive `[first_piece, last_piece]` range covering this file's bytes.
    pub piece_range: [u32; 2],
    /// Fraction of peers that have every piece covering this file. Hardcoded
    /// to `0.0` until M171 wires peer bitfield aggregation into the API layer.
    pub availability: f64,
}

/// `GET /api/v2/torrents/files?hash=X`.
///
/// # Errors
/// - `QbtError::BadRequest` if `hash` is not a 40-char hex string.
/// - `QbtError::NotFound` if the hash is unknown or metadata has not yet
///   arrived (magnet still resolving).
/// - `QbtError::Internal` on session/serialisation failures.
pub async fn list(
    State(state): State<QbtState>,
    Query(q): Query<HashQuery>,
) -> Result<QbtResponse, QbtError> {
    let id =
        Id20::from_hex(&q.hash).map_err(|e| QbtError::BadRequest(format!("invalid hash: {e}")))?;

    // Gate on metadata. `torrent_stats` also distinguishes "unknown hash"
    // (Err) from "known but still resolving" (Ok with has_metadata=false).
    // Both map to 404, matching qBt's behaviour for an unfinished magnet.
    let stats = state
        .session
        .torrent_stats(id)
        .await
        .map_err(|_| QbtError::NotFound)?;
    if !stats.has_metadata {
        return Err(QbtError::NotFound);
    }

    let info = state
        .session
        .torrent_info(id)
        .await
        .map_err(|_| QbtError::NotFound)?;

    // Pull pad-file attributes from the v1 metainfo when available. Hybrid
    // and v1-only torrents expose `FileEntry.attr`; v2-only torrents return
    // `None` here. In the latter case we skip the pad filter — v2 pad files
    // are rare enough in *arr-land that the cost of surfacing them is lower
    // than the cost of a round-trip through torrent_file_v2.
    let pad_flags: Vec<bool> = match state.session.torrent_file(id).await {
        Ok(Some(meta)) => match meta.info.files.as_ref() {
            Some(entries) => entries
                .iter()
                .map(|e| e.attr.as_deref() == Some("p"))
                .collect(),
            None => Vec::new(),
        },
        _ => Vec::new(),
    };

    let progress_bytes = state
        .session
        .file_progress(id)
        .await
        .map_err(|e| QbtError::Internal(format!("file_progress: {e}")))?;

    let piece_length = info.piece_length;
    let max_piece = info.num_pieces.saturating_sub(1);

    // Pre-compute cumulative file offsets once; piece_range for each file
    // is an O(1) lookup after this.
    let mut offsets: Vec<u64> = Vec::with_capacity(info.files.len());
    let mut cursor: u64 = 0;
    for f in &info.files {
        offsets.push(cursor);
        cursor = cursor.saturating_add(f.length);
    }

    let mut rows: Vec<QbtFile> = Vec::with_capacity(info.files.len());
    for (raw_idx, file) in info.files.iter().enumerate() {
        // Skip BEP 47 pad files when the v1 metainfo told us about them.
        // Pad files are used only for piece-alignment padding; qBt and the
        // *arr clients never want to see them in a file listing.
        if pad_flags.get(raw_idx).copied().unwrap_or(false) {
            continue;
        }

        let start = offsets.get(raw_idx).copied().unwrap_or(0);
        let (first_piece, last_piece) =
            piece_range_for(start, file.length, piece_length, max_piece);

        let downloaded = progress_bytes.get(raw_idx).copied().unwrap_or(0);
        let progress = if file.length == 0 {
            1.0
        } else {
            (downloaded as f64 / file.length as f64).clamp(0.0, 1.0)
        };

        let name = file
            .path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        rows.push(QbtFile {
            index: rows.len(),
            name,
            size: file.length,
            progress,
            // FIXME(M171): per-file priority
            priority: 1,
            is_seed: progress >= 1.0,
            piece_range: [first_piece, last_piece],
            // FIXME(M171): peer bitfield aggregation
            availability: 0.0,
        });
    }

    Ok(QbtResponse::Json(serde_json::to_value(&rows).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

/// Compute the inclusive `(first_piece, last_piece)` range that covers the
/// file occupying bytes `[start, start + len)`.
///
/// Mirrors `irontide_storage::FileMap::piece_range` so we don't have to
/// round-trip through the storage crate from the API layer. O(1) arithmetic.
fn piece_range_for(start: u64, len: u64, piece_length: u64, max_piece: u32) -> (u32, u32) {
    if piece_length == 0 {
        return (0, 0);
    }
    let first_u64 = start / piece_length;
    let first = u32::try_from(first_u64).unwrap_or(max_piece).min(max_piece);
    if len == 0 {
        return (first, first);
    }
    let last_byte = start.saturating_add(len).saturating_sub(1);
    let last_u64 = last_byte / piece_length;
    let last = u32::try_from(last_u64).unwrap_or(max_piece).min(max_piece);
    (first, last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piece_range_single_file_multiple_pieces() {
        // 1024 bytes / 256-byte pieces = 4 pieces, one file.
        assert_eq!(piece_range_for(0, 1024, 256, 3), (0, 3));
    }

    #[test]
    fn piece_range_multi_file_chain() {
        // Files [400, 300, 200], piece_length = 256, total 900 → 4 pieces.
        // File 0: 0..400  → pieces (0, 1)
        // File 1: 400..700 → pieces (1, 2)
        // File 2: 700..900 → pieces (2, 3)
        assert_eq!(piece_range_for(0, 400, 256, 3), (0, 1));
        assert_eq!(piece_range_for(400, 300, 256, 3), (1, 2));
        assert_eq!(piece_range_for(700, 200, 256, 3), (2, 3));
    }

    #[test]
    fn piece_range_zero_length_file_collapses() {
        // Zero-length file sitting at piece boundary 100 should return
        // a degenerate (1, 1) range, not a negative one.
        assert_eq!(piece_range_for(100, 0, 100, 2), (1, 1));
    }

    #[test]
    fn piece_range_clamped_to_max() {
        // start * piece_length overflow via u32 cast must clamp to max_piece.
        // 1_000_000 bytes, 100-byte pieces → 10_000 pieces. Asking for a
        // synthetic file past the end still returns a valid u32.
        let (_, last) = piece_range_for(0, 1_000_000, 100, 9_999);
        assert!(last <= 9_999);
    }
}
