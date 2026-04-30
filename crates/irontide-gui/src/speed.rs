use std::collections::VecDeque;

enum SpeedTier {
    Raw,
    Hourly,
    Daily,
}

const RAW_CAP: usize = 600;
const HOURLY_CAP: usize = 720;
const DAILY_CAP: usize = 1440;
const HOURLY_INTERVAL: u64 = 10;
const DAILY_INTERVAL: u64 = 120;

#[derive(Clone, Copy)]
pub struct SpeedSample {
    pub download: u64,
    pub upload: u64,
}

pub struct SpeedHistory {
    raw: VecDeque<SpeedSample>,
    hourly: VecDeque<SpeedSample>,
    daily: VecDeque<SpeedSample>,
    hourly_acc_dl: u64,
    hourly_acc_ul: u64,
    hourly_count: u64,
    daily_acc_dl: u64,
    daily_acc_ul: u64,
    daily_count: u64,
}

impl SpeedHistory {
    #[must_use]
    pub fn new() -> Self {
        Self {
            raw: VecDeque::with_capacity(RAW_CAP),
            hourly: VecDeque::with_capacity(HOURLY_CAP),
            daily: VecDeque::with_capacity(DAILY_CAP),
            hourly_acc_dl: 0,
            hourly_acc_ul: 0,
            hourly_count: 0,
            daily_acc_dl: 0,
            daily_acc_ul: 0,
            daily_count: 0,
        }
    }

    pub fn push(&mut self, dl: u64, ul: u64) {
        if self.raw.len() == RAW_CAP {
            self.raw.pop_front();
        }
        self.raw.push_back(SpeedSample {
            download: dl,
            upload: ul,
        });

        self.hourly_acc_dl += dl;
        self.hourly_acc_ul += ul;
        self.hourly_count += 1;
        if self.hourly_count >= HOURLY_INTERVAL {
            if self.hourly.len() == HOURLY_CAP {
                self.hourly.pop_front();
            }
            self.hourly.push_back(SpeedSample {
                download: self.hourly_acc_dl / HOURLY_INTERVAL,
                upload: self.hourly_acc_ul / HOURLY_INTERVAL,
            });
            self.hourly_acc_dl = 0;
            self.hourly_acc_ul = 0;
            self.hourly_count = 0;
        }

        self.daily_acc_dl += dl;
        self.daily_acc_ul += ul;
        self.daily_count += 1;
        if self.daily_count >= DAILY_INTERVAL {
            if self.daily.len() == DAILY_CAP {
                self.daily.pop_front();
            }
            self.daily.push_back(SpeedSample {
                download: self.daily_acc_dl / DAILY_INTERVAL,
                upload: self.daily_acc_ul / DAILY_INTERVAL,
            });
            self.daily_acc_dl = 0;
            self.daily_acc_ul = 0;
            self.daily_count = 0;
        }
    }

    fn auto_tier(&self) -> SpeedTier {
        if self.raw.len() < RAW_CAP {
            SpeedTier::Raw
        } else if self.hourly.len() < HOURLY_CAP {
            SpeedTier::Hourly
        } else {
            SpeedTier::Daily
        }
    }

    #[must_use]
    pub fn flatten_auto(&self) -> (Vec<i32>, Vec<i32>) {
        let buf = match self.auto_tier() {
            SpeedTier::Raw => &self.raw,
            SpeedTier::Hourly => &self.hourly,
            SpeedTier::Daily => &self.daily,
        };
        let samples: Vec<SpeedSample> = buf.iter().copied().collect();
        scale_samples(&samples)
    }

    #[must_use]
    pub fn max_rate(&self) -> u64 {
        let buf = match self.auto_tier() {
            SpeedTier::Raw => &self.raw,
            SpeedTier::Hourly => &self.hourly,
            SpeedTier::Daily => &self.daily,
        };
        buf.iter()
            .map(|s| s.download.max(s.upload))
            .max()
            .unwrap_or(0)
    }

    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "sample counts are bounded by VecDeque caps (≤1440), always fits u64"
    )]
    pub fn elapsed_label(&self) -> String {
        let tier = self.auto_tier();
        let count = match tier {
            SpeedTier::Raw => self.raw.len(),
            SpeedTier::Hourly => self.hourly.len(),
            SpeedTier::Daily => self.daily.len(),
        };
        if count == 0 {
            return String::new();
        }
        let count = count as u64;
        let total_secs = match tier {
            SpeedTier::Raw => count / 2,
            SpeedTier::Hourly => count * 5,
            SpeedTier::Daily => count * 60,
        };
        format_duration_short(total_secs)
    }
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "permille scaling: values bounded to 0..1000, f64 intermediate is exact at this magnitude"
)]
fn scale_samples(samples: &[SpeedSample]) -> (Vec<i32>, Vec<i32>) {
    if samples.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let max_val = samples
        .iter()
        .map(|s| s.download.max(s.upload))
        .max()
        .unwrap_or(0);
    if max_val == 0 {
        let flat: Vec<i32> = vec![1000; samples.len()];
        return (flat.clone(), flat);
    }
    let dl: Vec<i32> = samples
        .iter()
        .map(|s| 1000 - (s.download as f64 / max_val as f64 * 1000.0) as i32)
        .collect();
    let ul: Vec<i32> = samples
        .iter()
        .map(|s| 1000 - (s.upload as f64 / max_val as f64 * 1000.0) as i32)
        .collect();
    (dl, ul)
}

#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "SVG path coordinates: values bounded to 0..viewbox (≤1000), f64 intermediate is exact"
)]
pub fn build_path_commands(scaled: &[i32], viewbox_w: i32, viewbox_h: i32) -> String {
    if scaled.is_empty() || viewbox_w == 0 {
        return String::new();
    }
    let n = scaled.len();
    let mut out = String::with_capacity(n * 15);
    // Right-align: if fewer points than the view width, start from the
    // right edge so the graph grows from right to left.
    let step = if n > 1 {
        f64::from(viewbox_w) / (n - 1) as f64
    } else {
        0.0
    };
    let x_offset = if n > 1 {
        f64::from(viewbox_w) - step * (n - 1) as f64
    } else {
        f64::from(viewbox_w)
    };
    for (i, &y) in scaled.iter().enumerate() {
        let x = (x_offset + step * i as f64) as i32;
        let y_clamped = y.clamp(0, viewbox_h);
        if i == 0 {
            out.push_str(&format!("M {x} {y_clamped}"));
        } else {
            out.push_str(&format!(" L {x} {y_clamped}"));
        }
    }
    out
}

fn format_duration_short(total_secs: u64) -> String {
    if total_secs < 60 {
        format!("{total_secs}s")
    } else if total_secs < 3600 {
        let m = total_secs / 60;
        let s = total_secs % 60;
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m {s}s")
        }
    } else {
        let h = total_secs / 3600;
        let m = (total_secs % 3600) / 60;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    }
}

#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "rate limit f64→u64 after multiplication by known power-of-2 constants")]
pub fn parse_rate_limit(input: &str) -> Option<u64> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "0" {
        return Some(0);
    }
    let upper = trimmed.to_uppercase();
    let upper = upper.trim_end_matches("IB").trim_end_matches('B');
    let (num_str, multiplier) = if let Some(n) = upper.strip_suffix('G') {
        (n, 1_073_741_824.0_f64)
    } else if let Some(n) = upper.strip_suffix('M') {
        (n, 1_048_576.0_f64)
    } else if let Some(n) = upper.strip_suffix('K') {
        (n, 1024.0_f64)
    } else {
        return trimmed.parse::<u64>().ok();
    };
    let val: f64 = num_str.trim().parse().ok()?;
    Some((val * multiplier) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_cap_raw() {
        let mut h = SpeedHistory::new();
        for i in 0..700 {
            h.push(i, i * 2);
        }
        assert_eq!(h.raw.len(), RAW_CAP);
        assert_eq!(h.raw.front().unwrap().download, 100);
    }

    #[test]
    fn downsample_hourly_triggers_at_10() {
        let mut h = SpeedHistory::new();
        for _ in 0..9 {
            h.push(100, 50);
        }
        assert!(h.hourly.is_empty());
        h.push(100, 50);
        assert_eq!(h.hourly.len(), 1);
        assert_eq!(h.hourly[0].download, 100);
        assert_eq!(h.hourly[0].upload, 50);
    }

    #[test]
    fn downsample_daily_triggers_at_120() {
        let mut h = SpeedHistory::new();
        for _ in 0..119 {
            h.push(200, 100);
        }
        assert!(h.daily.is_empty());
        h.push(200, 100);
        assert_eq!(h.daily.len(), 1);
        assert_eq!(h.daily[0].download, 200);
    }

    #[test]
    fn flatten_auto_uses_raw_when_not_full() {
        let mut h = SpeedHistory::new();
        for _ in 0..50 {
            h.push(100, 50);
        }
        let (dl, _) = h.flatten_auto();
        assert_eq!(dl.len(), 50);
    }

    #[test]
    fn flatten_auto_switches_to_hourly_when_raw_full() {
        let mut h = SpeedHistory::new();
        for _ in 0..RAW_CAP {
            h.push(100, 50);
        }
        let (dl, _) = h.flatten_auto();
        assert_eq!(dl.len(), h.hourly.len());
    }

    #[test]
    fn scale_zero_max_produces_flat_line() {
        let mut h = SpeedHistory::new();
        for _ in 0..10 {
            h.push(0, 0);
        }
        let (dl, ul) = h.flatten_auto();
        assert!(dl.iter().all(|&v| v == 1000));
        assert!(ul.iter().all(|&v| v == 1000));
    }

    #[test]
    fn scale_normalises_to_1000() {
        let mut h = SpeedHistory::new();
        h.push(1000, 500);
        h.push(500, 1000);
        let (dl, ul) = h.flatten_auto();
        assert_eq!(dl[0], 0);
        assert_eq!(dl[1], 500);
        assert_eq!(ul[0], 500);
        assert_eq!(ul[1], 0);
    }

    #[test]
    fn build_path_commands_empty() {
        assert_eq!(build_path_commands(&[], 1000, 1000), "");
    }

    #[test]
    fn build_path_commands_single_point() {
        let s = build_path_commands(&[500], 1000, 1000);
        assert_eq!(s, "M 1000 500");
    }

    #[test]
    fn build_path_commands_two_points() {
        let s = build_path_commands(&[0, 1000], 1000, 1000);
        assert!(s.starts_with("M "));
        assert!(s.contains(" L "));
    }

    #[test]
    fn build_path_commands_right_aligned() {
        let s = build_path_commands(&[500, 500, 500], 1000, 1000);
        assert!(s.starts_with("M 0 500"));
    }

    #[test]
    fn parse_rate_limit_raw_bytes() {
        assert_eq!(parse_rate_limit("1048576"), Some(1_048_576));
    }

    #[test]
    fn parse_rate_limit_shorthand_m() {
        assert_eq!(parse_rate_limit("1M"), Some(1_048_576));
    }

    #[test]
    fn parse_rate_limit_shorthand_k() {
        assert_eq!(parse_rate_limit("500K"), Some(512_000));
    }

    #[test]
    fn parse_rate_limit_decimal() {
        assert_eq!(parse_rate_limit("1.5M"), Some(1_572_864));
    }

    #[test]
    fn parse_rate_limit_case_insensitive() {
        assert_eq!(parse_rate_limit("1m"), Some(1_048_576));
        assert_eq!(parse_rate_limit("500k"), Some(512_000));
    }

    #[test]
    fn parse_rate_limit_with_b_suffix() {
        assert_eq!(parse_rate_limit("1MB"), Some(1_048_576));
        assert_eq!(parse_rate_limit("1mb"), Some(1_048_576));
        assert_eq!(parse_rate_limit("500KB"), Some(512_000));
        assert_eq!(parse_rate_limit("500kb"), Some(512_000));
        assert_eq!(parse_rate_limit("1GB"), Some(1_073_741_824));
        assert_eq!(parse_rate_limit("1gb"), Some(1_073_741_824));
    }

    #[test]
    fn parse_rate_limit_with_ib_suffix() {
        assert_eq!(parse_rate_limit("1MiB"), Some(1_048_576));
        assert_eq!(parse_rate_limit("500KiB"), Some(512_000));
        assert_eq!(parse_rate_limit("1GiB"), Some(1_073_741_824));
    }

    #[test]
    fn parse_rate_limit_with_space() {
        assert_eq!(parse_rate_limit("1 M"), Some(1_048_576));
        assert_eq!(parse_rate_limit("500 KB"), Some(512_000));
        assert_eq!(parse_rate_limit("1.5 MB"), Some(1_572_864));
    }

    #[test]
    fn parse_rate_limit_zero_unlimited() {
        assert_eq!(parse_rate_limit("0"), Some(0));
        assert_eq!(parse_rate_limit(""), Some(0));
    }

    #[test]
    fn parse_rate_limit_invalid() {
        assert_eq!(parse_rate_limit("abc"), None);
        assert_eq!(parse_rate_limit("M"), None);
    }

    #[test]
    fn hourly_cap_at_720() {
        let mut h = SpeedHistory::new();
        let iters = (HOURLY_CAP + 100) * usize::try_from(HOURLY_INTERVAL).unwrap();
        for _ in 0..iters {
            h.push(100, 50);
        }
        assert_eq!(h.hourly.len(), HOURLY_CAP);
    }

    #[test]
    fn daily_cap_at_1440() {
        let mut h = SpeedHistory::new();
        let iters = (DAILY_CAP + 10) * usize::try_from(DAILY_INTERVAL).unwrap();
        for _ in 0..iters {
            h.push(100, 50);
        }
        assert_eq!(h.daily.len(), DAILY_CAP);
    }

    #[test]
    fn downsample_accuracy_averages_correctly() {
        let mut h = SpeedHistory::new();
        for i in 0..10u64 {
            h.push(i * 100, i * 50);
        }
        assert_eq!(h.hourly.len(), 1);
        // Average of 0,100,200,...,900 = 450
        assert_eq!(h.hourly[0].download, 450);
        // Average of 0,50,100,...,450 = 225
        assert_eq!(h.hourly[0].upload, 225);
    }

    #[test]
    fn max_rate_returns_peak() {
        let mut h = SpeedHistory::new();
        h.push(100, 200);
        h.push(500, 300);
        h.push(50, 400);
        assert_eq!(h.max_rate(), 500);
    }

    #[test]
    fn elapsed_label_short() {
        let mut h = SpeedHistory::new();
        for _ in 0..120 {
            h.push(100, 50);
        }
        assert_eq!(h.elapsed_label(), "1m");
    }

    #[test]
    fn elapsed_label_empty() {
        let h = SpeedHistory::new();
        assert_eq!(h.elapsed_label(), "");
    }

    #[test]
    fn format_duration_short_minutes() {
        assert_eq!(format_duration_short(0), "0s");
        assert_eq!(format_duration_short(30), "30s");
        assert_eq!(format_duration_short(60), "1m");
        assert_eq!(format_duration_short(90), "1m 30s");
        assert_eq!(format_duration_short(3600), "1h");
        assert_eq!(format_duration_short(3660), "1h 1m");
    }
}
