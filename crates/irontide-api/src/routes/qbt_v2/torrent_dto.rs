#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: qBt wire format uses signed-i64 for unsigned counters; truncation/sign-loss is intentional protocol-level behaviour matched to qBittorrent reference"
)]

//! qBt v2 torrent DTO shapes (M168 Task 9).
//!
//! `IronTide`'s internal torrent model is richer than qBt's — the qBt DTOs
//! are a flattened *projection* that pick the fields `*arr` actually reads
//! and wrap `IronTide`'s state enum into qBt's string-based state enum.
//!
//! # Mappings
//! - `QbtTorrent` ← `TorrentStats`: the row shape returned by `torrents/info`.
//! - `QbtTorrentProperties` ← `TorrentStats`: detailed view for `torrents/properties`.
//! - `QbtTransferInfo` ← `SessionStats`: session-wide counters for `transferInfo`.
//! - `qbt_state_string`: maps `TorrentState` + rates + progress onto the qBt
//!   canonical state strings (downloading, stalledDL, uploading, pausedUP,
//!   checkingUP, metaDL, error, etc.).

use irontide::session::{TorrentState, TorrentStats};
use serde::{Deserialize, Serialize};

/// qBt "infinite" sentinel for `eta` when the remaining time is unknown.
/// This is 100 days in seconds — matches what real qBt returns.
pub const QBT_ETA_INFINITE: i64 = 8_640_000;

/// Map `IronTide`'s `TorrentState` + rates + progress onto a qBt state string.
///
/// qBt's state enum is richer than a plain enum because it reflects dynamics:
/// a Downloading torrent with zero down-rate is `stalledDL`, not
/// `downloading`. Real qBt's Web UI and *arr clients depend on these strings.
#[must_use] 
pub fn qbt_state_string(s: &TorrentStats) -> &'static str {
    // Queued takes precedence (system-managed pause — maps to qBt's queued states).
    if s.is_queued {
        return if s.progress >= 1.0 {
            "queuedUP"
        } else {
            "queuedDL"
        };
    }
    // Paused takes precedence over other flags (real qBt behaviour).
    if s.is_paused {
        return if s.progress >= 1.0 {
            "pausedUP"
        } else {
            "pausedDL"
        };
    }
    // Errors surface next; error string is non-empty only on true failure.
    if !s.error.is_empty() {
        return "error";
    }
    match s.state {
        TorrentState::FetchingMetadata => "metaDL",
        TorrentState::Checking => {
            if s.progress >= 1.0 {
                "checkingUP"
            } else {
                "checkingDL"
            }
        }
        TorrentState::Downloading => {
            if s.download_rate > 0 {
                "downloading"
            } else {
                "stalledDL"
            }
        }
        TorrentState::Complete | TorrentState::Seeding => {
            if s.upload_rate > 0 {
                "uploading"
            } else {
                "stalledUP"
            }
        }
        TorrentState::Paused | TorrentState::Queued | TorrentState::Stopped => {
            // Already handled above; kept for match exhaustiveness.
            "pausedDL"
        }
        TorrentState::Sharing => "forcedUP",
    }
}

/// A single row in the `GET /api/v2/torrents/info` response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QbtTorrent {
    pub hash: String,
    pub name: String,
    pub size: u64,
    pub total_size: u64,
    pub progress: f64,
    pub dlspeed: u64,
    pub upspeed: u64,
    pub num_seeds: i64,
    pub num_leechs: i64,
    pub num_complete: i64,
    pub num_incomplete: i64,
    pub ratio: f64,
    pub eta: i64,
    pub downloaded: u64,
    pub uploaded: u64,
    pub amount_left: u64,
    pub state: String,
    pub save_path: String,
    pub magnet_uri: String,
    pub category: String,
    pub tags: String,
    pub auto_tmm: bool,
    pub priority: i64,
    pub added_on: i64,
    pub completion_on: i64,
    pub last_activity: i64,
    pub time_active: i64,
}

impl From<&TorrentStats> for QbtTorrent {
    fn from(s: &TorrentStats) -> Self {
        let hash = s.info_hashes.v1.map(|h| h.to_hex()).unwrap_or_default();
        let num_leechs = (s.num_peers as i64).saturating_sub(s.num_seeds as i64);
        let amount_left = s.total.saturating_sub(s.total_done);
        let ratio = if s.all_time_download > 0 {
            (s.all_time_upload as f64) / (s.all_time_download as f64)
        } else {
            0.0
        };
        // ETA: naive total_wanted / download_rate; qBt returns 8_640_000 when
        // download_rate is zero or progress is complete.
        let eta = if s.download_rate == 0 || s.progress >= 1.0 {
            QBT_ETA_INFINITE
        } else {
            let remaining = s.total_wanted.saturating_sub(s.total_wanted_done);
            (remaining / s.download_rate.max(1)) as i64
        };
        let last_activity = s.last_upload.max(s.last_download);

        Self {
            hash: hash.clone(),
            name: s.name.clone(),
            size: s.total,
            total_size: s.total,
            progress: f64::from(s.progress),
            dlspeed: s.download_rate,
            upspeed: s.upload_rate,
            num_seeds: s.num_seeds as i64,
            num_leechs,
            num_complete: i64::from(s.num_complete),
            num_incomplete: i64::from(s.num_incomplete),
            ratio,
            eta,
            downloaded: s.all_time_download,
            uploaded: s.all_time_upload,
            amount_left,
            state: qbt_state_string(s).to_owned(),
            save_path: s.save_path.clone(),
            magnet_uri: if hash.is_empty() {
                String::new()
            } else {
                format!("magnet:?xt=urn:btih:{hash}")
            },
            // M170: surface the resolved qBt-compat category label; empty
            // string (not `null`) when absent matches qBt's JSON shape.
            category: s.category.clone().unwrap_or_default(),
            // M171: qBt wire convention is a comma-separated string (not
            // a JSON array) for `tags`. Empty vec renders as empty string,
            // matching qBt's untagged-torrent shape.
            tags: s.tags.join(","),
            auto_tmm: false,
            priority: i64::from(s.queue_position),
            added_on: s.added_time,
            completion_on: s.completed_time,
            last_activity,
            time_active: s.active_duration,
        }
    }
}

/// Response shape for `GET /api/v2/torrents/properties?hash=X`.
///
/// Superset of `QbtTorrent`: fields relevant to torrent detail UI.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QbtTorrentProperties {
    pub save_path: String,
    pub creation_date: i64,
    pub piece_size: u64,
    pub comment: String,
    pub total_wasted: u64,
    pub total_uploaded: u64,
    pub total_uploaded_session: u64,
    pub total_downloaded: u64,
    pub total_downloaded_session: u64,
    pub up_limit: i64,
    pub dl_limit: i64,
    pub time_elapsed: i64,
    pub seeding_time: i64,
    pub nb_connections: i64,
    pub nb_connections_limit: i64,
    pub share_ratio: f64,
    pub addition_date: i64,
    pub completion_date: i64,
    pub created_by: String,
    pub dl_speed_avg: u64,
    pub dl_speed: u64,
    pub eta: i64,
    pub last_seen: i64,
    pub peers: i64,
    pub peers_total: i64,
    pub pieces_have: i64,
    pub pieces_num: i64,
    pub reannounce: i64,
    pub seeds: i64,
    pub seeds_total: i64,
    pub total_size: u64,
    pub up_speed_avg: u64,
    pub up_speed: u64,
}

impl From<&TorrentStats> for QbtTorrentProperties {
    fn from(s: &TorrentStats) -> Self {
        let share_ratio = if s.all_time_download > 0 {
            (s.all_time_upload as f64) / (s.all_time_download as f64)
        } else {
            0.0
        };
        let eta = if s.download_rate == 0 || s.progress >= 1.0 {
            QBT_ETA_INFINITE
        } else {
            let remaining = s.total_wanted.saturating_sub(s.total_wanted_done);
            (remaining / s.download_rate.max(1)) as i64
        };
        Self {
            save_path: s.save_path.clone(),
            // qBt sentinel for "unknown/unset creation_date" is -1, not 0.
            // Magnet-added torrents without resolved metadata surface as
            // -1 so clients don't confuse "epoch 1970" with "unknown".
            creation_date: s.creation_date.unwrap_or(-1),
            // Lane A populates piece_size from Lengths::piece_length() once
            // metadata is available. Pre-metadata it's 0, which matches
            // qBt's own behaviour for still-resolving magnets.
            piece_size: s.piece_size,
            comment: String::new(),
            total_wasted: s.total_failed_bytes,
            total_uploaded: s.all_time_upload,
            total_uploaded_session: s.total_upload,
            total_downloaded: s.all_time_download,
            total_downloaded_session: s.total_download,
            up_limit: -1,
            dl_limit: -1,
            time_elapsed: s.active_duration,
            seeding_time: s.seeding_duration,
            nb_connections: s.num_connections as i64,
            nb_connections_limit: s.connections_limit as i64,
            share_ratio,
            addition_date: s.added_time,
            completion_date: s.completed_time,
            // Empty string (not `null`) when absent — qBt serialises this
            // way in its own "properties" response.
            created_by: s.created_by.clone().unwrap_or_default(),
            dl_speed_avg: s.download_rate,
            dl_speed: s.download_rate,
            eta,
            last_seen: s.last_seen_complete,
            peers: s.num_peers.saturating_sub(s.num_seeds) as i64,
            peers_total: s.list_peers as i64,
            pieces_have: i64::from(s.pieces_have),
            pieces_num: i64::from(s.pieces_total),
            reannounce: 0,
            seeds: s.num_seeds as i64,
            seeds_total: s.list_seeds as i64,
            total_size: s.total,
            up_speed_avg: s.upload_rate,
            up_speed: s.upload_rate,
        }
    }
}

/// Response shape for `GET /api/v2/transferInfo`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QbtTransferInfo {
    pub dl_info_speed: u64,
    pub dl_info_data: u64,
    pub up_info_speed: u64,
    pub up_info_data: u64,
    pub connection_status: String,
    pub dht_nodes: u64,
    pub dl_rate_limit: i64,
    pub up_rate_limit: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use irontide::core::{Id20, InfoHashes};

    fn sample_stats() -> TorrentStats {
        TorrentStats {
            info_hashes: InfoHashes::v1_only(Id20::from([0xAB; 20])),
            name: "Sample".into(),
            total: 1_000_000,
            total_done: 500_000,
            total_wanted: 1_000_000,
            total_wanted_done: 500_000,
            progress: 0.5,
            download_rate: 1024,
            upload_rate: 512,
            num_peers: 10,
            num_seeds: 4,
            num_complete: 20,
            num_incomplete: 5,
            all_time_download: 2_000_000,
            all_time_upload: 1_000_000,
            added_time: 1_700_000_000,
            ..TorrentStats::default()
        }
    }

    #[test]
    fn qbt_torrent_from_torrent_stats_maps_hash_as_lowercase_hex() {
        let s = sample_stats();
        let t = QbtTorrent::from(&s);
        assert_eq!(t.hash.len(), 40);
        assert!(t.hash.chars().all(|c| c.is_ascii_hexdigit()));
        // Lowercase only.
        assert!(t.hash.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn qbt_torrent_size_equals_total_size() {
        let s = sample_stats();
        let t = QbtTorrent::from(&s);
        assert_eq!(t.size, 1_000_000);
        assert_eq!(t.total_size, 1_000_000);
    }

    #[test]
    fn qbt_torrent_progress_as_f64_zero_to_one() {
        let s = sample_stats();
        let t = QbtTorrent::from(&s);
        assert!((t.progress - 0.5).abs() < 1e-6);
    }

    #[test]
    fn qbt_state_string_downloading_with_rate() {
        let mut s = sample_stats();
        s.state = TorrentState::Downloading;
        s.download_rate = 100;
        assert_eq!(qbt_state_string(&s), "downloading");
    }

    #[test]
    fn qbt_state_string_downloading_stalled() {
        let mut s = sample_stats();
        s.state = TorrentState::Downloading;
        s.download_rate = 0;
        assert_eq!(qbt_state_string(&s), "stalledDL");
    }

    #[test]
    fn qbt_state_string_seeding_uploading() {
        let mut s = sample_stats();
        s.state = TorrentState::Seeding;
        s.upload_rate = 100;
        s.progress = 1.0;
        assert_eq!(qbt_state_string(&s), "uploading");
    }

    #[test]
    fn qbt_state_string_paused_pre_completion() {
        let mut s = sample_stats();
        s.is_paused = true;
        s.progress = 0.3;
        assert_eq!(qbt_state_string(&s), "pausedDL");
        s.progress = 1.0;
        assert_eq!(qbt_state_string(&s), "pausedUP");
    }

    #[test]
    fn qbt_state_string_queued_dl() {
        let mut s = sample_stats();
        s.is_queued = true;
        s.progress = 0.3;
        assert_eq!(qbt_state_string(&s), "queuedDL");
    }

    #[test]
    fn qbt_state_string_queued_up() {
        let mut s = sample_stats();
        s.is_queued = true;
        s.progress = 1.0;
        assert_eq!(qbt_state_string(&s), "queuedUP");
    }

    #[test]
    fn qbt_state_string_error_fallback() {
        let mut s = sample_stats();
        s.error = "disk full".into();
        assert_eq!(qbt_state_string(&s), "error");
    }

    // M171 C2: QbtTorrent.tags mirrors TorrentStats.tags as comma-joined string.

    #[test]
    fn qbt_torrent_tags_empty_when_no_tags() {
        let s = sample_stats();
        let t = QbtTorrent::from(&s);
        assert_eq!(t.tags, "");
    }

    #[test]
    fn qbt_torrent_tags_comma_joined_from_stats() {
        let mut s = sample_stats();
        s.tags = vec!["sonarr".to_string(), "kids".to_string()];
        let t = QbtTorrent::from(&s);
        assert_eq!(t.tags, "sonarr,kids");
    }

    #[test]
    fn qbt_torrent_tags_single_value() {
        let mut s = sample_stats();
        s.tags = vec!["only".to_string()];
        let t = QbtTorrent::from(&s);
        assert_eq!(t.tags, "only");
    }

    #[test]
    fn qbt_torrent_priority_from_queue_position() {
        let mut s = sample_stats();
        s.queue_position = 5;
        let t = QbtTorrent::from(&s);
        assert_eq!(t.priority, 5);
    }

    #[test]
    fn qbt_torrent_priority_minus_one_when_unqueued() {
        let mut s = sample_stats();
        s.queue_position = -1;
        let t = QbtTorrent::from(&s);
        assert_eq!(t.priority, -1);
    }
}
