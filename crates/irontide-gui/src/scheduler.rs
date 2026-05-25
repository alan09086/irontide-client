use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum CellState {
    Full = 0,
    Limited = 1,
    Off = 2,
}

impl CellState {
    #[allow(dead_code)]
    #[must_use]
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Limited,
            2 => Self::Off,
            _ => Self::Full,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Full => Self::Limited,
            Self::Limited => Self::Off,
            Self::Off => Self::Full,
        }
    }
}

pub const DAYS: usize = 7;
pub const HOURS: usize = 24;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BandwidthSchedule {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub grid: [[CellState; HOURS]; DAYS],
    #[serde(default = "default_limited_rate")]
    pub limited_rate_kib: u32,
}

fn default_true() -> bool {
    true
}

fn default_limited_rate() -> u32 {
    512
}

impl Default for BandwidthSchedule {
    fn default() -> Self {
        Self {
            enabled: false,
            grid: [[CellState::Full; HOURS]; DAYS],
            limited_rate_kib: default_limited_rate(),
        }
    }
}

impl BandwidthSchedule {
    #[must_use]
    pub fn preset_always_on() -> Self {
        Self {
            enabled: true,
            grid: [[CellState::Full; HOURS]; DAYS],
            limited_rate_kib: default_limited_rate(),
        }
    }

    #[must_use]
    pub fn preset_night_only() -> Self {
        let mut grid = [[CellState::Off; HOURS]; DAYS];
        for day in &mut grid {
            for (hour, cell) in day.iter_mut().enumerate() {
                if !(7..23).contains(&hour) {
                    *cell = CellState::Full;
                }
            }
        }
        Self {
            enabled: true,
            grid,
            limited_rate_kib: default_limited_rate(),
        }
    }

    #[must_use]
    pub fn preset_work_hours_limited() -> Self {
        let mut grid = [[CellState::Full; HOURS]; DAYS];
        for (day_idx, day) in grid.iter_mut().enumerate() {
            if day_idx < 5 {
                for cell in &mut day[9..17] {
                    *cell = CellState::Limited;
                }
            }
        }
        Self {
            enabled: true,
            grid,
            limited_rate_kib: default_limited_rate(),
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn current_state(&self) -> CellState {
        if !self.enabled {
            return CellState::Full;
        }
        let now = chrono::Local::now();
        let dow = now.format("%u").to_string().parse::<usize>().unwrap_or(1);
        let day_idx = dow.wrapping_sub(1).min(DAYS - 1);
        let hour = now
            .format("%H")
            .to_string()
            .parse::<usize>()
            .unwrap_or(0)
            .min(HOURS - 1);
        self.grid[day_idx][hour]
    }

    #[must_use]
    pub fn to_flat_grid(&self) -> Vec<u8> {
        let mut flat = Vec::with_capacity(DAYS * HOURS);
        for day in &self.grid {
            for cell in day {
                flat.push(*cell as u8);
            }
        }
        flat
    }

    #[allow(dead_code)]
    pub fn set_cell(&mut self, day: usize, hour: usize, state: CellState) {
        if day < DAYS && hour < HOURS {
            self.grid[day][hour] = state;
        }
    }

    pub fn toggle_cell(&mut self, day: usize, hour: usize) {
        if day < DAYS && hour < HOURS {
            self.grid[day][hour] = self.grid[day][hour].next();
        }
    }
}

fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        std::path::Path::new(&dir).join("irontide")
    } else if let Ok(home) = std::env::var("HOME") {
        std::path::Path::new(&home).join(".config").join("irontide")
    } else {
        PathBuf::from("/tmp/irontide")
    }
}

#[must_use]
pub fn schedule_path() -> PathBuf {
    config_dir().join("bandwidth_schedule.json")
}

#[must_use]
pub fn load_schedule() -> BandwidthSchedule {
    let path = schedule_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => BandwidthSchedule::default(),
    }
}

pub fn save_schedule(schedule: &BandwidthSchedule) -> std::io::Result<()> {
    let path = schedule_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(schedule).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)
}

#[allow(dead_code)]
pub const DAY_LABELS: [&str; DAYS] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_schedule_all_full() {
        let s = BandwidthSchedule::default();
        assert!(!s.enabled);
        for day in &s.grid {
            for cell in day {
                assert_eq!(*cell, CellState::Full);
            }
        }
    }

    #[test]
    fn cell_state_round_trip() {
        for v in 0..=2 {
            let state = CellState::from_u8(v);
            assert_eq!(state as u8, v);
        }
        assert_eq!(CellState::from_u8(255), CellState::Full);
    }

    #[test]
    fn cell_state_cycle() {
        assert_eq!(CellState::Full.next(), CellState::Limited);
        assert_eq!(CellState::Limited.next(), CellState::Off);
        assert_eq!(CellState::Off.next(), CellState::Full);
    }

    #[test]
    fn preset_night_only_structure() {
        let s = BandwidthSchedule::preset_night_only();
        assert!(s.enabled);
        for day in &s.grid {
            for (hour, cell) in day.iter().enumerate().take(23).skip(7) {
                assert_eq!(*cell, CellState::Off, "hour {hour} should be off");
            }
            for hour in [0, 1, 2, 3, 4, 5, 6, 23] {
                assert_eq!(day[hour], CellState::Full, "hour {hour} should be full");
            }
        }
    }

    #[test]
    fn preset_work_hours_limited_structure() {
        let s = BandwidthSchedule::preset_work_hours_limited();
        assert!(s.enabled);
        for day_idx in 0..5 {
            for hour in 9..17 {
                assert_eq!(s.grid[day_idx][hour], CellState::Limited);
            }
            assert_eq!(s.grid[day_idx][0], CellState::Full);
        }
        for day_idx in 5..7 {
            for hour in 0..24 {
                assert_eq!(s.grid[day_idx][hour], CellState::Full);
            }
        }
    }

    #[test]
    fn toggle_cell_cycles() {
        let mut s = BandwidthSchedule::default();
        assert_eq!(s.grid[0][0], CellState::Full);
        s.toggle_cell(0, 0);
        assert_eq!(s.grid[0][0], CellState::Limited);
        s.toggle_cell(0, 0);
        assert_eq!(s.grid[0][0], CellState::Off);
        s.toggle_cell(0, 0);
        assert_eq!(s.grid[0][0], CellState::Full);
    }

    #[test]
    fn set_cell_boundary() {
        let mut s = BandwidthSchedule::default();
        s.set_cell(6, 23, CellState::Off);
        assert_eq!(s.grid[6][23], CellState::Off);
        s.set_cell(99, 99, CellState::Off);
    }

    #[test]
    fn flat_grid_length() {
        let s = BandwidthSchedule::default();
        let flat = s.to_flat_grid();
        assert_eq!(flat.len(), DAYS * HOURS);
        assert!(flat.iter().all(|&v| v == 0));
    }

    #[test]
    fn disabled_schedule_always_full() {
        let mut s = BandwidthSchedule::preset_night_only();
        s.enabled = false;
        assert_eq!(s.current_state(), CellState::Full);
    }

    #[test]
    fn state_round_trip() {
        let mut s = BandwidthSchedule {
            enabled: true,
            limited_rate_kib: 256,
            ..BandwidthSchedule::default()
        };
        s.set_cell(2, 14, CellState::Limited);
        s.set_cell(5, 3, CellState::Off);

        let json = serde_json::to_string(&s).unwrap();
        let loaded: BandwidthSchedule = serde_json::from_str(&json).unwrap();
        assert!(loaded.enabled);
        assert_eq!(loaded.limited_rate_kib, 256);
        assert_eq!(loaded.grid[2][14], CellState::Limited);
        assert_eq!(loaded.grid[5][3], CellState::Off);
        assert_eq!(loaded.grid[0][0], CellState::Full);
    }

    #[test]
    fn preset_always_on() {
        let s = BandwidthSchedule::preset_always_on();
        assert!(s.enabled);
        for day in &s.grid {
            for cell in day {
                assert_eq!(*cell, CellState::Full);
            }
        }
    }
}
