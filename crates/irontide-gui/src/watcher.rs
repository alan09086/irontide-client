use std::path::Path;
use std::time::Duration;

use notify_debouncer_full::{DebounceEventResult, new_debouncer, notify::RecursiveMode};

use irontide_config::WatchedFolder;

#[must_use]
pub fn is_torrent_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("torrent"))
}

pub struct FolderWatcher {
    _debouncer: notify_debouncer_full::Debouncer<
        notify_debouncer_full::notify::RecommendedWatcher,
        notify_debouncer_full::RecommendedCache,
    >,
}

impl FolderWatcher {
    pub fn start(
        folders: &[WatchedFolder],
        event_tx: tokio::sync::mpsc::UnboundedSender<WatchEvent>,
    ) -> Option<Self> {
        if folders.is_empty() {
            return None;
        }

        let tx = event_tx;
        let folder_configs: Vec<WatchedFolder> = folders.to_vec();

        let debouncer = new_debouncer(
            Duration::from_millis(500),
            None,
            move |result: DebounceEventResult| {
                let Ok(events) = result else {
                    return;
                };
                for event in events {
                    use notify_debouncer_full::notify::EventKind;
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => {}
                        _ => continue,
                    }
                    for path in &event.paths {
                        if !is_torrent_file(path) || !path.is_file() {
                            continue;
                        }
                        let parent = path.parent().map(|p| p.to_string_lossy().into_owned());
                        let config = parent.as_ref().and_then(|parent_str| {
                            folder_configs.iter().find(|f| f.path == *parent_str)
                        });
                        if let Some(cfg) = config {
                            let _ = tx.send(WatchEvent {
                                path: path.clone(),
                                download_dir: cfg.download_dir.clone(),
                                add_paused: cfg.add_paused,
                                remove_after_add: cfg.remove_after_add,
                            });
                        }
                    }
                }
            },
        );

        let mut debouncer = match debouncer {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("failed to create filesystem watcher: {e}");
                return None;
            }
        };

        for folder in folders {
            let path = Path::new(&folder.path);
            if !path.is_dir() {
                tracing::warn!("watched folder does not exist: {}", folder.path);
                continue;
            }
            if let Err(e) = debouncer.watch(path, RecursiveMode::NonRecursive) {
                tracing::warn!("failed to watch {}: {e}", folder.path);
            } else {
                tracing::info!("watching folder for .torrent files: {}", folder.path);
            }
        }

        Some(Self {
            _debouncer: debouncer,
        })
    }
}

#[derive(Debug, Clone)]
pub struct WatchEvent {
    pub path: std::path::PathBuf,
    pub download_dir: Option<String>,
    pub add_paused: bool,
    pub remove_after_add: bool,
}

pub async fn process_watch_events(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<WatchEvent>,
    session: irontide::session::SessionHandle,
    weak: slint::Weak<crate::MainWindow>,
) {
    while let Some(event) = rx.recv().await {
        let path_display = event.path.file_name().map_or_else(
            || event.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );

        let bytes = match tokio::fs::read(&event.path).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    "failed to read watched torrent {}: {e}",
                    event.path.display()
                );
                continue;
            }
        };

        let mut params = irontide::AddTorrentParams::from_bytes(bytes);

        if let Some(dir) = &event.download_dir {
            params = params.download_dir(dir);
        }
        if event.add_paused {
            params = params.paused(true);
        }

        match params.add_to(&session).await {
            Ok(_) => {
                if event.remove_after_add
                    && let Err(e) = tokio::fs::remove_file(&event.path).await
                {
                    tracing::warn!("failed to remove watched torrent after add: {e}");
                }
                let msg = format!("Auto-added: {path_display}");
                crate::bridge::show_toast(&weak, &msg, false);
            }
            Err(e) => {
                let msg = format!("Failed to add {path_display}: {e}");
                crate::bridge::show_toast(&weak, &msg, true);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn is_torrent_file_accepts_torrent() {
        assert!(is_torrent_file(&PathBuf::from("test.torrent")));
        assert!(is_torrent_file(&PathBuf::from("/a/b/c.TORRENT")));
        assert!(is_torrent_file(&PathBuf::from("file.Torrent")));
    }

    #[test]
    fn is_torrent_file_rejects_non_torrent() {
        assert!(!is_torrent_file(&PathBuf::from("test.txt")));
        assert!(!is_torrent_file(&PathBuf::from("torrent")));
        assert!(!is_torrent_file(&PathBuf::from("test.torrent.bak")));
        assert!(!is_torrent_file(&PathBuf::from("")));
    }

    #[test]
    fn watched_folder_config_defaults() {
        let toml_str = r#"path = "/watch""#;
        let f: WatchedFolder = toml::from_str(toml_str).unwrap();
        assert_eq!(f.path, "/watch");
        assert!(f.download_dir.is_none());
        assert!(!f.add_paused);
        assert!(f.remove_after_add);
    }

    #[test]
    fn watched_folder_config_round_trip() {
        let f = WatchedFolder {
            path: "/watch".to_owned(),
            download_dir: Some("/dl".to_owned()),
            add_paused: true,
            remove_after_add: false,
        };
        let s = toml::to_string(&f).unwrap();
        let f2: WatchedFolder = toml::from_str(&s).unwrap();
        assert_eq!(f, f2);
    }

    #[test]
    fn config_file_empty_watched_folders_omitted() {
        let cf = irontide_config::ConfigFile::default();
        let s = toml::to_string(&cf).unwrap();
        assert!(!s.contains("watched_folder"));
    }
}
