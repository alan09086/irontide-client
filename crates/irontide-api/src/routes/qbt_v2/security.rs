//! Origin/Referer CSRF middleware (M172a Lane B).
//!
//! Mirrors qBt WebUI v2 semantics exactly:
//!
//! | Method              | Origin present | Referer present | Result                     |
//! |---------------------|----------------|-----------------|----------------------------|
//! | GET/HEAD/OPTIONS    | *              | *               | allow (short-circuit)      |
//! | POST/PATCH/PUT/DEL  | absent         | absent          | allow (server-to-server)   |
//! | POST/PATCH/PUT/DEL  | present        | *               | Origin's scheme+host+port  |
//! |                     |                |                 | must match Host; else 403  |
//! | POST/PATCH/PUT/DEL  | absent         | present         | Referer's origin-part must |
//! |                     |                |                 | match Host; else 403       |
//!
//! The "both absent → allow" rule is what makes `*arr` clients work — they
//! don't set Origin/Referer on server-to-server qBt calls. The browser-CSRF
//! threat model hinges on a browser *always* attaching one of the two to a
//! cross-origin request; if both are missing, the request can't have come
//! from a hostile tab.
//!
//! ## Reverse-proxy mode
//!
//! When `web_ui_reverse_proxy_enabled` is `true`, the middleware consults
//! [`super::state::resolve_client_ip`] to decide whether the peer is a
//! trusted forwarder. Trusted peers supply `X-Forwarded-Host` and
//! `X-Forwarded-Proto`; the Host-equality check is performed against those
//! values. Untrusted peers fall back to direct-Host validation — defence-
//! in-depth against an attacker spoofing XFH from outside the proxy layer.

use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::state::{QbtState, resolve_client_ip};

/// Parsed origin tuple `(scheme, host, port)` where `port` is `None` for
/// the URL scheme's default (`http`/`80`, `https`/`443`).
#[derive(Debug, PartialEq, Eq)]
struct Origin {
    scheme: String,
    host: String,
    port: Option<u16>,
}

/// CSRF guard middleware — applied via `route_layer(from_fn_with_state(...))`.
///
/// Re-reads `Settings.qbt_compat.csrf_protection_enabled` on every request so
/// runtime toggles via `setPreferences` take effect immediately (A7).
pub async fn csrf_guard(State(state): State<QbtState>, req: Request, next: Next) -> Response {
    // GET / HEAD / OPTIONS are guaranteed-idempotent in RFC 7231; browsers
    // may send them freely cross-origin without user interaction. The CSRF
    // threat model we're defending against doesn't apply here. We short-
    // circuit BEFORE the settings read so that a shutting-down session
    // (settings channel closed) doesn't wedge GET requests — they can
    // continue through the router to surface the real 503 at the handler.
    match *req.method() {
        Method::GET | Method::HEAD | Method::OPTIONS => return next.run(req).await,
        _ => {}
    }

    // Fetching settings is a channel round-trip: cheap, and the only way to
    // honour runtime reconfig without wiring a broadcast fan-out for a
    // handful of flags. If the fetch fails (shutting-down session) we err
    // closed — 403 beats open under a race on a mutating request.
    let Ok(settings) = state.session.settings().await else {
        return forbidden();
    };

    if !settings.qbt_compat.csrf_protection_enabled {
        return next.run(req).await;
    }

    // Lift the headers out up-front — we can't borrow the request across
    // `next.run(req)` which consumes it.
    let headers = req.headers().clone();
    let origin_header = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    let referer_header = headers.get(header::REFERER).and_then(|v| v.to_str().ok());

    // Both absent: server-to-server request (matches qBt; this is how
    // `*arr` clients issue their calls). Allow through.
    if origin_header.is_none() && referer_header.is_none() {
        return next.run(req).await;
    }

    // Resolve the Host we're about to validate against. Reverse-proxy mode
    // uses XFH/XFP only when the immediate peer is trusted — otherwise fall
    // back to the direct Host header (defence-in-depth against header-spoof
    // attacks from untrusted peers).
    let use_proxy_headers = settings.qbt_compat.web_ui_reverse_proxy_enabled && {
        let client_ip = resolve_client_ip(&req, &state);
        let proxies = state.reverse_proxies_list.read();
        proxies.iter().any(|net| net.contains(&client_ip))
    };

    let expected_authority = if use_proxy_headers {
        match expected_authority_from_xfh(&headers) {
            Some(a) => a,
            None => return forbidden(),
        }
    } else {
        match expected_authority_from_host(&headers) {
            Some(a) => a,
            None => return forbidden(),
        }
    };

    // Host-header validation is sometimes disabled separately (operator running
    // behind a reverse proxy that rewrites Host in ways we can't predict).
    if !settings.qbt_compat.host_header_validation_enabled {
        return next.run(req).await;
    }

    // Precedence: Origin is authoritative when present; Referer is only
    // consulted when Origin is absent. Matches qBt's behaviour and RFC 6265bis
    // guidance that Origin is the canonical indicator of the fetch's parent
    // context.
    let verdict = if let Some(origin) = origin_header {
        validate_origin(origin, &expected_authority)
    } else {
        // Unwrap: we've already short-circuited the both-absent case above.
        let referer = referer_header.expect("referer present when origin absent");
        validate_referer(referer, &expected_authority)
    };

    if verdict {
        next.run(req).await
    } else {
        forbidden()
    }
}

/// Single-origin authority `(scheme, host, port)` extracted from the request's
/// perspective — either the direct `Host` header or the proxy-supplied
/// `X-Forwarded-Host` + `X-Forwarded-Proto` pair.
#[derive(Debug)]
struct Authority {
    scheme: String,
    host: String,
    port: Option<u16>,
}

fn expected_authority_from_host(headers: &axum::http::HeaderMap) -> Option<Authority> {
    let host = headers.get(header::HOST)?.to_str().ok()?;
    // qBt's reference implementation assumes http:// when no TLS terminator
    // reached the handler — we do the same. When TLS lands in a later
    // milestone, this will need to consult the connector info.
    let (h, p) = split_host_port(host)?;
    Some(Authority {
        scheme: "http".into(),
        host: h,
        port: p,
    })
}

fn expected_authority_from_xfh(headers: &axum::http::HeaderMap) -> Option<Authority> {
    let xfh = headers.get("x-forwarded-host")?.to_str().ok()?;
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http")
        .to_owned();
    let (h, p) = split_host_port(xfh)?;
    Some(Authority {
        scheme,
        host: h,
        port: p,
    })
}

/// Split an `authority` value (Host header, XFH) into `(host, port)`. IPv6
/// literals like `[::1]:9080` must be handled — the rightmost `:` is the
/// port separator only when it follows a `]` or there's no other colon.
fn split_host_port(value: &str) -> Option<(String, Option<u16>)> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Bracketed IPv6 literal.
    if let Some(rest) = trimmed.strip_prefix('[') {
        let bracket_close = rest.find(']')?;
        let host = &rest[..bracket_close];
        let after = &rest[bracket_close + 1..];
        if after.is_empty() {
            return Some((host.to_owned(), None));
        }
        let port_str = after.strip_prefix(':')?;
        let port: u16 = port_str.parse().ok()?;
        return Some((host.to_owned(), Some(port)));
    }

    // Plain hostname / IPv4 / bare IPv6 (no port).
    if let Some((host, port)) = trimmed.rsplit_once(':') {
        // Reject "a:b:c" (that's a bare IPv6 — no brackets, no port) by
        // detecting more than one colon.
        if host.contains(':') {
            return Some((trimmed.to_owned(), None));
        }
        let port: u16 = port.parse().ok()?;
        return Some((host.to_owned(), Some(port)));
    }
    Some((trimmed.to_owned(), None))
}

fn validate_origin(origin_header: &str, expected: &Authority) -> bool {
    let Some(parsed) = parse_origin_or_url(origin_header) else {
        return false;
    };
    origins_match(&parsed, expected)
}

fn validate_referer(referer_header: &str, expected: &Authority) -> bool {
    // Referer is a full URL; we only care about the scheme+authority prefix,
    // so reuse the Origin parser which handles trailing-path stripping.
    let Some(parsed) = parse_origin_or_url(referer_header) else {
        return false;
    };
    origins_match(&parsed, expected)
}

/// Parse either an Origin header (scheme://authority) or a Referer URL into
/// the `(scheme, host, port)` tuple we care about. A Referer's path /
/// query / fragment are ignored. No dependency on the `url` crate — the
/// grammar we accept is deliberately narrow.
fn parse_origin_or_url(value: &str) -> Option<Origin> {
    let trimmed = value.trim();
    // scheme ends at "://". If there's no "://", this isn't a valid origin.
    let sep = trimmed.find("://")?;
    let scheme = trimmed[..sep].to_ascii_lowercase();
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    {
        return None;
    }
    let after = &trimmed[sep + 3..];
    // authority ends at first '/', '?', or '#'. Everything before that is
    // `user@host:port`; we don't accept userinfo (qBt doesn't either).
    let end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let authority = &after[..end];
    if authority.is_empty() {
        return None;
    }
    // Reject userinfo — `user@host` is rarely used and opens parser ambiguity.
    if authority.contains('@') {
        return None;
    }
    let (host, port) = split_host_port(authority)?;
    Some(Origin { scheme, host, port })
}

fn origins_match(candidate: &Origin, expected: &Authority) -> bool {
    if candidate.scheme != expected.scheme {
        return false;
    }
    if !candidate.host.eq_ignore_ascii_case(&expected.host) {
        return false;
    }
    // Normalise default ports: http/80 and https/443 are equivalent whether
    // the client wrote ":80" explicitly or omitted it entirely.
    let cand_port = normalise_default_port(&candidate.scheme, candidate.port);
    let exp_port = normalise_default_port(&expected.scheme, expected.port);
    cand_port == exp_port
}

fn normalise_default_port(scheme: &str, port: Option<u16>) -> Option<u16> {
    match (scheme, port) {
        ("http", Some(80)) | ("https", Some(443)) => None,
        (_, p) => p,
    }
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, "Fails.").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_host_port_plain_no_port() {
        assert_eq!(
            split_host_port("localhost"),
            Some(("localhost".into(), None))
        );
    }

    #[test]
    fn split_host_port_with_port() {
        assert_eq!(
            split_host_port("localhost:9080"),
            Some(("localhost".into(), Some(9080)))
        );
    }

    #[test]
    fn split_host_port_ipv6_bracketed() {
        assert_eq!(
            split_host_port("[::1]:9080"),
            Some(("::1".into(), Some(9080)))
        );
        assert_eq!(split_host_port("[::1]"), Some(("::1".into(), None)));
    }

    #[test]
    fn split_host_port_rejects_bad_port() {
        assert_eq!(split_host_port("localhost:abc"), None);
    }

    #[test]
    fn default_port_normalisation() {
        assert_eq!(normalise_default_port("http", Some(80)), None);
        assert_eq!(normalise_default_port("https", Some(443)), None);
        assert_eq!(normalise_default_port("http", Some(8080)), Some(8080));
        assert_eq!(normalise_default_port("http", None), None);
    }

    #[test]
    fn origin_match_simple() {
        let exp = Authority {
            scheme: "http".into(),
            host: "localhost".into(),
            port: Some(9080),
        };
        assert!(validate_origin("http://localhost:9080", &exp));
        assert!(!validate_origin("http://evil.example.com", &exp));
        assert!(!validate_origin("https://localhost:9080", &exp));
    }

    #[test]
    fn referer_match_with_path_ignored() {
        let exp = Authority {
            scheme: "http".into(),
            host: "localhost".into(),
            port: Some(9080),
        };
        assert!(validate_referer(
            "http://localhost:9080/webui/torrents/abc",
            &exp
        ));
        assert!(!validate_referer(
            "http://evil.example.com/webui/torrents",
            &exp
        ));
    }

    #[test]
    fn origin_case_insensitive_host() {
        let exp = Authority {
            scheme: "http".into(),
            host: "localhost".into(),
            port: Some(9080),
        };
        assert!(validate_origin("http://LOCALHOST:9080", &exp));
    }
}
