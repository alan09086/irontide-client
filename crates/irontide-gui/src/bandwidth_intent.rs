//! Goal-based bandwidth limits (M203).
//!
//! Instead of specifying raw rate limits, the user declares an *intent*
//! — "leave 10 Mbps free for video calls" — and the system derives
//! effective download/upload limits from the detected line speed minus
//! the reservation.

use std::path::PathBuf;

fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        std::path::Path::new(&dir).join("irontide")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::Path::new(&home).join(".config").join("irontide")
    } else {
        PathBuf::from("/tmp/irontide")
    }
}

fn config_path() -> PathBuf {
    config_dir().join("bandwidth_intent.json")
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IntentMode {
    #[default]
    Unlimited,
    ManualLimits,
    LeaveReserve,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BandwidthIntent {
    pub mode: IntentMode,
    pub detected_download_kbps: u64,
    pub detected_upload_kbps: u64,
    pub reserved_download_kbps: u64,
    pub reserved_upload_kbps: u64,
}

impl Default for BandwidthIntent {
    fn default() -> Self {
        Self {
            mode: IntentMode::Unlimited,
            detected_download_kbps: 0,
            detected_upload_kbps: 0,
            reserved_download_kbps: 0,
            reserved_upload_kbps: 0,
        }
    }
}

impl BandwidthIntent {
    #[must_use]
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    #[must_use]
    pub fn effective_limits(&self) -> EffectiveLimits {
        match self.mode {
            IntentMode::Unlimited | IntentMode::ManualLimits => EffectiveLimits {
                download_bytes_per_sec: 0,
                upload_bytes_per_sec: 0,
            },
            IntentMode::LeaveReserve => {
                let dl = self
                    .detected_download_kbps
                    .saturating_sub(self.reserved_download_kbps)
                    .saturating_mul(1000)
                    / 8;
                let ul = self
                    .detected_upload_kbps
                    .saturating_sub(self.reserved_upload_kbps)
                    .saturating_mul(1000)
                    / 8;
                EffectiveLimits {
                    download_bytes_per_sec: dl,
                    upload_bytes_per_sec: ul,
                }
            }
        }
    }

    pub fn apply_preset(&mut self, preset: &IntentPreset) {
        self.mode = IntentMode::LeaveReserve;
        self.reserved_download_kbps = preset.reserve_download_kbps;
        self.reserved_upload_kbps = preset.reserve_upload_kbps;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EffectiveLimits {
    pub download_bytes_per_sec: u64,
    pub upload_bytes_per_sec: u64,
}

#[derive(Debug, Clone)]
pub struct IntentPreset {
    pub name: &'static str,
    pub description: &'static str,
    pub reserve_download_kbps: u64,
    pub reserve_upload_kbps: u64,
}

pub static PRESETS: &[IntentPreset] = &[
    IntentPreset {
        name: "Video Call",
        description: "Reserve bandwidth for HD video calls",
        reserve_download_kbps: 10_000,
        reserve_upload_kbps: 5_000,
    },
    IntentPreset {
        name: "Gaming",
        description: "Low-latency reserve for online gaming",
        reserve_download_kbps: 5_000,
        reserve_upload_kbps: 2_000,
    },
    IntentPreset {
        name: "Browsing",
        description: "Light reserve for comfortable web browsing",
        reserve_download_kbps: 3_000,
        reserve_upload_kbps: 1_000,
    },
    IntentPreset {
        name: "Streaming",
        description: "Reserve for 4K video streaming",
        reserve_download_kbps: 25_000,
        reserve_upload_kbps: 2_000,
    },
];

pub fn format_speed_kbps(kbps: u64) -> String {
    if kbps >= 1_000_000 {
        #[allow(clippy::cast_precision_loss, reason = "display-only formatting")]
        let gbps = kbps as f64 / 1_000_000.0;
        format!("{gbps:.1} Gbps")
    } else if kbps >= 1_000 {
        #[allow(clippy::cast_precision_loss, reason = "display-only formatting")]
        let mbps = kbps as f64 / 1_000.0;
        format!("{mbps:.1} Mbps")
    } else {
        format!("{kbps} Kbps")
    }
}

pub fn format_speed_bytes(bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        return "Unlimited".to_string();
    }
    let kib = bytes_per_sec / 1024;
    if kib >= 1024 {
        #[allow(clippy::cast_precision_loss, reason = "display-only formatting")]
        let mib = kib as f64 / 1024.0;
        format!("{mib:.1} MiB/s")
    } else {
        format!("{kib} KiB/s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unlimited() {
        let intent = BandwidthIntent::default();
        assert_eq!(intent.mode, IntentMode::Unlimited);
        let limits = intent.effective_limits();
        assert_eq!(limits.download_bytes_per_sec, 0);
        assert_eq!(limits.upload_bytes_per_sec, 0);
    }

    #[test]
    fn leave_reserve_calculates_limits() {
        let intent = BandwidthIntent {
            mode: IntentMode::LeaveReserve,
            detected_download_kbps: 100_000,
            detected_upload_kbps: 20_000,
            reserved_download_kbps: 10_000,
            reserved_upload_kbps: 5_000,
        };
        let limits = intent.effective_limits();
        // (100000 - 10000) * 1000 / 8 = 11_250_000 bytes/sec
        assert_eq!(limits.download_bytes_per_sec, 11_250_000);
        // (20000 - 5000) * 1000 / 8 = 1_875_000 bytes/sec
        assert_eq!(limits.upload_bytes_per_sec, 1_875_000);
    }

    #[test]
    fn reserve_exceeding_detected_saturates_to_zero() {
        let intent = BandwidthIntent {
            mode: IntentMode::LeaveReserve,
            detected_download_kbps: 5_000,
            detected_upload_kbps: 1_000,
            reserved_download_kbps: 10_000,
            reserved_upload_kbps: 5_000,
        };
        let limits = intent.effective_limits();
        assert_eq!(limits.download_bytes_per_sec, 0);
        assert_eq!(limits.upload_bytes_per_sec, 0);
    }

    #[test]
    fn manual_limits_returns_zero() {
        let intent = BandwidthIntent {
            mode: IntentMode::ManualLimits,
            detected_download_kbps: 100_000,
            detected_upload_kbps: 20_000,
            reserved_download_kbps: 10_000,
            reserved_upload_kbps: 5_000,
        };
        let limits = intent.effective_limits();
        assert_eq!(limits.download_bytes_per_sec, 0);
        assert_eq!(limits.upload_bytes_per_sec, 0);
    }

    #[test]
    fn apply_preset_sets_mode_and_reserve() {
        let mut intent = BandwidthIntent::default();
        intent.apply_preset(&PRESETS[0]);
        assert_eq!(intent.mode, IntentMode::LeaveReserve);
        assert_eq!(intent.reserved_download_kbps, 10_000);
        assert_eq!(intent.reserved_upload_kbps, 5_000);
    }

    #[test]
    fn preset_count() {
        assert_eq!(PRESETS.len(), 4);
    }

    #[test]
    fn format_speed_kbps_units() {
        assert_eq!(format_speed_kbps(500), "500 Kbps");
        assert_eq!(format_speed_kbps(10_000), "10.0 Mbps");
        assert_eq!(format_speed_kbps(1_500_000), "1.5 Gbps");
    }

    #[test]
    fn format_speed_bytes_units() {
        assert_eq!(format_speed_bytes(0), "Unlimited");
        assert_eq!(format_speed_bytes(512 * 1024), "512 KiB/s");
        assert_eq!(format_speed_bytes(5 * 1024 * 1024), "5.0 MiB/s");
    }

    #[test]
    fn state_round_trip() {
        let intent = BandwidthIntent {
            mode: IntentMode::LeaveReserve,
            detected_download_kbps: 100_000,
            reserved_download_kbps: 10_000,
            ..BandwidthIntent::default()
        };
        let json = serde_json::to_string(&intent).unwrap();
        let loaded: BandwidthIntent = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.mode, IntentMode::LeaveReserve);
        assert_eq!(loaded.detected_download_kbps, 100_000);
        assert_eq!(loaded.reserved_download_kbps, 10_000);
    }

    #[test]
    fn all_presets_have_nonzero_reserve() {
        for preset in PRESETS {
            assert!(preset.reserve_download_kbps > 0, "{}", preset.name);
            assert!(preset.reserve_upload_kbps > 0, "{}", preset.name);
        }
    }
}
