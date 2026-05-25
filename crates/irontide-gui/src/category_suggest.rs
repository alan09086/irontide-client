//! Smart category suggestion via keyword-based local classifier (M202).
//!
//! Learns from past user categorisations and suggests categories for new
//! torrents based on name tokens, file extensions, and tracker domains.

use std::collections::HashMap;
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
    config_dir().join("category_classifier.json")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClassifierModel {
    pub category_keywords: HashMap<String, Vec<String>>,
    pub extension_map: HashMap<String, String>,
    pub tracker_map: HashMap<String, String>,
}

impl Default for ClassifierModel {
    fn default() -> Self {
        let mut extension_map = HashMap::new();
        extension_map.insert("mkv".to_string(), "Video".to_string());
        extension_map.insert("mp4".to_string(), "Video".to_string());
        extension_map.insert("avi".to_string(), "Video".to_string());
        extension_map.insert("flac".to_string(), "Music".to_string());
        extension_map.insert("mp3".to_string(), "Music".to_string());
        extension_map.insert("ogg".to_string(), "Music".to_string());
        extension_map.insert("pdf".to_string(), "Books".to_string());
        extension_map.insert("epub".to_string(), "Books".to_string());
        extension_map.insert("iso".to_string(), "Software".to_string());
        extension_map.insert("exe".to_string(), "Software".to_string());
        extension_map.insert("deb".to_string(), "Software".to_string());
        extension_map.insert("rpm".to_string(), "Software".to_string());

        Self {
            category_keywords: HashMap::new(),
            extension_map,
            tracker_map: HashMap::new(),
        }
    }
}

impl ClassifierModel {
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

    pub fn suggest(
        &self,
        name: &str,
        file_extensions: &[String],
        trackers: &[String],
    ) -> Option<SuggestionResult> {
        let name_lower = name.to_lowercase();
        let tokens: Vec<&str> = name_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();

        let mut scores: HashMap<&str, f32> = HashMap::new();

        for ext in file_extensions {
            let ext_lower = ext.to_lowercase();
            if let Some(cat) = self.extension_map.get(&ext_lower) {
                *scores.entry(cat.as_str()).or_default() += 2.0;
            }
        }

        for tracker in trackers {
            let domain = extract_domain(tracker);
            if let Some(cat) = self.tracker_map.get(&domain) {
                *scores.entry(cat.as_str()).or_default() += 1.5;
            }
        }

        for (category, keywords) in &self.category_keywords {
            for kw in keywords {
                let kw_lower = kw.to_lowercase();
                if tokens.iter().any(|t| *t == kw_lower) {
                    *scores.entry(category.as_str()).or_default() += 1.0;
                }
            }
        }

        let best = scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal));
        best.map(|(cat, score)| SuggestionResult {
            category: (*cat).to_string(),
            confidence: *score,
        })
    }

    pub fn train(
        &mut self,
        category: &str,
        name: &str,
        file_extensions: &[String],
        trackers: &[String],
    ) {
        let name_lower = name.to_lowercase();
        let tokens: Vec<String> = name_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() >= 3)
            .map(String::from)
            .collect();

        let keywords = self
            .category_keywords
            .entry(category.to_string())
            .or_default();
        for token in tokens {
            if !keywords.contains(&token) {
                keywords.push(token);
            }
        }

        for ext in file_extensions {
            let ext_lower = ext.to_lowercase();
            self.extension_map
                .entry(ext_lower)
                .or_insert_with(|| category.to_string());
        }

        for tracker in trackers {
            let domain = extract_domain(tracker);
            if !domain.is_empty() {
                self.tracker_map
                    .entry(domain)
                    .or_insert_with(|| category.to_string());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SuggestionResult {
    pub category: String,
    #[allow(
        dead_code,
        reason = "M202: used for ranking; exposed for future confidence display"
    )]
    pub confidence: f32,
}

fn extract_domain(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_has_extension_map() {
        let model = ClassifierModel::default();
        assert_eq!(model.extension_map.get("mkv"), Some(&"Video".to_string()));
        assert_eq!(model.extension_map.get("flac"), Some(&"Music".to_string()));
        assert_eq!(model.extension_map.get("pdf"), Some(&"Books".to_string()));
        assert_eq!(
            model.extension_map.get("iso"),
            Some(&"Software".to_string())
        );
    }

    #[test]
    fn suggest_by_extension() {
        let model = ClassifierModel::default();
        let result = model.suggest("Some Movie 2024", &["mkv".to_string()], &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().category, "Video");
    }

    #[test]
    fn suggest_by_multiple_extensions() {
        let model = ClassifierModel::default();
        let result = model.suggest(
            "Album",
            &["flac".to_string(), "flac".to_string(), "jpg".to_string()],
            &[],
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().category, "Music");
    }

    #[test]
    fn suggest_returns_none_for_unknown() {
        let model = ClassifierModel::default();
        let result = model.suggest("unknown thing", &[], &[]);
        assert!(result.is_none());
    }

    #[test]
    fn train_and_suggest_by_keyword() {
        let mut model = ClassifierModel::default();
        model.train("Anime", "Tokyo.Ghoul.S01.1080p", &["mkv".to_string()], &[]);
        let result = model.suggest("Tokyo Revengers S02", &[], &[]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().category, "Anime");
    }

    #[test]
    fn train_adds_tracker_domain() {
        let mut model = ClassifierModel::default();
        model.train(
            "Linux",
            "Ubuntu 24.04",
            &[],
            &["https://torrent.ubuntu.com/announce".to_string()],
        );
        assert_eq!(
            model.tracker_map.get("torrent.ubuntu.com"),
            Some(&"Linux".to_string())
        );
    }

    #[test]
    fn suggest_by_tracker() {
        let mut model = ClassifierModel::default();
        model
            .tracker_map
            .insert("tracker.example.com".to_string(), "Games".to_string());
        let result = model.suggest(
            "Something",
            &[],
            &["http://tracker.example.com:6969/announce".to_string()],
        );
        assert!(result.is_some());
        assert_eq!(result.unwrap().category, "Games");
    }

    #[test]
    fn extract_domain_cases() {
        assert_eq!(
            extract_domain("https://tracker.example.com/announce"),
            "tracker.example.com"
        );
        assert_eq!(
            extract_domain("http://tracker.example.com:6969/announce"),
            "tracker.example.com"
        );
        assert_eq!(
            extract_domain("udp://tracker.example.com:1337"),
            "tracker.example.com"
        );
        assert_eq!(extract_domain("tracker.example.com"), "tracker.example.com");
    }

    #[test]
    fn confidence_stacks() {
        let mut model = ClassifierModel::default();
        model
            .category_keywords
            .insert("Video".to_string(), vec!["movie".to_string()]);
        let result = model.suggest("Some Movie 2024", &["mkv".to_string()], &[]);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.category, "Video");
        assert!(r.confidence > 2.0);
    }

    #[test]
    fn train_deduplicates_keywords() {
        let mut model = ClassifierModel::default();
        model.train("Test", "hello world foo", &[], &[]);
        model.train("Test", "hello again bar", &[], &[]);
        let kws = model.category_keywords.get("Test").unwrap();
        let hello_count = kws.iter().filter(|k| *k == "hello").count();
        assert_eq!(hello_count, 1);
    }

    #[test]
    fn state_round_trip() {
        let mut model = ClassifierModel::default();
        model.train("TestCat", "test torrent name", &["txt".to_string()], &[]);
        let json = serde_json::to_string(&model).unwrap();
        let loaded: ClassifierModel = serde_json::from_str(&json).unwrap();
        assert!(loaded.category_keywords.contains_key("TestCat"));
    }
}
