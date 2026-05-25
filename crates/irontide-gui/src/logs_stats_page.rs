//! Logs + Statistics page model (M200).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

const MAX_LOG_ENTRIES: usize = 2000;
const TRANSFER_HISTORY_DAYS: usize = 90;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LogLevel {
    Info = 0,
    Warning = 1,
    Error = 2,
}

impl LogLevel {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Warning => "WARN",
            Self::Error => "ERROR",
        }
    }

    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: SystemTime,
    pub level: LogLevel,
    pub category: String,
    pub message: String,
}

impl LogEntry {
    #[must_use]
    pub fn format_timestamp(&self) -> String {
        let elapsed = self
            .timestamp
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = elapsed.as_secs();
        let hours = (secs / 3600) % 24;
        let mins = (secs / 60) % 60;
        let s = secs % 60;
        format!("{hours:02}:{mins:02}:{s:02}")
    }
}

#[derive(Debug, Clone)]
pub struct LogBuffer {
    inner: Arc<Mutex<VecDeque<LogEntry>>>,
}

impl LogBuffer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(VecDeque::with_capacity(MAX_LOG_ENTRIES))),
        }
    }

    pub fn push(&self, entry: LogEntry) {
        let mut buf = self.inner.lock().unwrap();
        if buf.len() >= MAX_LOG_ENTRIES {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    #[must_use]
    pub fn snapshot(&self) -> Vec<LogEntry> {
        self.inner.lock().unwrap().iter().cloned().collect()
    }

    #[must_use]
    pub fn snapshot_filtered(&self, min_level: LogLevel) -> Vec<LogEntry> {
        self.inner
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.level.as_u8() >= min_level.as_u8())
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }
}

impl Default for LogBuffer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn alert_to_log_entry(alert: &irontide::session::Alert) -> LogEntry {
    let (level, category, message) = match &alert.kind {
        irontide::session::AlertKind::TorrentAdded { name, .. } => {
            (LogLevel::Info, "Torrent", format!("Added: {name}"))
        }
        irontide::session::AlertKind::TorrentRemoved { info_hash } => (
            LogLevel::Info,
            "Torrent",
            format!("Removed: {}", info_hash.to_hex()),
        ),
        irontide::session::AlertKind::TorrentPaused { info_hash } => (
            LogLevel::Info,
            "Torrent",
            format!("Paused: {}", info_hash.to_hex()),
        ),
        irontide::session::AlertKind::TorrentResumed { info_hash } => (
            LogLevel::Info,
            "Torrent",
            format!("Resumed: {}", info_hash.to_hex()),
        ),
        irontide::session::AlertKind::TorrentFinished { info_hash } => (
            LogLevel::Info,
            "Torrent",
            format!("Finished: {}", info_hash.to_hex()),
        ),
        irontide::session::AlertKind::StateChanged {
            info_hash,
            prev_state,
            new_state,
        } => (
            LogLevel::Info,
            "Torrent",
            format!(
                "{}: {prev_state:?} → {new_state:?}",
                &info_hash.to_hex()[..8]
            ),
        ),
        irontide::session::AlertKind::MetadataReceived { name, .. } => (
            LogLevel::Info,
            "Torrent",
            format!("Metadata received: {name}"),
        ),
        irontide::session::AlertKind::MetadataFailed { info_hash } => (
            LogLevel::Warning,
            "Torrent",
            format!("Metadata failed: {}", info_hash.to_hex()),
        ),
        irontide::session::AlertKind::TorrentChecked {
            info_hash,
            pieces_have,
            pieces_total,
        } => (
            LogLevel::Info,
            "Storage",
            format!(
                "Checked {}: {pieces_have}/{pieces_total} pieces",
                &info_hash.to_hex()[..8]
            ),
        ),
        irontide::session::AlertKind::HashFailed {
            info_hash, piece, ..
        } => (
            LogLevel::Warning,
            "Storage",
            format!("Hash failed: {} piece {piece}", &info_hash.to_hex()[..8]),
        ),
        irontide::session::AlertKind::PeerConnected { addr, .. } => {
            (LogLevel::Info, "Peer", format!("Connected: {addr}"))
        }
        irontide::session::AlertKind::PeerDisconnected { addr, reason, .. } => {
            let reason_str = reason.as_deref().unwrap_or("unknown");
            (
                LogLevel::Info,
                "Peer",
                format!("Disconnected: {addr} ({reason_str})"),
            )
        }
        irontide::session::AlertKind::PeerBanned { addr, .. } => {
            (LogLevel::Warning, "Peer", format!("Banned: {addr}"))
        }
        irontide::session::AlertKind::PeerBlocked { addr } => {
            (LogLevel::Info, "Peer", format!("Blocked: {addr}"))
        }
        irontide::session::AlertKind::TrackerReply { url, num_peers, .. } => (
            LogLevel::Info,
            "Tracker",
            format!("{url}: {num_peers} peers"),
        ),
        irontide::session::AlertKind::TrackerWarning { url, message, .. } => {
            (LogLevel::Warning, "Tracker", format!("{url}: {message}"))
        }
        irontide::session::AlertKind::TrackerError { url, message, .. } => {
            (LogLevel::Error, "Tracker", format!("{url}: {message}"))
        }
        irontide::session::AlertKind::DhtBootstrapComplete => {
            (LogLevel::Info, "DHT", "Bootstrap complete".to_string())
        }
        irontide::session::AlertKind::DhtGetPeers {
            info_hash,
            num_peers,
        } => (
            LogLevel::Info,
            "DHT",
            format!("get_peers {}: {num_peers} peers", &info_hash.to_hex()[..8]),
        ),
        irontide::session::AlertKind::ListenSucceeded { port } => (
            LogLevel::Info,
            "Session",
            format!("Listening on port {port}"),
        ),
        irontide::session::AlertKind::ListenFailed { port, message } => (
            LogLevel::Error,
            "Session",
            format!("Listen failed on port {port}: {message}"),
        ),
        irontide::session::AlertKind::TorrentError { info_hash, message } => (
            LogLevel::Error,
            "Torrent",
            format!("{}: {message}", &info_hash.to_hex()[..8]),
        ),
        irontide::session::AlertKind::PerformanceWarning { info_hash, message } => (
            LogLevel::Warning,
            "Performance",
            format!("{}: {message}", &info_hash.to_hex()[..8]),
        ),
        irontide::session::AlertKind::FileError { path, message, .. } => (
            LogLevel::Error,
            "Storage",
            format!("{}: {message}", path.display()),
        ),
        irontide::session::AlertKind::PortMappingSucceeded { port, protocol } => (
            LogLevel::Info,
            "NAT",
            format!("Port mapping succeeded: {protocol} {port}"),
        ),
        irontide::session::AlertKind::PortMappingFailed { port, message, .. } => (
            LogLevel::Error,
            "NAT",
            format!("Port mapping failed on {port}: {message}"),
        ),
        _ => (LogLevel::Info, "Session", format!("{:?}", alert.kind)),
    };

    LogEntry {
        timestamp: alert.timestamp,
        level,
        category: category.to_string(),
        message,
    }
}

#[derive(Debug, Clone, Default)]
pub struct DailyTransfer {
    pub downloaded: u64,
    pub uploaded: u64,
}

#[derive(Debug, Clone)]
pub struct TransferHistory {
    pub days: Vec<DailyTransfer>,
}

impl TransferHistory {
    #[must_use]
    pub fn new() -> Self {
        Self {
            days: vec![DailyTransfer::default(); TRANSFER_HISTORY_DAYS],
        }
    }

    pub fn record_today(&mut self, downloaded: u64, uploaded: u64) {
        if let Some(today) = self.days.last_mut() {
            today.downloaded = downloaded;
            today.uploaded = uploaded;
        }
    }

    pub fn rotate_day(&mut self) {
        if self.days.len() >= TRANSFER_HISTORY_DAYS {
            self.days.remove(0);
        }
        self.days.push(DailyTransfer::default());
    }

    #[must_use]
    pub fn max_transfer(&self) -> u64 {
        self.days
            .iter()
            .map(|d| d.downloaded.max(d.uploaded))
            .max()
            .unwrap_or(0)
    }

    #[must_use]
    pub fn total_downloaded(&self) -> u64 {
        self.days.iter().map(|d| d.downloaded).sum()
    }

    #[must_use]
    pub fn total_uploaded(&self) -> u64 {
        self.days.iter().map(|d| d.uploaded).sum()
    }
}

impl Default for TransferHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct StatCard {
    pub label: String,
    pub value: String,
}

pub struct SessionSnapshot {
    pub total_torrents: usize,
    pub active_torrents: usize,
    pub dl_rate: u64,
    pub ul_rate: u64,
    pub total_downloaded: u64,
    pub total_uploaded: u64,
    pub dht_nodes: usize,
    pub total_peers: usize,
    pub uptime_secs: u64,
}

pub fn build_stat_cards(snap: &SessionSnapshot) -> Vec<StatCard> {
    vec![
        StatCard {
            label: "Torrents".to_string(),
            value: format!("{}", snap.total_torrents),
        },
        StatCard {
            label: "Active".to_string(),
            value: format!("{}", snap.active_torrents),
        },
        StatCard {
            label: "DL Rate".to_string(),
            value: format_speed(snap.dl_rate),
        },
        StatCard {
            label: "UL Rate".to_string(),
            value: format_speed(snap.ul_rate),
        },
        StatCard {
            label: "Downloaded".to_string(),
            value: format_size(snap.total_downloaded),
        },
        StatCard {
            label: "Uploaded".to_string(),
            value: format_size(snap.total_uploaded),
        },
        StatCard {
            label: "Ratio".to_string(),
            value: format_ratio(snap.total_downloaded, snap.total_uploaded),
        },
        StatCard {
            label: "DHT Nodes".to_string(),
            value: format!("{}", snap.dht_nodes),
        },
        StatCard {
            label: "Peers".to_string(),
            value: format!("{}", snap.total_peers),
        },
        StatCard {
            label: "Uptime".to_string(),
            value: format_uptime(snap.uptime_secs),
        },
    ]
}

#[allow(clippy::cast_precision_loss, reason = "display-only formatting")]
fn format_speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec < 1024 {
        format!("{bytes_per_sec} B/s")
    } else if bytes_per_sec < 1_048_576 {
        format!("{:.1} KiB/s", bytes_per_sec as f64 / 1024.0)
    } else if bytes_per_sec < 1_073_741_824 {
        format!("{:.1} MiB/s", bytes_per_sec as f64 / 1_048_576.0)
    } else {
        format!("{:.2} GiB/s", bytes_per_sec as f64 / 1_073_741_824.0)
    }
}

#[allow(clippy::cast_precision_loss, reason = "display-only formatting")]
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1_048_576 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else if bytes < 1_073_741_824 {
        format!("{:.1} MiB", bytes as f64 / 1_048_576.0)
    } else if bytes < 1_099_511_627_776 {
        format!("{:.2} GiB", bytes as f64 / 1_073_741_824.0)
    } else {
        format!("{:.2} TiB", bytes as f64 / 1_099_511_627_776.0)
    }
}

#[allow(clippy::cast_precision_loss, reason = "display-only formatting")]
fn format_ratio(downloaded: u64, uploaded: u64) -> String {
    if downloaded == 0 {
        if uploaded == 0 {
            "0.00".to_string()
        } else {
            "∞".to_string()
        }
    } else {
        format!("{:.2}", uploaded as f64 / downloaded as f64)
    }
}

fn format_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {mins}m")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else {
        format!("{mins}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_buffer_push_and_snapshot() {
        let buf = LogBuffer::new();
        buf.push(LogEntry {
            timestamp: SystemTime::now(),
            level: LogLevel::Info,
            category: "Test".to_string(),
            message: "hello".to_string(),
        });
        assert_eq!(buf.len(), 1);
        let snap = buf.snapshot();
        assert_eq!(snap[0].message, "hello");
    }

    #[test]
    fn log_buffer_capacity_limit() {
        let buf = LogBuffer::new();
        for i in 0..MAX_LOG_ENTRIES + 100 {
            buf.push(LogEntry {
                timestamp: SystemTime::now(),
                level: LogLevel::Info,
                category: "Test".to_string(),
                message: format!("msg {i}"),
            });
        }
        assert_eq!(buf.len(), MAX_LOG_ENTRIES);
        let snap = buf.snapshot();
        assert_eq!(snap[0].message, "msg 100");
    }

    #[test]
    fn log_buffer_filtered_snapshot() {
        let buf = LogBuffer::new();
        buf.push(LogEntry {
            timestamp: SystemTime::now(),
            level: LogLevel::Info,
            category: "Test".to_string(),
            message: "info".to_string(),
        });
        buf.push(LogEntry {
            timestamp: SystemTime::now(),
            level: LogLevel::Warning,
            category: "Test".to_string(),
            message: "warn".to_string(),
        });
        buf.push(LogEntry {
            timestamp: SystemTime::now(),
            level: LogLevel::Error,
            category: "Test".to_string(),
            message: "err".to_string(),
        });
        let warnings = buf.snapshot_filtered(LogLevel::Warning);
        assert_eq!(warnings.len(), 2);
        let errors = buf.snapshot_filtered(LogLevel::Error);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn log_buffer_clear() {
        let buf = LogBuffer::new();
        buf.push(LogEntry {
            timestamp: SystemTime::now(),
            level: LogLevel::Info,
            category: "Test".to_string(),
            message: "a".to_string(),
        });
        buf.clear();
        assert_eq!(buf.len(), 0);
    }

    #[test]
    fn log_level_labels() {
        assert_eq!(LogLevel::Info.label(), "INFO");
        assert_eq!(LogLevel::Warning.label(), "WARN");
        assert_eq!(LogLevel::Error.label(), "ERROR");
    }

    #[test]
    fn transfer_history_record_and_max() {
        let mut h = TransferHistory::new();
        h.record_today(1000, 500);
        assert_eq!(h.max_transfer(), 1000);
        assert_eq!(h.total_downloaded(), 1000);
        assert_eq!(h.total_uploaded(), 500);
    }

    #[test]
    fn transfer_history_rotate() {
        let mut h = TransferHistory::new();
        h.record_today(100, 50);
        h.rotate_day();
        assert_eq!(h.days.len(), TRANSFER_HISTORY_DAYS);
        assert_eq!(h.days.last().unwrap().downloaded, 0);
    }

    #[test]
    fn stat_cards_count() {
        let snap = SessionSnapshot {
            total_torrents: 10,
            active_torrents: 5,
            dl_rate: 1_048_576,
            ul_rate: 524_288,
            total_downloaded: 1_073_741_824,
            total_uploaded: 536_870_912,
            dht_nodes: 200,
            total_peers: 50,
            uptime_secs: 3661,
        };
        let cards = build_stat_cards(&snap);
        assert_eq!(cards.len(), 10);
        assert_eq!(cards[0].label, "Torrents");
        assert_eq!(cards[0].value, "10");
    }

    #[test]
    fn format_speed_units() {
        assert_eq!(format_speed(500), "500 B/s");
        assert_eq!(format_speed(2048), "2.0 KiB/s");
        assert_eq!(format_speed(1_048_576), "1.0 MiB/s");
        assert_eq!(format_speed(1_073_741_824), "1.00 GiB/s");
    }

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(2048), "2.0 KiB");
        assert_eq!(format_size(1_048_576), "1.0 MiB");
        assert_eq!(format_size(1_073_741_824), "1.00 GiB");
        assert_eq!(format_size(1_099_511_627_776), "1.00 TiB");
    }

    #[test]
    fn format_ratio_cases() {
        assert_eq!(format_ratio(0, 0), "0.00");
        assert_eq!(format_ratio(0, 100), "∞");
        assert_eq!(format_ratio(1000, 500), "0.50");
    }

    #[test]
    fn format_uptime_cases() {
        assert_eq!(format_uptime(59), "0m");
        assert_eq!(format_uptime(3661), "1h 1m");
        assert_eq!(format_uptime(90061), "1d 1h 1m");
    }
}
