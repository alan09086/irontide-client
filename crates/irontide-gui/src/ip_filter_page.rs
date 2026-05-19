use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ManualRule {
    pub label: String,
    pub first: String,
    pub last: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct IpFilterState {
    #[serde(default)]
    pub rules: Vec<ManualRule>,
    #[serde(default)]
    pub imported_files: Vec<String>,
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
pub fn state_path() -> PathBuf {
    config_dir().join("ip_filter_rules.json")
}

#[must_use]
pub fn load_state() -> IpFilterState {
    let path = state_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => IpFilterState::default(),
    }
}

pub fn save_state(state: &IpFilterState) -> std::io::Result<()> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state)
        .map_err(std::io::Error::other)?;
    std::fs::write(&path, json)
}

pub fn parse_ip_range(text: &str) -> Option<(IpAddr, IpAddr)> {
    let text = text.trim();
    if let Some((first, last)) = text.split_once('-') {
        let f: IpAddr = first.trim().parse().ok()?;
        let l: IpAddr = last.trim().parse().ok()?;
        Some((f, l))
    } else if let Some((ip, prefix_len)) = text.split_once('/') {
        let addr: IpAddr = ip.trim().parse().ok()?;
        let bits: u32 = prefix_len.trim().parse().ok()?;
        match addr {
            IpAddr::V4(v4) => {
                if bits > 32 {
                    return None;
                }
                let mask = if bits == 0 { 0 } else { u32::MAX << (32 - bits) };
                let start = u32::from(v4) & mask;
                let end = start | !mask;
                Some((
                    IpAddr::V4(start.into()),
                    IpAddr::V4(end.into()),
                ))
            }
            IpAddr::V6(v6) => {
                if bits > 128 {
                    return None;
                }
                let mask = if bits == 0 { 0 } else { u128::MAX << (128 - bits) };
                let start = u128::from(v6) & mask;
                let end = start | !mask;
                Some((
                    IpAddr::V6(start.into()),
                    IpAddr::V6(end.into()),
                ))
            }
        }
    } else {
        let addr: IpAddr = text.parse().ok()?;
        Some((addr, addr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn parse_single_ip() {
        let (f, l) = parse_ip_range("192.168.1.1").unwrap();
        assert_eq!(f, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)));
        assert_eq!(f, l);
    }

    #[test]
    fn parse_range() {
        let (f, l) = parse_ip_range("10.0.0.1 - 10.0.0.255").unwrap();
        assert_eq!(f, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
        assert_eq!(l, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 255)));
    }

    #[test]
    fn parse_cidr() {
        let (f, l) = parse_ip_range("192.168.0.0/24").unwrap();
        assert_eq!(f, IpAddr::V4(Ipv4Addr::new(192, 168, 0, 0)));
        assert_eq!(l, IpAddr::V4(Ipv4Addr::new(192, 168, 0, 255)));
    }

    #[test]
    fn parse_cidr_16() {
        let (f, l) = parse_ip_range("172.16.0.0/16").unwrap();
        assert_eq!(f, IpAddr::V4(Ipv4Addr::new(172, 16, 0, 0)));
        assert_eq!(l, IpAddr::V4(Ipv4Addr::new(172, 16, 255, 255)));
    }

    #[test]
    fn parse_ipv6_single() {
        let (f, l) = parse_ip_range("::1").unwrap();
        assert_eq!(f, IpAddr::V6(Ipv6Addr::LOCALHOST));
        assert_eq!(f, l);
    }

    #[test]
    fn parse_ipv6_cidr() {
        let (f, l) = parse_ip_range("fe80::/10").unwrap();
        assert_eq!(f, IpAddr::V6("fe80::".parse::<Ipv6Addr>().unwrap()));
        assert_eq!(l, IpAddr::V6("febf:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse::<Ipv6Addr>().unwrap()));
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_ip_range("not-an-ip").is_none());
        assert!(parse_ip_range("192.168.0.0/33").is_none());
    }

    #[test]
    fn state_round_trip() {
        let state = IpFilterState {
            rules: vec![ManualRule {
                label: "Test".into(),
                first: "10.0.0.0".into(),
                last: "10.0.0.255".into(),
                enabled: true,
            }],
            imported_files: vec!["blocklist.p2p".into()],
        };
        let json = serde_json::to_string(&state).unwrap();
        let loaded: IpFilterState = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.rules.len(), 1);
        assert_eq!(loaded.rules[0].label, "Test");
        assert_eq!(loaded.imported_files.len(), 1);
    }

    #[test]
    fn default_state_empty() {
        let s = IpFilterState::default();
        assert!(s.rules.is_empty());
        assert!(s.imported_files.is_empty());
    }
}
