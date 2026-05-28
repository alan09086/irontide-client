//! Integration tests for M172a Lane B — Origin/Referer CSRF middleware.
//!
//! Uses real `TcpListener::bind(0)` + `ApiServer::run` (rather than the
//! `tower::ServiceExt::oneshot` path) so `ConnectInfo<SocketAddr>` is
//! populated from the actual TCP peer address. The CSRF middleware's
//! reverse-proxy trust-hop logic (A7 / G5) is only meaningful with a real
//! peer address, so the oneshot path is unsuitable for these tests.
//!
//! Each test spins up a dedicated session + `ApiServer` on `127.0.0.1:0`,
//! captures the OS-assigned port, and drives requests via `reqwest`.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Method, Request};

use irontide::session::Settings;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn test_session(customize: impl FnOnce(&mut Settings)) -> irontide::session::SessionHandle {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-qbt-csrf-{}-{}", std::process::id(), n));
    let _ = std::fs::remove_dir_all(&resume_dir);

    let mut settings = Settings {
        listen_port: 0,
        download_dir: std::path::PathBuf::from("/tmp"),
        enable_dht: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(resume_dir),
        save_resume_interval_secs: 0,
        ..Settings::default()
    };
    settings.qbt_compat.enabled = true;
    customize(&mut settings);
    irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("failed to start session")
}

/// Spin up an `ApiServer` on an ephemeral port and return `(base_url, handle,
/// session)`. The handle is retained by callers for the lifetime of the test;
/// dropping it aborts the server task.
async fn tcp_server(
    customize: impl FnOnce(&mut Settings),
) -> (
    String,
    tokio::task::JoinHandle<std::io::Result<()>>,
    irontide::session::SessionHandle,
) {
    let session = test_session(customize).await;
    let server = irontide_api::ApiServer::bind(
        "127.0.0.1:0".parse::<SocketAddr>().expect("valid addr"),
        session.clone(),
    )
    .await
    .expect("bind");
    let base = format!("http://{}", server.local_addr());
    let handle = tokio::spawn(server.run());
    // Wait for the server to accept connections — a small sleep is the simplest
    // approach and the alternative (loop until reqwest succeeds) adds complexity
    // without substantively better robustness for unit tests.
    tokio::time::sleep(Duration::from_millis(20)).await;
    (base, handle, session)
}

fn reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build reqwest client")
}

async fn login_via_tcp(client: &reqwest::Client, base: &str) -> reqwest::StatusCode {
    client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("login sent")
        .status()
}

// ── 1: GETs allowed regardless of headers ───────────────────────────────

#[tokio::test]
async fn get_request_allowed_regardless_of_headers() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();

    // Login is a POST; use it to obtain a cookie for the GET probe.
    let st = login_via_tcp(&client, &base).await;
    assert_eq!(st.as_u16(), 200, "initial login should succeed");

    // GET with a cross-origin Referer header: the CSRF guard must allow it.
    let resp = client
        .get(format!("{base}/api/v2/app/version"))
        .header("Referer", "http://evil.example.com/csrf")
        .send()
        .await
        .expect("GET sent");
    assert_eq!(resp.status().as_u16(), 200);
}

// ── 2: POST with matching Origin passes ────────────────────────────────

#[tokio::test]
async fn post_with_matching_origin_passes() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    // Host is "127.0.0.1:PORT"; Origin must match.
    let host = base.trim_start_matches("http://");
    let origin = format!("http://{host}");
    let resp = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("Origin", origin)
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
}

// ── 3: Mismatched Origin → 403 ─────────────────────────────────────────

#[tokio::test]
async fn post_with_mismatched_origin_rejected_403() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    let resp = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("Origin", "http://evil.example.com")
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 403);
}

// ── 4: POST with matching Referer passes (Origin absent) ───────────────

#[tokio::test]
async fn post_with_matching_referer_passes() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    let host = base.trim_start_matches("http://");
    let referer = format!("http://{host}/webui/");
    let resp = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("Referer", referer)
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
}

// ── 5: Mismatched Referer → 403 (Origin absent) ────────────────────────

#[tokio::test]
async fn post_with_mismatched_referer_rejected_403() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    let resp = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("Referer", "http://evil.example.com/webui/")
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 403);
}

// ── 6: Neither Origin nor Referer → allow (qBt parity, *arr path) ──────

#[tokio::test]
async fn post_with_neither_header_allowed_qbt_parity() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    // reqwest leaves Origin/Referer empty on a bare POST unless we set them.
    let resp = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
}

// ── 7: Origin overrides Referer ────────────────────────────────────────

#[tokio::test]
async fn post_with_origin_overrides_referer() {
    // Rule: Origin is authoritative when present. A matching Origin +
    // mismatched Referer must PASS (Origin wins); a mismatched Origin +
    // matching Referer must FAIL.
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    let host = base.trim_start_matches("http://");
    let good_origin = format!("http://{host}");

    // Matching Origin + mismatched Referer → allow.
    let ok = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("Origin", &good_origin)
        .header("Referer", "http://evil.example.com/csrf")
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send good-origin");
    assert_eq!(ok.status().as_u16(), 200);

    // Mismatched Origin + matching Referer → reject.
    let good_referer = format!("http://{host}/webui/");
    let bad = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("Origin", "http://evil.example.com")
        .header("Referer", good_referer)
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send bad-origin");
    assert_eq!(bad.status().as_u16(), 403);
}

// ── 8: CSRF disabled in settings → allow all ──────────────────────────

#[tokio::test]
async fn csrf_disabled_in_settings_passes_all_requests() {
    let (base, _handle, _session) = tcp_server(|s| {
        s.qbt_compat.csrf_protection_enabled = false;
    })
    .await;
    let client = reqwest_client();
    let resp = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("Origin", "http://evil.example.com")
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
}

// ── 9: Reverse-proxy mode trusts XFH when peer is in CIDR list ────────

#[tokio::test]
async fn reverse_proxy_mode_trusts_x_forwarded_host() {
    // Trust the loopback peer address, send Origin matching XFH. The
    // middleware must validate Origin against XFH (via XFP scheme) not
    // the direct Host header.
    let (base, _handle, _session) = tcp_server(|s| {
        s.qbt_compat.web_ui_reverse_proxy_enabled = true;
        s.qbt_compat
            .web_ui_reverse_proxies_list
            .push("127.0.0.1/32".to_owned());
    })
    .await;
    let client = reqwest_client();
    let resp = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("X-Forwarded-Host", "public.example.com")
        .header("X-Forwarded-Proto", "https")
        .header("Origin", "https://public.example.com")
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
}

// ── 10: Reverse-proxy mode rejects untrusted peer ─────────────────────

#[tokio::test]
async fn reverse_proxy_mode_rejects_untrusted_source_ip() {
    // Same settings as above but DON'T trust 127.0.0.1. The peer is not in
    // the CIDR list, so the middleware falls back to direct-Host validation.
    // The Origin header targets XFH, not the loopback Host, so it mismatches.
    let (base, _handle, _session) = tcp_server(|s| {
        s.qbt_compat.web_ui_reverse_proxy_enabled = true;
        // Trust a different CIDR — 10.10.0.0/24 certainly doesn't contain
        // 127.0.0.1.
        s.qbt_compat
            .web_ui_reverse_proxies_list
            .push("10.10.0.0/24".to_owned());
    })
    .await;
    let client = reqwest_client();
    let resp = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("X-Forwarded-Host", "public.example.com")
        .header("X-Forwarded-Proto", "https")
        .header("Origin", "https://public.example.com")
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 403);
}

// ── 11: /webui/* routes are CSRF protected ─────────────────────────────

#[tokio::test]
async fn webui_routes_also_csrf_protected() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    // A cross-origin POST to /webui/preferences/save must 403.
    let resp = client
        .post(format!("{base}/webui/preferences/save"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("Origin", "http://evil.example.com")
        .body("max_peers_per_torrent=200")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 403);
}

// ── 12: /api/v2/* routes are CSRF protected ────────────────────────────

#[tokio::test]
async fn api_v2_routes_also_csrf_protected() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    // Log in first so the cookie passes require_sid.
    login_via_tcp(&client, &base).await;
    let resp = client
        .post(format!("{base}/api/v2/torrents/pause?hashes=all"))
        .header("Origin", "http://evil.example.com")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 403);
}

// ── 13: Host-header validation disabled → skip Origin check ───────────

#[tokio::test]
async fn host_header_validation_disabled_skips_origin_check() {
    let (base, _handle, _session) = tcp_server(|s| {
        s.qbt_compat.host_header_validation_enabled = false;
    })
    .await;
    let client = reqwest_client();
    let resp = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .header("Origin", "http://evil.example.com")
        .body("username=admin&password=adminadmin")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 200);
}

// ── 14: live setPreferences updates reverse-proxy CIDR list (A7) ─────

#[tokio::test]
async fn live_set_preferences_updates_reverse_proxy_cidr_list() {
    // Start with no trusted proxies → same pattern as test 10 (untrusted).
    // Mid-test, call setPreferences to trust 127.0.0.1/32. Next request
    // with XFH targeting evil.example.com + matching Origin must now pass.
    let (base, _handle, _session) = tcp_server(|s| {
        s.qbt_compat.web_ui_reverse_proxy_enabled = true;
    })
    .await;
    let client = reqwest_client();

    // Login (cookie-jar auto-captures SID).
    let st = login_via_tcp(&client, &base).await;
    assert_eq!(st.as_u16(), 200);

    // Before the update: XFH-spoofed request must 403.
    let before = client
        .post(format!("{base}/api/v2/torrents/pause?hashes=all"))
        .header("X-Forwarded-Host", "public.example.com")
        .header("X-Forwarded-Proto", "http")
        .header("Origin", "http://public.example.com")
        .send()
        .await
        .expect("before");
    assert_eq!(before.status().as_u16(), 403);

    // Patch trust list via setPreferences.
    let patch = serde_json::json!({
        "web_ui_reverse_proxies_list": "127.0.0.1/32",
    })
    .to_string();
    let host = base.trim_start_matches("http://");
    let good_origin = format!("http://{host}");
    let resp = client
        .post(format!("{base}/api/v2/app/setPreferences"))
        .header("content-type", "application/json")
        .header("Origin", &good_origin)
        .body(patch)
        .send()
        .await
        .expect("setPrefs");
    assert_eq!(resp.status().as_u16(), 200);

    // After the update: the same XFH-spoofed call must now pass.
    let after = client
        .post(format!("{base}/api/v2/torrents/pause?hashes=all"))
        .header("X-Forwarded-Host", "public.example.com")
        .header("X-Forwarded-Proto", "http")
        .header("Origin", "http://public.example.com")
        .send()
        .await
        .expect("after");
    assert_eq!(after.status().as_u16(), 200);
}

// ── 15-17: XFF trust-hop resolution (G5) ──────────────────────────────

use irontide_api::routes::qbt_v2::{
    QbtState, SessionStore, default_argon2_permits, resolve_client_ip,
};
use std::sync::Arc;

fn build_request_with_xff(xff: Option<&str>, peer_ip: [u8; 4]) -> Request<Body> {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri("http://localhost/");
    if let Some(v) = xff {
        builder = builder.header("x-forwarded-for", v);
    }
    let mut req = builder.body(Body::empty()).expect("build request");
    let connect_info = axum::extract::ConnectInfo(SocketAddr::from((peer_ip, 65000)));
    req.extensions_mut().insert(connect_info);
    req
}

async fn state_with_trusted_cidrs(cidrs: &[&str]) -> QbtState {
    let session = test_session(|_| {}).await;
    let store = Arc::new(SessionStore::new(Duration::from_hours(1), 16));
    let state = QbtState::new(
        Arc::new(session),
        store,
        default_argon2_permits(None),
        1_000,
    );
    let parsed: Vec<ipnet::IpNet> = cidrs
        .iter()
        .map(|s| s.parse().expect("valid CIDR"))
        .collect();
    *state.reverse_proxies_list.write() = parsed;
    state
}

#[tokio::test]
async fn xff_chain_takes_last_untrusted_hop() {
    // X-Forwarded-For: 192.0.2.5, 10.0.0.99  — peer 10.0.0.50
    // Trust list: [10.0.0.0/24] → client = 192.0.2.5 (only untrusted hop).
    let state = state_with_trusted_cidrs(&["10.0.0.0/24"]).await;
    let req = build_request_with_xff(Some("192.0.2.5, 10.0.0.99"), [10, 0, 0, 50]);
    let ip = resolve_client_ip(&req, &state);
    assert_eq!(ip.to_string(), "192.0.2.5");

    // Now trust 192.0.2.0/24 too → every hop is trusted → fall back to
    // chain[0] = 192.0.2.5 (the leftmost claimed client).
    let state2 = state_with_trusted_cidrs(&["10.0.0.0/24", "192.0.2.0/24"]).await;
    let req2 = build_request_with_xff(Some("192.0.2.5, 10.0.0.99"), [10, 0, 0, 50]);
    let ip2 = resolve_client_ip(&req2, &state2);
    assert_eq!(ip2.to_string(), "192.0.2.5");
}

#[tokio::test]
async fn xff_chain_all_trusted_falls_back_to_first() {
    // With trust = 10.0.0.0/8 covering everyone, chain[0] (leftmost) wins.
    let state = state_with_trusted_cidrs(&["10.0.0.0/8"]).await;
    let req = build_request_with_xff(Some("10.1.2.3, 10.4.5.6"), [10, 7, 8, 9]);
    let ip = resolve_client_ip(&req, &state);
    assert_eq!(ip.to_string(), "10.1.2.3");
}

#[tokio::test]
async fn xff_bare_peer_without_xff_header() {
    // No XFF header → chain = [peer]; peer isn't in trust list → return peer.
    let state = state_with_trusted_cidrs(&["10.0.0.0/24"]).await;
    let req = build_request_with_xff(None, [192, 0, 2, 1]);
    let ip = resolve_client_ip(&req, &state);
    assert_eq!(ip.to_string(), "192.0.2.1");
}

// ── M174: v1 API routes are CSRF protected ────────────────────────────

#[tokio::test]
async fn v1_api_post_rejected_by_csrf_with_cross_origin() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    let resp = client
        .post(format!("{base}/api/v1/session/shutdown"))
        .header("Origin", "http://evil.example.com")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status().as_u16(), 403);
}

#[tokio::test]
async fn v1_api_get_passes_csrf_with_cross_origin() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = reqwest_client();
    let resp = client
        .get(format!("{base}/api/v1/session/stats"))
        .header("Origin", "http://evil.example.com")
        .send()
        .await
        .expect("send");
    assert_ne!(
        resp.status().as_u16(),
        403,
        "GET requests must bypass CSRF guard"
    );
}
