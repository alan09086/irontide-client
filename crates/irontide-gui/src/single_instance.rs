use std::io;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

#[derive(Debug, Clone)]
pub enum InstanceMessage {
    OpenFile(PathBuf),
    OpenMagnet(String),
    Show,
}

impl InstanceMessage {
    fn serialize(&self) -> String {
        match self {
            Self::OpenFile(p) => format!("FILE:{}\n", p.display()),
            Self::OpenMagnet(uri) => format!("MAGNET:{uri}\n"),
            Self::Show => "SHOW\n".to_owned(),
        }
    }

    fn parse(line: &str) -> Option<Self> {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("FILE:") {
            Some(Self::OpenFile(PathBuf::from(rest)))
        } else if let Some(rest) = line.strip_prefix("MAGNET:") {
            Some(Self::OpenMagnet(rest.to_owned()))
        } else if line == "SHOW" {
            Some(Self::Show)
        } else {
            None
        }
    }
}

pub fn socket_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        Path::new(&dir).join("irontide-gui.sock")
    } else {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/irontide-gui-{uid}.sock"))
    }
}

pub enum AcquireResult {
    Primary(InstanceGuard),
    Secondary,
}

pub struct InstanceGuard {
    #[cfg(unix)]
    listener: Option<UnixListener>,
    path: PathBuf,
}

impl InstanceGuard {
    #[cfg(unix)]
    pub fn acquire(messages: &[InstanceMessage]) -> io::Result<AcquireResult> {
        let path = socket_path();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        match UnixListener::bind(&path) {
            Ok(listener) => {
                listener.set_nonblocking(true)?;
                Ok(AcquireResult::Primary(Self {
                    listener: Some(listener),
                    path,
                }))
            }
            Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                if let Ok(mut stream) = UnixStream::connect(&path) {
                    use std::io::Write;
                    let payload = if messages.is_empty() {
                        InstanceMessage::Show.serialize()
                    } else {
                        messages.iter().map(InstanceMessage::serialize).collect()
                    };
                    stream.write_all(payload.as_bytes())?;
                    stream.shutdown(std::net::Shutdown::Write)?;
                    Ok(AcquireResult::Secondary)
                } else {
                    // Stale socket — remove and retry.
                    let _ = std::fs::remove_file(&path);
                    let listener = UnixListener::bind(&path)?;
                    listener.set_nonblocking(true)?;
                    Ok(AcquireResult::Primary(Self {
                        listener: Some(listener),
                        path,
                    }))
                }
            }
            Err(e) => Err(e),
        }
    }

    #[cfg(unix)]
    pub fn spawn_listener(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<InstanceMessage>,
    ) {
        let Some(std_listener) = self.listener.take() else {
            return;
        };

        std::thread::Builder::new()
            .name("ipc-listener".into())
            .spawn(move || {
                std_listener.set_nonblocking(false).ok();
                for stream in std_listener.incoming() {
                    match stream {
                        Ok(mut conn) => {
                            use std::io::Read;
                            let mut buf = String::new();
                            if conn.read_to_string(&mut buf).is_ok() {
                                for line in buf.lines() {
                                    if let Some(msg) = InstanceMessage::parse(line)
                                        && tx.send(msg).is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!("IPC accept error: {e}");
                        }
                    }
                }
            })
            .ok();
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn parse_cli_arg(arg: &str) -> Option<InstanceMessage> {
    if arg.starts_with("magnet:") {
        Some(InstanceMessage::OpenMagnet(arg.to_owned()))
    } else {
        let path = PathBuf::from(arg);
        if crate::watcher::is_torrent_file(&path) || arg.ends_with(".torrent") {
            let abs = if path.is_absolute() {
                path
            } else {
                std::env::current_dir().ok()?.join(path)
            };
            Some(InstanceMessage::OpenFile(abs))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_serialize_round_trip_file() {
        let msg = InstanceMessage::OpenFile(PathBuf::from("/tmp/test.torrent"));
        let s = msg.serialize();
        let parsed = InstanceMessage::parse(&s).unwrap();
        assert!(matches!(parsed, InstanceMessage::OpenFile(ref p) if p.as_path() == Path::new("/tmp/test.torrent")));
    }

    #[test]
    fn message_serialize_round_trip_magnet() {
        let uri = "magnet:?xt=urn:btih:abc123";
        let msg = InstanceMessage::OpenMagnet(uri.to_owned());
        let s = msg.serialize();
        let parsed = InstanceMessage::parse(&s).unwrap();
        assert!(matches!(parsed, InstanceMessage::OpenMagnet(u) if u == uri));
    }

    #[test]
    fn message_serialize_round_trip_show() {
        let msg = InstanceMessage::Show;
        let s = msg.serialize();
        let parsed = InstanceMessage::parse(&s).unwrap();
        assert!(matches!(parsed, InstanceMessage::Show));
    }

    #[test]
    fn parse_ignores_unknown() {
        assert!(InstanceMessage::parse("UNKNOWN:data").is_none());
        assert!(InstanceMessage::parse("").is_none());
    }

    #[test]
    fn socket_path_is_absolute() {
        let p = socket_path();
        assert!(p.is_absolute());
        assert!(p.to_string_lossy().contains("irontide-gui"));
    }

    #[test]
    fn parse_cli_arg_magnet() {
        let msg = parse_cli_arg("magnet:?xt=urn:btih:abc").unwrap();
        assert!(matches!(msg, InstanceMessage::OpenMagnet(u) if u.starts_with("magnet:")));
    }

    #[test]
    fn parse_cli_arg_torrent_file() {
        let msg = parse_cli_arg("/tmp/test.torrent").unwrap();
        assert!(matches!(msg, InstanceMessage::OpenFile(p) if p == Path::new("/tmp/test.torrent")));
    }

    #[test]
    fn parse_cli_arg_rejects_non_torrent() {
        assert!(parse_cli_arg("/tmp/readme.txt").is_none());
        assert!(parse_cli_arg("https://example.com").is_none());
    }

    #[test]
    fn socket_path_ends_with_irontide_gui_sock() {
        let p = socket_path();
        assert!(
            p.to_string_lossy().ends_with("irontide-gui.sock"),
            "socket path should end with irontide-gui.sock: {p:?}"
        );
    }
}
