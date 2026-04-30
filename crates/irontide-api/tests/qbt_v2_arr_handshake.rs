//! Real-TCP simulated *arr handshake smoke tests (M172a Lane B G7).
//!
//! Replays the exact request sequence Radarr/Sonarr/Prowlarr/Lidarr issue
//! against a qBt v2 server, with matching User-Agent strings and — critically
//! — NO Origin / NO Referer headers. The CSRF absent-both-allow rule is what
//! makes these clients work; if the rule ever regresses, every `*arr` Test
//! Connection button turns red simultaneously.
//!
//! All tests use `reqwest` with a cookie jar against an `ApiServer` bound on
//! an ephemeral port. The per-test session is fully isolated via a unique
//! resume-data directory.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use irontide::session::Settings;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn test_session(customize: impl FnOnce(&mut Settings)) -> irontide::session::SessionHandle {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-qbt-arr-hs-{}-{}", std::process::id(), n));
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
    tokio::time::sleep(Duration::from_millis(20)).await;
    (base, handle, session)
}

fn arr_client(user_agent: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .user_agent(user_agent)
        .timeout(Duration::from_secs(5))
        .build()
        .expect("build reqwest client")
}

/// The canonical 5-call *arr handshake: login → webapiVersion → preferences →
/// categories → add magnet. Returns on first failure with a panic message that
/// names the failing step. Uses reqwest's cookie jar implicitly, so SID flows
/// between calls without explicit Cookie header management.
async fn arr_handshake_sequence(
    base: &str,
    client: &reqwest::Client,
    extra_headers: &[(&str, &str)],
    magnet: &str,
) -> Vec<reqwest::StatusCode> {
    let mut out = Vec::with_capacity(5);
    // 1. Login.
    let mut req = client
        .post(format!("{base}/api/v2/auth/login"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body("username=admin&password=adminadmin");
    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }
    out.push(req.send().await.expect("login").status());

    // 2. webapiVersion.
    let mut req = client.get(format!("{base}/api/v2/app/webapiVersion"));
    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }
    out.push(req.send().await.expect("webapiVersion").status());

    // 3. preferences.
    let mut req = client.get(format!("{base}/api/v2/app/preferences"));
    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }
    out.push(req.send().await.expect("preferences").status());

    // 4. categories.
    let mut req = client.get(format!("{base}/api/v2/torrents/categories"));
    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }
    out.push(req.send().await.expect("categories").status());

    // 5. add magnet.
    let mut body = String::from("urls=");
    for b in magnet.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                body.push(b as char);
            }
            _ => { use std::fmt::Write; let _ = write!(body, "%{b:02X}"); }
        }
    }
    let mut req = client
        .post(format!("{base}/api/v2/torrents/add"))
        .header("content-type", "application/x-www-form-urlencoded")
        .body(body);
    for (k, v) in extra_headers {
        req = req.header(*k, *v);
    }
    out.push(req.send().await.expect("add").status());

    out
}

const TEST_MAGNET: &str =
    "magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c&dn=BigBuckBunny";

#[tokio::test]
async fn simulated_radarr_test_connection_succeeds_absent_origin_referer() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = arr_client("Radarr/5.27.0");
    let statuses = arr_handshake_sequence(&base, &client, &[], TEST_MAGNET).await;
    for (i, s) in statuses.iter().enumerate() {
        assert!(
            s.is_success(),
            "step {} failed with status {s}: {statuses:?}",
            i + 1
        );
    }
}

#[tokio::test]
async fn simulated_sonarr_test_connection_succeeds() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = arr_client("Sonarr/4.0.15");
    let statuses = arr_handshake_sequence(&base, &client, &[], TEST_MAGNET).await;
    for (i, s) in statuses.iter().enumerate() {
        assert!(
            s.is_success(),
            "step {} failed with status {s}: {statuses:?}",
            i + 1
        );
    }
}

#[tokio::test]
async fn simulated_prowlarr_app_sync_succeeds() {
    // Prowlarr's sync flow is a subset of the full handshake — only the first
    // three calls (login → webapiVersion → preferences) need to succeed for a
    // green Prowlarr status.
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = arr_client("Prowlarr/1.28.2");
    let statuses = arr_handshake_sequence(&base, &client, &[], TEST_MAGNET).await;
    for (i, s) in statuses.iter().take(3).enumerate() {
        assert!(
            s.is_success(),
            "prowlarr sync step {} failed with {s}: {statuses:?}",
            i + 1
        );
    }
}

#[tokio::test]
async fn malicious_browser_cross_origin_post_rejected() {
    // An attacker's browser tab from `http://evil.example.com` issues the
    // same POST sequence. CSRF blocks login at step 1 (Origin mismatch
    // against Host=127.0.0.1:PORT). No cookie is ever planted, so steps
    // 2-5 all 403 on require_sid independently.
    //
    // We assert step 1 fails with 403 — that's the clearest signal: the
    // attack was stopped before any session was issued. The later steps
    // are redundant but harmless; asserting specifically on step 5 (add)
    // would still be 403, but via require_sid rather than csrf_guard,
    // which muddies the intent of this test.
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = arr_client("Mozilla/5.0");
    let statuses = arr_handshake_sequence(
        &base,
        &client,
        &[("Origin", "http://evil.example.com")],
        TEST_MAGNET,
    )
    .await;
    assert_eq!(
        statuses[0].as_u16(),
        403,
        "step 1 (login) must 403 under cross-origin CSRF — got {statuses:?}"
    );
    assert_eq!(
        statuses[4].as_u16(),
        403,
        "step 5 (add) must 403 — either CSRF or require_sid, no cookie present"
    );
}

#[tokio::test]
async fn simulated_lidarr_test_connection_succeeds() {
    let (base, _handle, _session) = tcp_server(|_| {}).await;
    let client = arr_client("Lidarr/2.14.0");
    let statuses = arr_handshake_sequence(&base, &client, &[], TEST_MAGNET).await;
    for (i, s) in statuses.iter().enumerate() {
        assert!(
            s.is_success(),
            "step {} failed with status {s}: {statuses:?}",
            i + 1
        );
    }
}
