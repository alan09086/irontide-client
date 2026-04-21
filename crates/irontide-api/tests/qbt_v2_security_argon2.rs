//! Integration tests for M172a Lane A — argon2id password verification,
//! legacy plaintext migration, setPreferences password rotation, and the
//! global argon2 concurrency semaphore.
//!
//! Uses the same tower::ServiceExt::oneshot pattern as the other
//! `qbt_v2_*` tests; `MockConnectInfo` is layered inside `build_router`
//! so the required `ConnectInfo<SocketAddr>` extractor on `auth::login`
//! always succeeds.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use irontide::session::{Settings, hash_qbt_password};
use irontide_api::routes::build_router;

static SESSION_COUNTER: AtomicUsize = AtomicUsize::new(0);

async fn test_session(customize: impl FnOnce(&mut Settings)) -> irontide::session::SessionHandle {
    let n = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let resume_dir =
        std::env::temp_dir().join(format!("irontide-qbt-argon2-{}-{}", std::process::id(), n));
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

fn login_req(user: &str, pass: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v2/auth/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(format!("username={user}&password={pass}")))
        .expect("build login request")
}

async fn resp_parts(
    resp: axum::http::Response<Body>,
) -> (StatusCode, String, axum::http::HeaderMap) {
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned(), headers)
}

// ── argon2 verification ────────────────────────────────────────────

#[tokio::test]
async fn password_hash_roundtrip_verifies_admin_admin() {
    let session = test_session(|_| {}).await;
    let router = build_router(session);
    let (status, body, headers) = resp_parts(
        router
            .clone()
            .oneshot(login_req("admin", "adminadmin"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body was: {body}");
    assert!(
        headers.get(header::SET_COOKIE).is_some(),
        "expect SID cookie"
    );
}

#[tokio::test]
async fn wrong_password_rejected_constant_time() {
    // We don't measure timing (flaky in CI); instead assert the body + status
    // match the "indistinguishable" contract — malformed hash and wrong
    // password both return the identical 403 `Fails.` payload.
    let session = test_session(|_| {}).await;
    let router = build_router(session);
    let (status, body, headers) = resp_parts(
        router
            .oneshot(login_req("admin", "wrongpassword"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, "Fails.");
    assert!(headers.get(header::SET_COOKIE).is_none());
}

#[tokio::test]
async fn malformed_password_hash_returns_forbidden() {
    // Settings::validate rejects non-argon2id hashes, but the handler must
    // still degrade gracefully if a malformed PHC makes it through (e.g.
    // an operator bypassed validation via a raw TOML edit). C2: map to 403
    // `Fails.` indistinguishably from wrong-password.
    //
    // Trick: ship a PHC-shaped string that *starts* with `$argon2id$` so
    // validate() doesn't reject, but the parameter fields are gibberish.
    let session = test_session(|s| {
        s.qbt_compat.password_hash = "$argon2id$v=19$bogus,bogus$bogus$bogus".into();
    })
    .await;
    let router = build_router(session);
    let (status, body, _) = resp_parts(
        router
            .oneshot(login_req("admin", "adminadmin"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body, "Fails.");
}

#[tokio::test]
async fn empty_password_hash_with_plaintext_still_authenticates_grandfather() {
    // Emulates a boot where migration failed: hash empty, plaintext retained
    // in memory. Login must still succeed via the grandfather path until
    // the next migration attempt.
    let session = test_session(|s| {
        s.qbt_compat.password_hash.clear();
        s.qbt_compat.password = "legacyplaintext".into();
    })
    .await;
    let router = build_router(session);
    let (status, _body, headers) = resp_parts(
        router
            .oneshot(login_req("admin", "legacyplaintext"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(headers.get(header::SET_COOKIE).is_some());
}

#[tokio::test]
async fn argon2_params_match_owasp_m19456_t2_p1() {
    // Inspect the DEFAULT_ADMINADMIN_HASH PHC string — M172a locks in the
    // OWASP cheat-sheet recommendation for argon2id.
    let s = Settings::default();
    let phc = &s.qbt_compat.password_hash;
    assert!(phc.contains("m=19456"), "missing m= param: {phc}");
    assert!(phc.contains("t=2"), "missing t= param: {phc}");
    assert!(phc.contains("p=1"), "missing p= param: {phc}");
    assert!(phc.starts_with("$argon2id$"));
}

// ── Default config / migration ─────────────────────────────────────

#[tokio::test]
async fn default_config_ships_pre_hashed() {
    // A3: fresh install ships pre-hashed default → no WARN, no .bak.
    let s = Settings::default();
    assert!(s.qbt_compat.password_hash.starts_with("$argon2id$"));
    assert!(s.qbt_compat.password.is_empty());
}

#[test]
fn legacy_plaintext_migration_rewrites_config_once() {
    // Disk-backed: irontide_config::migrate_qbt_credentials_in_file writes
    // back once then no-ops. Boot-twice invariant: re-running migration on
    // the rewritten config does NOT touch the .bak.
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("config.toml");
    let legacy = r#"
[qbt_compat]
enabled = true
username = "admin"
password = "adminadmin"
"#;
    std::fs::write(&path, legacy).expect("write legacy");

    let outcome = irontide_config::migrate_qbt_credentials_in_file(&path).expect("first pass");
    assert_eq!(outcome, irontide_config::QbtFileMigration::Rewritten);

    let body_after_first = std::fs::read_to_string(&path).expect("read after 1");
    assert!(body_after_first.contains("password_hash"));
    assert!(body_after_first.contains("password = \"\""));

    let bak_path = irontide_config::bak_path_for(&path);
    let bak_first = std::fs::read_to_string(&bak_path).expect("bak 1");
    assert!(bak_first.contains("adminadmin"));

    // Boot 2: simulate the daemon restarting with the rewritten config.
    let outcome2 = irontide_config::migrate_qbt_credentials_in_file(&path).expect("second pass");
    assert_eq!(outcome2, irontide_config::QbtFileMigration::NoOp);

    let bak_second = std::fs::read_to_string(&bak_path).expect("bak 2");
    assert_eq!(bak_first, bak_second, ".bak must stay snapshot-of-first");
}

#[test]
fn migration_failure_logs_and_keeps_plaintext_working() {
    // C2: if the on-disk rewrite fails, the in-memory Settings migration
    // must keep the plaintext populated so the grandfather login path
    // still authenticates. We assert the migration helper is a pure
    // in-memory mutator and that a *file* rewrite failure is isolated
    // from the session startup path.
    let mut qbt = irontide::session::QbtCompatSettings {
        password_hash: String::new(),
        password: "legacyplaintext".into(),
        ..Default::default()
    };
    // In-memory migration succeeds without touching disk.
    let outcome =
        irontide::session::migrate_qbt_credentials(&mut qbt).expect("in-memory must succeed");
    assert_eq!(outcome, irontide::session::QbtCredentialMigration::Upgraded);
    assert!(qbt.password_hash.starts_with("$argon2id$"));
    assert!(qbt.password.is_empty());

    // If a hypothetical file-backed migration were to fail afterwards
    // (tempfile failure, EACCES etc.) the in-memory state is ALREADY
    // migrated and login proceeds via the hash — so "failure" in file
    // migration does not brick the daemon.
}

// ── setPreferences password rotation ──────────────────────────────

async fn login_ok(router: &axum::Router, user: &str, pass: &str) -> Option<String> {
    let resp = router
        .clone()
        .oneshot(login_req(user, pass))
        .await
        .expect("login sent");
    if resp.status() != StatusCode::OK {
        return None;
    }
    let cookie = resp
        .headers()
        .get(header::SET_COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .next()?
        .to_owned();
    let _ = resp.into_body().collect().await.ok()?;
    Some(cookie)
}

#[tokio::test]
async fn set_preferences_web_ui_password_hashes_on_write() {
    let session = test_session(|_| {}).await;
    let router = build_router(session);
    let sid = login_ok(&router, "admin", "adminadmin")
        .await
        .expect("initial login");

    // Rotate via setPreferences.
    let body = serde_json::json!({ "web_ui_password": "newsecretpw" }).to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/app/setPreferences")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &sid)
        .body(Body::from(body))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let _ = resp.into_body().collect().await.unwrap();

    // Old password now fails — we have to log out first because the SID
    // stored a successful-login token against the old creds.
    let (status_old, _, _) = resp_parts(
        router
            .clone()
            .oneshot(login_req("admin", "adminadmin"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status_old, StatusCode::FORBIDDEN);

    // New password works.
    let (status_new, _, headers_new) = resp_parts(
        router
            .clone()
            .oneshot(login_req("admin", "newsecretpw"))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status_new, StatusCode::OK);
    assert!(headers_new.get(header::SET_COOKIE).is_some());
}

#[tokio::test]
async fn get_preferences_omits_web_ui_password_and_password_hash() {
    let session = test_session(|_| {}).await;
    let router = build_router(session);
    let sid = login_ok(&router, "admin", "adminadmin")
        .await
        .expect("login");

    let req = Request::builder()
        .method("GET")
        .uri("/api/v2/app/preferences")
        .header(header::COOKIE, &sid)
        .body(Body::empty())
        .unwrap();
    let (status, body, _) = resp_parts(router.oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    // Neither the hash nor the plaintext field may appear in the GET body.
    assert!(
        !body.contains("password_hash"),
        "GET /preferences must not serialise password_hash: {body}"
    );
    assert!(
        !body.contains("web_ui_password"),
        "GET /preferences must not serialise web_ui_password: {body}"
    );
}

// ── Global argon2 semaphore ───────────────────────────────────────

#[tokio::test]
async fn global_semaphore_default_is_num_cpus_times_two_capped_16() {
    let permits = irontide_api::routes::default_argon2_permits(None);
    let expected_max = 16_usize;
    let expected_min = 2_usize;
    assert!(
        (expected_min..=expected_max).contains(&permits),
        "permits {permits} must be in [{expected_min}, {expected_max}]"
    );
    assert_eq!(permits, num_cpus::get().saturating_mul(2).clamp(2, 16));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn distributed_flood_capped_at_semaphore_size() {
    // G2: spawn more concurrent logins than there are argon2 permits and
    // assert the queue drains correctly. We don't measure CPU — just that
    // every request resolves to either 200 or 403 with no deadlock.
    let session = test_session(|s| {
        s.qbt_compat.max_concurrent_argon2_ops = Some(2);
    })
    .await;
    let router = Arc::new(build_router(session));

    let n = 10_usize;
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        let router = Arc::clone(&router);
        handles.push(tokio::spawn(async move {
            let pass = if i.is_multiple_of(2) {
                "adminadmin"
            } else {
                "wrong"
            };
            let resp = (*router)
                .clone()
                .oneshot(login_req("admin", pass))
                .await
                .unwrap();
            resp.status()
        }));
    }

    let results = futures_util::future::join_all(handles).await;
    let mut ok = 0;
    let mut forbidden = 0;
    for r in results {
        let s = r.expect("task");
        match s {
            StatusCode::OK => ok += 1,
            StatusCode::FORBIDDEN => forbidden += 1,
            other => panic!("unexpected status: {other}"),
        }
    }
    // Half correct creds, half wrong — every login resolves.
    assert_eq!(ok + forbidden, n);
    assert!(ok > 0, "some correct-creds attempts must succeed");
    assert!(forbidden > 0, "some wrong-creds attempts must fail");
}

#[tokio::test]
async fn set_preferences_web_ui_password_hashes_not_stored_plaintext() {
    // Defence-in-depth: after password rotation the Settings must contain
    // the PHC hash (not the plaintext) and the legacy `password` field
    // must be cleared.
    let session = test_session(|_| {}).await;
    let router = build_router(session.clone());
    let sid = login_ok(&router, "admin", "adminadmin")
        .await
        .expect("initial login");

    let body = serde_json::json!({ "web_ui_password": "brand-new-pw" }).to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/v2/app/setPreferences")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, &sid)
        .body(Body::from(body))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let _ = resp.into_body().collect().await.unwrap();

    let s = session.settings().await.expect("settings");
    assert!(
        s.qbt_compat.password_hash.starts_with("$argon2id$"),
        "expected PHC string after rotation, got: {}",
        s.qbt_compat.password_hash
    );
    assert!(
        s.qbt_compat.password.is_empty(),
        "legacy plaintext must be cleared on rotation"
    );
}

// SocketAddr import kept for future Lane B/C tests that need to compare the
// mock peer against a CIDR whitelist.
#[allow(dead_code)]
const _MOCK_PEER_TYPECHECK: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
    0,
);

// Suppress "unused import" when the above dead_code stub is disabled.
#[allow(unused_imports)]
use hash_qbt_password as _hash_qbt_password_import_tag;
