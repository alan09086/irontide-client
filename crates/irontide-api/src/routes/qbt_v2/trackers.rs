#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: qBt trackers DTO — tracker tier/peer-counts follow qBt wire-format integer widths"
)]

//! qBt v2 `GET /api/v2/torrents/trackers?hash=X` (M171 Lane B).
//!
//! Returns an array of `QbtTrackerInfo` rows. qBt-compat quirk: the first
//! three entries are **pseudo-trackers** representing the engine's built-in
//! discovery subsystems (DHT, PeX, LSD) — a convention that the `*arr`
//! family relies on to display connectivity breadcrumbs in their UIs.
//!
//! The real trackers (from the torrent's announce list) follow the three
//! pseudo-trackers, matching the qBt wire order.
//!
//! # Status code mapping
//! `TrackerStatus` variants are projected onto qBt's numeric statuses:
//! - `NotContacted` -> 1 (updating)
//! - `Working`      -> 2 (working)
//! - `Error`        -> 4 (error / not working)
//!
//! The pseudo-trackers report `2` (working) when the matching subsystem is
//! enabled in `Settings`, and `0` (disabled) otherwise.

use axum::extract::{Query, State};
use irontide::core::Id20;
use irontide::session::{TrackerInfo, TrackerStatus};
use irontide_format::{is_pseudo_tracker, synthesize_pseudo_trackers};
use serde::Serialize;

use super::response::{QbtError, QbtResponse};
use super::state::QbtState;
use super::torrents::HashQuery;

/// A single row in the `/api/v2/torrents/trackers` response.
///
/// Field names and serialisation order match qBt WebUI v2 verbatim so
/// clients that treat the response as a schema (not just JSON) are happy.
#[derive(Debug, Clone, Serialize)]
pub struct QbtTrackerInfo {
    /// Announce URL, or one of the three pseudo-tracker literals
    /// `"** [DHT] **"` / `"** [PeX] **"` / `"** [LSD] **"`.
    pub url: String,
    /// qBt numeric status — see module-level docs.
    pub status: i32,
    /// Tier index (pseudo-trackers use `-1`; real trackers start at `0`).
    pub tier: i64,
    /// Number of peers known via this tracker. For pseudo-trackers this
    /// is the count produced by the corresponding subsystem (DHT node
    /// count, PeX peer count, LSD peer count). For real trackers it is
    /// `seeders + leechers` from the last scrape.
    pub num_peers: i32,
    /// Number of seeders reported by the last scrape.
    pub num_seeds: i32,
    /// Number of leechers reported by the last scrape.
    pub num_leeches: i32,
    /// Number of completed downloads reported by the last scrape.
    pub num_downloaded: i32,
    /// Human-readable last-announce message (empty until M172 surfaces
    /// per-tracker error strings).
    pub msg: String,
}

/// Builder for the three pseudo-trackers that qBt always emits.
///
/// Wraps `irontide_format::synthesize_pseudo_trackers` and applies the
/// qBt-wire-only `enabled → status` projection (`enabled` → 2 working,
/// disabled → 0 disabled). `dht_node_count`, `pex_peer_count`, and
/// `lsd_peer_count` come from the corresponding subsystem handles; the
/// format helper stores them in `TrackerInfo::seeders` as a count
/// surrogate and we map them onto qBt's `num_peers` column.
fn make_pseudo_trackers(
    dht_enabled: bool,
    pex_enabled: bool,
    lsd_enabled: bool,
    dht_node_count: i32,
    pex_peer_count: i32,
    lsd_peer_count: i32,
) -> [QbtTrackerInfo; 3] {
    let pseudo = synthesize_pseudo_trackers(
        &[],
        dht_node_count.max(0) as usize,
        pex_peer_count.max(0) as usize,
        lsd_peer_count.max(0) as usize,
    );
    debug_assert_eq!(pseudo.len(), 3);
    let mut iter = pseudo.into_iter();
    let dht = pseudo_to_qbt(iter.next().expect("DHT pseudo"), dht_enabled);
    let pex = pseudo_to_qbt(iter.next().expect("PeX pseudo"), pex_enabled);
    let lsd = pseudo_to_qbt(iter.next().expect("LSD pseudo"), lsd_enabled);
    [dht, pex, lsd]
}

/// Map a pseudo-tracker `TrackerInfo` to its qBt wire form, applying the
/// qBt enabled→status projection (working=2, disabled=0) and translating
/// the `usize::MAX` sentinel tier back to qBt's `-1`.
fn pseudo_to_qbt(t: TrackerInfo, enabled: bool) -> QbtTrackerInfo {
    debug_assert!(is_pseudo_tracker(&t), "pseudo_to_qbt called on real tracker");
    QbtTrackerInfo {
        url: t.url,
        status: if enabled { 2 } else { 0 },
        tier: -1,
        num_peers: widen_u32(t.seeders.unwrap_or(0)),
        num_seeds: 0,
        num_leeches: 0,
        num_downloaded: 0,
        msg: String::new(),
    }
}

/// Project an IronTide `TrackerStatus` onto qBt's numeric status code.
const fn tracker_status_code(s: TrackerStatus) -> i32 {
    match s {
        TrackerStatus::NotContacted => 1,
        TrackerStatus::Working => 2,
        TrackerStatus::Error => 4,
    }
}

/// Safely widen a `u32` tracker count to qBt's `i32` wire type. The qBt
/// wire uses signed ints even for counts that are always non-negative;
/// saturating at `i32::MAX` keeps enormous scrape counts from wrapping
/// into negative numbers.
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
const fn widen_u32(v: u32) -> i32 {
    if v > i32::MAX as u32 {
        i32::MAX
    } else {
        v as i32
    }
}

/// `GET /api/v2/torrents/trackers?hash=X`.
///
/// # Errors
/// - `QbtError::BadRequest` if `hash` is not a 40-char hex string.
/// - `QbtError::NotFound` if the hash is unknown.
/// - `QbtError::Internal` on session/serialisation failures.
pub async fn list(
    State(state): State<QbtState>,
    Query(q): Query<HashQuery>,
) -> Result<QbtResponse, QbtError> {
    let id =
        Id20::from_hex(&q.hash).map_err(|e| QbtError::BadRequest(format!("invalid hash: {e}")))?;

    // Probe existence before anything else so unknown hashes produce a
    // 404 (not a 200 with just pseudo-trackers).
    let _ = state
        .session
        .torrent_stats(id)
        .await
        .map_err(|_| QbtError::NotFound)?;

    let settings = state
        .session
        .settings()
        .await
        .map_err(|e| QbtError::Internal(format!("read settings: {e}")))?;

    let dht_node_count = state.session.dht_node_count().await.unwrap_or(0) as i32;

    // M178 (Lane C / TODO-2): real PeX/LSD counts now wired through the
    // session handle. Best-effort: any error keeps the count at zero (the
    // pseudo row still renders, just with `num_peers = 0`).
    let pex_peer_count = state
        .session
        .pex_peer_count(id)
        .await
        .map(|c| widen_u32(u32::try_from(c).unwrap_or(u32::MAX)))
        .unwrap_or(0);
    let lsd_peer_count = state
        .session
        .lsd_peer_count(id)
        .await
        .map(|c| widen_u32(u32::try_from(c).unwrap_or(u32::MAX)))
        .unwrap_or(0);

    let pseudo = make_pseudo_trackers(
        settings.enable_dht,
        settings.enable_pex,
        settings.enable_lsd,
        dht_node_count,
        pex_peer_count,
        lsd_peer_count,
    );

    // Real trackers — silently dropped if the tracker_list call fails
    // (e.g. the torrent was removed mid-flight). qBt behaves the same.
    let real_rows: Vec<QbtTrackerInfo> = match state.session.tracker_list(id).await {
        Ok(list) => list
            .into_iter()
            .map(|t| {
                let seeds = t.seeders.unwrap_or(0);
                let leeches = t.leechers.unwrap_or(0);
                let downloaded = t.downloaded.unwrap_or(0);
                QbtTrackerInfo {
                    url: t.url,
                    status: tracker_status_code(t.status),
                    tier: t.tier as i64,
                    num_peers: widen_u32(seeds.saturating_add(leeches)),
                    num_seeds: widen_u32(seeds),
                    num_leeches: widen_u32(leeches),
                    num_downloaded: widen_u32(downloaded),
                    msg: String::new(),
                }
            })
            .collect(),
        Err(_) => Vec::new(),
    };

    let mut rows: Vec<QbtTrackerInfo> = Vec::with_capacity(3 + real_rows.len());
    rows.extend(pseudo);
    rows.extend(real_rows);

    Ok(QbtResponse::Json(serde_json::to_value(&rows).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudo_trackers_enabled_state() {
        let rows = make_pseudo_trackers(true, true, true, 42, 0, 0);
        assert_eq!(rows[0].url, "** [DHT] **");
        assert_eq!(rows[1].url, "** [PeX] **");
        assert_eq!(rows[2].url, "** [LSD] **");
        for row in &rows {
            assert_eq!(row.status, 2);
            assert_eq!(row.tier, -1);
        }
        assert_eq!(
            rows[0].num_peers, 42,
            "DHT pseudo-tracker wires dht_node_count"
        );
    }

    #[test]
    fn pseudo_trackers_disabled_state() {
        let rows = make_pseudo_trackers(false, false, false, 0, 0, 0);
        for row in &rows {
            assert_eq!(row.status, 0, "{} should be disabled", row.url);
            assert_eq!(row.tier, -1);
            assert_eq!(row.num_peers, 0);
        }
    }

    #[test]
    fn pseudo_trackers_mixed_state() {
        let rows = make_pseudo_trackers(true, false, true, 7, 0, 0);
        assert_eq!(rows[0].status, 2); // DHT enabled
        assert_eq!(
            rows[0].num_peers, 7,
            "DHT num_peers must reflect count even in mixed state"
        );
        assert_eq!(rows[1].status, 0); // PeX disabled
        assert_eq!(rows[2].status, 2); // LSD enabled
    }

    /// M171 D4 / E0.4: DHT pseudo-tracker `num_peers` routes the session's
    /// `dht_node_count()` through the builder. M178 (Lane A): PeX and LSD
    /// args added to the signature with zero placeholders here; Lane C
    /// will wire real counts.
    #[test]
    fn pseudo_trackers_dht_num_peers_matches_count() {
        let rows = make_pseudo_trackers(true, false, false, 123, 0, 0);
        assert_eq!(
            rows[0].num_peers, 123,
            "DHT pseudo-tracker wires dht_node_count"
        );
        assert_eq!(rows[1].num_peers, 0, "PeX placeholder zero in Lane A");
        assert_eq!(rows[2].num_peers, 0, "LSD placeholder zero in Lane A");
    }

    /// M178 D-user-2 / TODO-2: PeX and LSD counts now flow through the
    /// signature. Lane C wires the real session-side counters; this test
    /// asserts the wire mapping ahead of that.
    #[test]
    fn pseudo_trackers_pex_lsd_counts_propagate() {
        let rows = make_pseudo_trackers(true, true, true, 5, 11, 23);
        assert_eq!(rows[0].num_peers, 5);
        assert_eq!(rows[1].num_peers, 11);
        assert_eq!(rows[2].num_peers, 23);
    }

    #[test]
    fn tracker_status_mapping() {
        assert_eq!(tracker_status_code(TrackerStatus::NotContacted), 1);
        assert_eq!(tracker_status_code(TrackerStatus::Working), 2);
        assert_eq!(tracker_status_code(TrackerStatus::Error), 4);
    }

    #[test]
    fn widen_clamps_overflow() {
        assert_eq!(widen_u32(0), 0);
        assert_eq!(widen_u32(1000), 1000);
        #[allow(clippy::cast_sign_loss)]
        let max_i32_as_u32 = i32::MAX as u32;
        assert_eq!(widen_u32(max_i32_as_u32), i32::MAX);
        assert_eq!(widen_u32(u32::MAX), i32::MAX);
    }
}
