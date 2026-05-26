//! Magnet URI construction (M217 — Copy Magnet Link).
//!
//! Slint 1.x has no application-facing clipboard API, so this module pairs
//! with `crate::clipboard` to support the per-row Copy Magnet action and
//! the `PaletteCommandId::CopyMagnetLink` palette command.
//!
//! The magnet URI format follows BEP-9 and the de facto magnet-link
//! convention used by qBittorrent / Transmission / Deluge:
//!
//! ```text
//! magnet:?xt=urn:btih:<info_hash>&dn=<name>&tr=<tracker1>&tr=<tracker2>
//! ```
//!
//! `info_hash` is rendered lowercase hex. `name` and each tracker URL are
//! percent-encoded per RFC 3986 unreserved-character rules.

/// Percent-encode a string for use in a URI query value.
///
/// Encodes everything that isn't an unreserved character per RFC 3986
/// (alphanumeric, `-`, `_`, `.`, `~`). The output is ASCII-safe and
/// suitable for inclusion in `dn=` and `tr=` query parameters.
fn percent_encode(input: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Build a magnet URI from a torrent's info-hash, display name, and tracker list.
///
/// `info_hash` should be a 40-character hex string (BEP-9 v1) or 64-character
/// hex (BEP-52 v2 — irontide doesn't ship v2 yet, but the formatter is
/// agnostic). The hash is lowercased to match qBittorrent / Transmission's
/// rendering convention.
///
/// `name` is percent-encoded into the `dn=` parameter. Empty name omits `dn=`.
///
/// Each entry in `trackers` becomes one `tr=` parameter, percent-encoded.
/// Empty tracker list is legal — DHT-only swarms produce magnet URIs with
/// just `xt=`.
#[must_use]
pub fn build_magnet_uri(info_hash: &str, name: &str, trackers: &[String]) -> String {
    let mut uri = format!("magnet:?xt=urn:btih:{}", info_hash.to_lowercase());
    if !name.is_empty() {
        uri.push_str("&dn=");
        uri.push_str(&percent_encode(name));
    }
    for tracker in trackers {
        if tracker.is_empty() {
            continue;
        }
        uri.push_str("&tr=");
        uri.push_str(&percent_encode(tracker));
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnet_uri_with_no_trackers_and_no_name() {
        let uri = build_magnet_uri("ABCDEF0123456789ABCDEF0123456789ABCDEF01", "", &[]);
        assert_eq!(
            uri,
            "magnet:?xt=urn:btih:abcdef0123456789abcdef0123456789abcdef01"
        );
    }

    #[test]
    fn magnet_uri_lowercases_info_hash() {
        let uri = build_magnet_uri("DEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF", "x", &[]);
        assert!(uri.contains("xt=urn:btih:deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"));
    }

    #[test]
    fn magnet_uri_includes_name_and_trackers() {
        let trackers = vec![
            "udp://tracker.example.com:6969/announce".to_string(),
            "http://other.example.org/announce".to_string(),
        ];
        let uri = build_magnet_uri("0011223344556677889900112233445566778899", "Ubuntu", &trackers);
        assert!(uri.contains("&dn=Ubuntu"));
        assert!(uri.contains("&tr=udp%3A%2F%2Ftracker.example.com%3A6969%2Fannounce"));
        assert!(uri.contains("&tr=http%3A%2F%2Fother.example.org%2Fannounce"));
    }

    #[test]
    fn magnet_uri_percent_encodes_name_with_spaces_and_symbols() {
        let uri = build_magnet_uri(
            "0011223344556677889900112233445566778899",
            "Ubuntu 24.04 LTS (amd64) [release]",
            &[],
        );
        assert!(uri.contains("&dn=Ubuntu%2024.04%20LTS%20%28amd64%29%20%5Brelease%5D"));
    }

    #[test]
    fn magnet_uri_skips_empty_tracker_entries() {
        let trackers = vec![
            String::new(),
            "udp://tracker.example.com:6969/announce".to_string(),
        ];
        let uri = build_magnet_uri("0011223344556677889900112233445566778899", "x", &trackers);
        let tr_count = uri.matches("&tr=").count();
        assert_eq!(tr_count, 1, "expected exactly one &tr= for the non-empty tracker");
    }

    #[test]
    fn percent_encode_preserves_unreserved_characters() {
        assert_eq!(percent_encode("abc-XYZ_0.9~"), "abc-XYZ_0.9~");
    }

    #[test]
    fn percent_encode_encodes_reserved_characters() {
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("/"), "%2F");
        assert_eq!(percent_encode(":"), "%3A");
    }
}
