use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchPlugin {
    pub name: String,
    pub url_template: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub categories: Vec<String>,
    pub result_format: ResultFormat,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ResultFormat {
    #[serde(rename = "json")]
    Json {
        results_path: String,
        fields: FieldMapping,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FieldMapping {
    pub name: String,
    pub magnet_url: String,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub seeds: Option<String>,
    #[serde(default)]
    pub leechers: Option<String>,
    #[serde(default)]
    pub info_page: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub name: String,
    pub magnet_url: String,
    pub size: String,
    pub seeds: i32,
    pub leechers: i32,
    pub source: String,
}

pub fn plugins_dir() -> PathBuf {
    let base = dirs_path();
    base.join("plugins").join("search")
}

fn dirs_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        Path::new(&dir).join("irontide")
    } else if let Ok(home) = std::env::var("HOME") {
        Path::new(&home).join(".config").join("irontide")
    } else {
        PathBuf::from("/tmp/irontide")
    }
}

pub fn load_plugins() -> Vec<SearchPlugin> {
    let dir = plugins_dir();
    let mut plugins = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                match std::fs::read_to_string(&path) {
                    Ok(text) => match serde_json::from_str::<SearchPlugin>(&text) {
                        Ok(p) => plugins.push(p),
                        Err(e) => {
                            tracing::warn!("failed to parse search plugin {}: {e}", path.display());
                        }
                    },
                    Err(e) => {
                        tracing::warn!("failed to read search plugin {}: {e}", path.display());
                    }
                }
            }
        }
    }
    plugins.sort_by_key(|p| p.name.clone());
    plugins
}

pub fn build_search_url(plugin: &SearchPlugin, query: &str) -> String {
    plugin.url_template.replace("{query}", &urlencoded(query))
}

fn urlencoded(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(char::from(HEX[(b >> 4) as usize]));
                out.push(char::from(HEX[(b & 0xF) as usize]));
            }
        }
    }
    out
}

const HEX: [u8; 16] = *b"0123456789ABCDEF";

pub fn parse_json_results(
    body: &str,
    results_path: &str,
    fields: &FieldMapping,
    source: &str,
) -> Vec<SearchResult> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };

    let items = resolve_path(&value, results_path);
    let Some(arr) = items.and_then(|v| v.as_array()) else {
        return Vec::new();
    };

    arr.iter()
        .filter_map(|item| {
            let name = get_string(item, &fields.name)?;
            let magnet_url = get_string(item, &fields.magnet_url)?;
            let size = fields
                .size
                .as_ref()
                .and_then(|p| get_string(item, p))
                .unwrap_or_default();
            let seeds = fields
                .seeds
                .as_ref()
                .and_then(|p| get_i32(item, p))
                .unwrap_or(0);
            let leechers = fields
                .leechers
                .as_ref()
                .and_then(|p| get_i32(item, p))
                .unwrap_or(0);
            Some(SearchResult {
                name,
                magnet_url,
                size,
                seeds,
                leechers,
                source: source.to_owned(),
            })
        })
        .collect()
}

fn resolve_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for key in path.split('.') {
        current = current.get(key)?;
    }
    Some(current)
}

fn get_string(value: &serde_json::Value, path: &str) -> Option<String> {
    let v = resolve_path(value, path)?;
    if let Some(s) = v.as_str() {
        Some(s.to_owned())
    } else {
        v.as_i64().map(|n| n.to_string())
    }
}

fn get_i32(value: &serde_json::Value, path: &str) -> Option<i32> {
    let v = resolve_path(value, path)?;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "seed/leech counts are always small integers"
    )]
    v.as_i64().map(|n| n as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_round_trip() {
        let plugin = SearchPlugin {
            name: "Test".to_owned(),
            url_template: "https://example.com/search?q={query}".to_owned(),
            enabled: true,
            categories: vec!["all".to_owned()],
            result_format: ResultFormat::Json {
                results_path: "results".to_owned(),
                fields: FieldMapping {
                    name: "title".to_owned(),
                    magnet_url: "magnet".to_owned(),
                    size: Some("size".to_owned()),
                    seeds: Some("seeders".to_owned()),
                    leechers: Some("leechers".to_owned()),
                    info_page: None,
                },
            },
        };
        let json = serde_json::to_string(&plugin).unwrap();
        let parsed: SearchPlugin = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test");
        assert!(parsed.enabled);
    }

    #[test]
    fn build_search_url_encodes_query() {
        let plugin = SearchPlugin {
            name: "Test".to_owned(),
            url_template: "https://api.example.com/search?q={query}&limit=20".to_owned(),
            enabled: true,
            categories: Vec::new(),
            result_format: ResultFormat::Json {
                results_path: "data".to_owned(),
                fields: FieldMapping {
                    name: "name".to_owned(),
                    magnet_url: "magnet".to_owned(),
                    size: None,
                    seeds: None,
                    leechers: None,
                    info_page: None,
                },
            },
        };
        let url = build_search_url(&plugin, "hello world");
        assert_eq!(url, "https://api.example.com/search?q=hello+world&limit=20");
    }

    #[test]
    fn build_search_url_encodes_special_chars() {
        let plugin = SearchPlugin {
            name: "T".to_owned(),
            url_template: "https://api.example.com?q={query}".to_owned(),
            enabled: true,
            categories: Vec::new(),
            result_format: ResultFormat::Json {
                results_path: "r".to_owned(),
                fields: FieldMapping {
                    name: "n".to_owned(),
                    magnet_url: "m".to_owned(),
                    size: None,
                    seeds: None,
                    leechers: None,
                    info_page: None,
                },
            },
        };
        let url = build_search_url(&plugin, "foo&bar=baz");
        assert!(url.contains("foo%26bar%3Dbaz"));
    }

    #[test]
    fn parse_json_results_basic() {
        let body = r#"{
            "results": [
                {"title": "Ubuntu 24.04", "magnet": "magnet:?xt=urn:btih:abc", "seeders": 100, "leechers": 5, "size": "4.7 GB"},
                {"title": "Fedora 40", "magnet": "magnet:?xt=urn:btih:def", "seeders": 50, "leechers": 3, "size": "2.1 GB"}
            ]
        }"#;
        let fields = FieldMapping {
            name: "title".to_owned(),
            magnet_url: "magnet".to_owned(),
            size: Some("size".to_owned()),
            seeds: Some("seeders".to_owned()),
            leechers: Some("leechers".to_owned()),
            info_page: None,
        };
        let results = parse_json_results(body, "results", &fields, "test");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "Ubuntu 24.04");
        assert_eq!(results[0].seeds, 100);
        assert_eq!(results[0].source, "test");
        assert_eq!(results[1].name, "Fedora 40");
    }

    #[test]
    fn parse_json_results_nested_path() {
        let body = r#"{"data": {"items": [{"n": "Test", "m": "magnet:?xt=x"}]}}"#;
        let fields = FieldMapping {
            name: "n".to_owned(),
            magnet_url: "m".to_owned(),
            size: None,
            seeds: None,
            leechers: None,
            info_page: None,
        };
        let results = parse_json_results(body, "data.items", &fields, "src");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Test");
    }

    #[test]
    fn parse_json_results_invalid_json() {
        let results = parse_json_results(
            "not json",
            "results",
            &FieldMapping {
                name: "n".to_owned(),
                magnet_url: "m".to_owned(),
                size: None,
                seeds: None,
                leechers: None,
                info_page: None,
            },
            "x",
        );
        assert!(results.is_empty());
    }

    #[test]
    fn parse_json_results_missing_required_fields() {
        let body = r#"{"results": [{"title": "Test"}]}"#;
        let fields = FieldMapping {
            name: "title".to_owned(),
            magnet_url: "magnet".to_owned(),
            size: None,
            seeds: None,
            leechers: None,
            info_page: None,
        };
        let results = parse_json_results(body, "results", &fields, "x");
        assert!(results.is_empty());
    }

    #[test]
    fn plugins_dir_is_absolute() {
        let dir = plugins_dir();
        assert!(dir.is_absolute());
        assert!(dir.to_string_lossy().contains("plugins"));
    }

    #[test]
    fn urlencoded_preserves_alphanum() {
        assert_eq!(urlencoded("hello123"), "hello123");
    }

    #[test]
    fn urlencoded_encodes_spaces() {
        assert_eq!(urlencoded("hello world"), "hello+world");
    }

    #[test]
    fn enabled_defaults_to_true() {
        let json = r#"{"name":"X","url_template":"http://x/{query}","categories":[],"result_format":{"type":"json","results_path":"r","fields":{"name":"n","magnet_url":"m"}}}"#;
        let p: SearchPlugin = serde_json::from_str(json).unwrap();
        assert!(p.enabled);
    }
}
