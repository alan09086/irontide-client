//! System clipboard wrapper (M217 — Copy Magnet Link).
//!
//! Slint 1.x has no stable application-facing clipboard API, so the GUI
//! depends on `arboard` for clipboard writes. This module provides a
//! one-call surface so call sites don't need to know about arboard's
//! Result types.
//!
//! M217 ships a hash-only magnet URI per BEP-9. Name + tracker
//! enrichment requires async metadata lookup against the engine and
//! is deferred to a later milestone — the hash-only form is a valid
//! magnet URI accepted by every mainstream `BitTorrent` client (qBittorrent,
//! Transmission, Deluge) and resolves swarm + metadata via DHT/PEX.

use crate::magnet::build_magnet_uri;

/// Errors that can occur during clipboard operations.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    /// The arboard backend failed to initialize. On Linux this usually
    /// means no X11/Wayland clipboard server is reachable.
    #[error("clipboard backend unavailable: {0}")]
    Backend(#[from] arboard::Error),
    /// The provided info-hash was empty (stale palette dispatch with no
    /// selection).
    #[error("no torrent selected")]
    NoSelection,
}

/// Build and copy a magnet URI for the given torrent info-hash. Produces
/// a hash-only magnet (`magnet:?xt=urn:btih:<hash>`) — valid per BEP-9
/// and resolves to full swarm metadata via DHT/PEX in any mainstream
/// `BitTorrent` client.
pub fn copy_magnet_for_hash(hash: &str) -> Result<String, ClipboardError> {
    if hash.is_empty() {
        return Err(ClipboardError::NoSelection);
    }
    let uri = build_magnet_uri(hash, "", &[]);
    set_clipboard_text(&uri)?;
    Ok(uri)
}

/// Write `text` to the system clipboard. Pure passthrough over arboard
/// so call sites get a single error surface.
pub fn set_clipboard_text(text: &str) -> Result<(), ClipboardError> {
    let mut cb = arboard::Clipboard::new()?;
    cb.set_text(text.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_magnet_for_hash_rejects_empty() {
        let err = copy_magnet_for_hash("").unwrap_err();
        assert!(matches!(err, ClipboardError::NoSelection));
    }

    /// Headless CI doesn't have a clipboard server; this test verifies the
    /// arboard round-trip when run locally. Marked `#[ignore]` so `cargo
    /// test --workspace` stays green on systems without X11/Wayland.
    #[test]
    #[ignore = "requires an X11/Wayland clipboard backend"]
    fn clipboard_round_trips_text() {
        let text = "irontide M217 clipboard test";
        set_clipboard_text(text).expect("clipboard write succeeded");
        let mut cb = arboard::Clipboard::new().expect("clipboard read backend");
        let read = cb.get_text().expect("clipboard read");
        assert_eq!(read, text);
    }
}
