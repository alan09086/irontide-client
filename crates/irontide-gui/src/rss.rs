use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RssFeed {
    pub url: String,
    pub title: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub last_refresh: Option<i64>,
    #[serde(default)]
    pub error: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RssItem {
    pub title: String,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub torrent_url: Option<String>,
    #[serde(default)]
    pub magnet_url: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub pub_date: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
    pub feed_url: String,
    #[serde(default)]
    pub read: bool,
    #[serde(default)]
    pub downloaded: bool,
}

impl RssItem {
    #[must_use]
    pub fn display_title(&self) -> &str {
        if self.title.is_empty() {
            self.link.as_deref().unwrap_or("(untitled)")
        } else {
            &self.title
        }
    }

    #[must_use]
    pub fn best_download_url(&self) -> Option<&str> {
        self.magnet_url
            .as_deref()
            .or(self.torrent_url.as_deref())
            .or(self.link.as_deref())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DownloadRule {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub must_contain: String,
    #[serde(default)]
    pub must_not_contain: String,
    #[serde(default)]
    pub feed_urls: Vec<String>,
    #[serde(default)]
    pub use_regex: bool,
    #[serde(default)]
    pub smart_filter: bool,
    #[serde(default)]
    pub add_paused: bool,
    #[serde(default)]
    pub download_dir: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

impl DownloadRule {
    #[must_use]
    #[allow(
        dead_code,
        reason = "will be used by auto-download in a future milestone"
    )]
    pub fn matches(&self, item: &RssItem) -> bool {
        if !self.enabled {
            return false;
        }
        if !self.feed_urls.is_empty() && !self.feed_urls.contains(&item.feed_url) {
            return false;
        }
        let title = item.title.to_lowercase();
        if !self.must_contain.is_empty() {
            let pattern = self.must_contain.to_lowercase();
            if self.use_regex {
                return false;
            }
            let any_match = pattern.split('|').any(|p| title.contains(p.trim()));
            if !any_match {
                return false;
            }
        }
        if !self.must_not_contain.is_empty() {
            let exclude = self.must_not_contain.to_lowercase();
            let any_exclude = exclude.split('|').any(|p| title.contains(p.trim()));
            if any_exclude {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RssState {
    #[serde(default)]
    pub feeds: Vec<RssFeed>,
    #[serde(default)]
    pub rules: Vec<DownloadRule>,
    #[serde(default)]
    pub items: Vec<RssItem>,
}

pub fn state_path() -> PathBuf {
    config_dir().join("rss.json")
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

pub fn load_state() -> RssState {
    let path = state_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => RssState::default(),
    }
}

pub fn save_state(state: &RssState) -> std::io::Result<()> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)
}

pub fn parse_rss_feed(xml: &str, feed_url: &str) -> Vec<RssItem> {
    let mut items = Vec::new();
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut in_item = false;
    let mut current_title = String::new();
    let mut current_link = String::new();
    let mut current_description = String::new();
    let mut current_pub_date = String::new();
    let mut current_torrent_url: Option<String> = None;
    let mut current_magnet_url: Option<String> = None;
    let mut current_size: Option<String> = None;
    let mut current_tag = String::new();

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if name == "item" || name == "entry" {
                    in_item = true;
                    current_title.clear();
                    current_link.clear();
                    current_description.clear();
                    current_pub_date.clear();
                    current_torrent_url = None;
                    current_magnet_url = None;
                    current_size = None;
                }
                if in_item {
                    name.clone_into(&mut current_tag);
                    extract_element_attrs(
                        &name,
                        &e,
                        &mut current_torrent_url,
                        &mut current_size,
                        &mut current_link,
                    );
                }
            }
            Ok(quick_xml::events::Event::Empty(e)) if in_item => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                extract_element_attrs(
                    &name,
                    &e,
                    &mut current_torrent_url,
                    &mut current_size,
                    &mut current_link,
                );
            }
            Ok(quick_xml::events::Event::Text(e)) if in_item => {
                let text = e.unescape().unwrap_or_default().to_string();
                if text.starts_with("magnet:") {
                    current_magnet_url = Some(text.clone());
                }
                match current_tag.as_str() {
                    "title" => current_title = text,
                    "link" if current_link.is_empty() => current_link = text,
                    "description" | "summary" | "content" => current_description = text,
                    "pubDate" | "published" | "updated" => current_pub_date = text,
                    _ => {}
                }
            }
            Ok(quick_xml::events::Event::End(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                if (name == "item" || name == "entry") && in_item {
                    in_item = false;
                    if current_link.starts_with("magnet:") && current_magnet_url.is_none() {
                        current_magnet_url = Some(current_link.clone());
                    }
                    let to_opt = |s: &String| {
                        if s.is_empty() { None } else { Some(s.clone()) }
                    };
                    items.push(RssItem {
                        title: current_title.clone(),
                        link: to_opt(&current_link),
                        torrent_url: current_torrent_url.clone(),
                        magnet_url: current_magnet_url.clone(),
                        description: to_opt(&current_description),
                        pub_date: to_opt(&current_pub_date),
                        size: current_size
                            .as_deref()
                            .and_then(|s| s.parse::<u64>().ok())
                            .map(format_size),
                        feed_url: feed_url.to_owned(),
                        read: false,
                        downloaded: false,
                    });
                } else if in_item {
                    current_tag.clear();
                }
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    items
}

fn extract_element_attrs(
    name: &str,
    e: &quick_xml::events::BytesStart<'_>,
    torrent_url: &mut Option<String>,
    size: &mut Option<String>,
    link: &mut String,
) {
    if name == "enclosure" {
        for attr in e.attributes().flatten() {
            if attr.key.as_ref() == b"url" {
                let url = String::from_utf8_lossy(&attr.value).to_string();
                if url.ends_with(".torrent") || url.contains("download") {
                    *torrent_url = Some(url);
                }
            }
            if attr.key.as_ref() == b"length" {
                *size = Some(String::from_utf8_lossy(&attr.value).to_string());
            }
        }
    }
    if name == "link" {
        for attr in e.attributes().flatten() {
            if attr.key.as_ref() == b"href" && link.is_empty() {
                *link = String::from_utf8_lossy(&attr.value).to_string();
            }
        }
    }
}

pub fn extract_feed_title(xml: &str) -> String {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut in_title = false;
    let mut depth = 0;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                depth += 1;
                if name == "title" && depth <= 3 {
                    in_title = true;
                }
                if name == "item" || name == "entry" {
                    break;
                }
            }
            Ok(quick_xml::events::Event::Text(e)) if in_title => {
                return e.unescape().unwrap_or_default().to_string();
            }
            Ok(quick_xml::events::Event::End(_)) => {
                in_title = false;
                depth -= 1;
            }
            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    String::from("Untitled Feed")
}

#[allow(clippy::cast_precision_loss, reason = "display-only size formatting")]
fn format_size(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * 1024 * 1024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[allow(
    dead_code,
    reason = "will be used by auto-download in a future milestone"
)]
pub fn matching_rules<'a>(item: &RssItem, rules: &'a [DownloadRule]) -> Vec<&'a DownloadRule> {
    rules.iter().filter(|r| r.matches(item)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rss2_basic() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Test Feed</title>
    <item>
      <title>Ubuntu 24.04 LTS</title>
      <link>https://example.com/ubuntu</link>
      <enclosure url="https://example.com/ubuntu.torrent" length="4700000000" type="application/x-bittorrent"/>
      <pubDate>Mon, 01 Jan 2024 00:00:00 GMT</pubDate>
    </item>
    <item>
      <title>Fedora 40</title>
      <link>magnet:?xt=urn:btih:abc123</link>
    </item>
  </channel>
</rss>"#;
        let items = parse_rss_feed(xml, "https://example.com/feed.xml");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Ubuntu 24.04 LTS");
        assert_eq!(
            items[0].torrent_url.as_deref(),
            Some("https://example.com/ubuntu.torrent")
        );
        assert_eq!(items[0].size.as_deref(), Some("4.4 GiB"));
        assert_eq!(items[1].title, "Fedora 40");
        assert!(items[1].magnet_url.is_some());
        assert!(items[1].magnet_url.as_ref().unwrap().starts_with("magnet:"));
    }

    #[test]
    fn parse_atom_feed() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Feed</title>
  <entry>
    <title>Entry One</title>
    <link href="https://example.com/entry1"/>
    <published>2024-01-01T00:00:00Z</published>
  </entry>
</feed>"#;
        let items = parse_rss_feed(xml, "https://example.com/atom.xml");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Entry One");
        assert_eq!(items[0].link.as_deref(), Some("https://example.com/entry1"));
    }

    #[test]
    fn extract_title_rss() {
        let xml = r#"<?xml version="1.0"?><rss><channel><title>My Feed</title><item></item></channel></rss>"#;
        assert_eq!(extract_feed_title(xml), "My Feed");
    }

    #[test]
    fn extract_title_missing() {
        let xml = r#"<?xml version="1.0"?><rss><channel><item></item></channel></rss>"#;
        assert_eq!(extract_feed_title(xml), "Untitled Feed");
    }

    #[test]
    fn rule_matches_simple() {
        let rule = DownloadRule {
            name: "Linux ISOs".to_owned(),
            enabled: true,
            must_contain: "ubuntu|fedora".to_owned(),
            must_not_contain: "beta".to_owned(),
            feed_urls: vec![],
            use_regex: false,
            smart_filter: false,
            add_paused: false,
            download_dir: None,
            category: None,
        };
        let item_match = RssItem {
            title: "Ubuntu 24.04 LTS".to_owned(),
            link: None,
            torrent_url: None,
            magnet_url: None,
            description: None,
            pub_date: None,
            size: None,
            feed_url: "https://example.com/feed".to_owned(),
            read: false,
            downloaded: false,
        };
        assert!(rule.matches(&item_match));

        let item_exclude = RssItem {
            title: "Ubuntu 24.10 Beta".to_owned(),
            ..item_match.clone()
        };
        assert!(!rule.matches(&item_exclude));

        let item_no_match = RssItem {
            title: "Arch Linux 2024".to_owned(),
            ..item_match
        };
        assert!(!rule.matches(&item_no_match));
    }

    #[test]
    fn rule_disabled_never_matches() {
        let rule = DownloadRule {
            name: "disabled".to_owned(),
            enabled: false,
            must_contain: String::new(),
            must_not_contain: String::new(),
            feed_urls: vec![],
            use_regex: false,
            smart_filter: false,
            add_paused: false,
            download_dir: None,
            category: None,
        };
        let item = RssItem {
            title: "anything".to_owned(),
            link: None,
            torrent_url: None,
            magnet_url: None,
            description: None,
            pub_date: None,
            size: None,
            feed_url: String::new(),
            read: false,
            downloaded: false,
        };
        assert!(!rule.matches(&item));
    }

    #[test]
    fn rule_feed_url_filter() {
        let rule = DownloadRule {
            name: "specific".to_owned(),
            enabled: true,
            must_contain: String::new(),
            must_not_contain: String::new(),
            feed_urls: vec!["https://a.com/feed".to_owned()],
            use_regex: false,
            smart_filter: false,
            add_paused: false,
            download_dir: None,
            category: None,
        };
        let item_a = RssItem {
            title: "test".to_owned(),
            link: None,
            torrent_url: None,
            magnet_url: None,
            description: None,
            pub_date: None,
            size: None,
            feed_url: "https://a.com/feed".to_owned(),
            read: false,
            downloaded: false,
        };
        assert!(rule.matches(&item_a));

        let item_b = RssItem {
            feed_url: "https://b.com/feed".to_owned(),
            ..item_a
        };
        assert!(!rule.matches(&item_b));
    }

    #[test]
    fn format_size_ranges() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1 KiB");
        assert_eq!(format_size(1_500_000), "1.4 MiB");
        assert_eq!(format_size(4_700_000_000), "4.4 GiB");
    }

    #[test]
    fn state_round_trip() {
        let state = RssState {
            feeds: vec![RssFeed {
                url: "https://example.com/feed".to_owned(),
                title: "Test".to_owned(),
                enabled: true,
                alias: None,
                last_refresh: Some(1_700_000_000),
                error: None,
            }],
            rules: vec![DownloadRule {
                name: "rule1".to_owned(),
                enabled: true,
                must_contain: "linux".to_owned(),
                must_not_contain: String::new(),
                feed_urls: vec![],
                use_regex: false,
                smart_filter: false,
                add_paused: false,
                download_dir: None,
                category: None,
            }],
            items: vec![],
        };
        let json = serde_json::to_string(&state).unwrap();
        let recovered: RssState = serde_json::from_str(&json).unwrap();
        assert_eq!(recovered.feeds.len(), 1);
        assert_eq!(recovered.feeds[0].url, "https://example.com/feed");
        assert_eq!(recovered.rules.len(), 1);
        assert_eq!(recovered.rules[0].must_contain, "linux");
    }

    #[test]
    fn best_download_url_priority() {
        let item = RssItem {
            title: "test".to_owned(),
            link: Some("https://example.com".to_owned()),
            torrent_url: Some("https://example.com/file.torrent".to_owned()),
            magnet_url: Some("magnet:?xt=urn:btih:abc".to_owned()),
            description: None,
            pub_date: None,
            size: None,
            feed_url: String::new(),
            read: false,
            downloaded: false,
        };
        assert_eq!(item.best_download_url(), Some("magnet:?xt=urn:btih:abc"));

        let no_magnet = RssItem {
            magnet_url: None,
            ..item.clone()
        };
        assert_eq!(
            no_magnet.best_download_url(),
            Some("https://example.com/file.torrent")
        );

        let link_only = RssItem {
            magnet_url: None,
            torrent_url: None,
            ..item
        };
        assert_eq!(link_only.best_download_url(), Some("https://example.com"));
    }

    #[test]
    fn matching_rules_collects() {
        let rules = vec![
            DownloadRule {
                name: "r1".to_owned(),
                enabled: true,
                must_contain: "ubuntu".to_owned(),
                must_not_contain: String::new(),
                feed_urls: vec![],
                use_regex: false,
                smart_filter: false,
                add_paused: false,
                download_dir: None,
                category: None,
            },
            DownloadRule {
                name: "r2".to_owned(),
                enabled: true,
                must_contain: "fedora".to_owned(),
                must_not_contain: String::new(),
                feed_urls: vec![],
                use_regex: false,
                smart_filter: false,
                add_paused: false,
                download_dir: None,
                category: None,
            },
        ];
        let item = RssItem {
            title: "Ubuntu 24.04".to_owned(),
            link: None,
            torrent_url: None,
            magnet_url: None,
            description: None,
            pub_date: None,
            size: None,
            feed_url: String::new(),
            read: false,
            downloaded: false,
        };
        let matched = matching_rules(&item, &rules);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].name, "r1");
    }
}
