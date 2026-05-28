//! Integration tests for the M232 full Preferences page.
//!
//! Covers:
//!   - `GET  /webui/preferences`           — full 8-tab document render
//!   - `POST /webui/preferences/save`      — settings round-trip + classification
//!   - `GET  /webui/settings`              — legacy redirect → `/webui/preferences`
//!
//! Test sessions isolate the resume + category + tag registries to a per-test
//! tempdir so parallel runs do not collide on the user's `$XDG_CONFIG_HOME`
//! (see MEMORY.md → `feedback_api_registry_test_isolation.md`).

use std::fmt::Write as _;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

use irontide::session::Settings;
use irontide_api::routes::build_router;

/// Helper for the urlencoded form encoder — checkbox fields only appear
/// in the body when `true` (HTML checkbox semantics, matched by the
/// `Option<String>` + `#[serde(default)]` server-side declaration).
fn push_bool(parts: &mut Vec<String>, name: &str, v: bool) {
    if v {
        parts.push(format!("{name}=on"));
    }
}

/// Build a router backed by a session with isolated resume + category + tag
/// registries. The [`TempDir`] must be held for the lifetime of the test.
async fn test_router_isolated() -> (axum::Router, TempDir) {
    let dir = tempfile::tempdir().expect("create tempdir");
    let settings = Settings {
        listen_port: 0,
        download_dir: dir.path().join("downloads"),
        enable_dht: false,
        enable_lsd: false,
        enable_upnp: false,
        enable_natpmp: false,
        resume_data_dir: Some(dir.path().join("resume")),
        save_resume_interval_secs: 0,
        category_registry_path: Some(dir.path().join("categories.toml")),
        tag_registry_path: Some(dir.path().join("tags.toml")),
        ..Settings::default()
    };

    let session = irontide::ClientBuilder::from_settings(settings)
        .start()
        .await
        .expect("start test session");
    (build_router(session), dir)
}

/// Mutable form fixture mirroring every field on `PreferencesForm`. Each test
/// constructs the default via `FormBody::default_for(tempdir)` and mutates the
/// fields it cares about, then `.encode()` produces the urlencoded body.
struct FormBody {
    // Behaviour
    notify_on_complete: bool,
    notify_on_error: bool,
    default_add_paused: bool,
    // Downloads
    download_dir: String,
    use_incomplete_dir: bool,
    incomplete_dir: String,
    create_subfolder: bool,
    preallocate_mode: String,
    watched_folder: String,
    delete_torrent_after_add: bool,
    move_completed_enabled: bool,
    move_completed_to: String,
    // Connection
    listen_port: u16,
    randomize_port_on_startup: bool,
    enable_upnp: bool,
    enable_natpmp: bool,
    max_connections_global: i32,
    max_peers_per_torrent: usize,
    active_downloads: i32,
    active_seeds: i32,
    network_interface: String,
    // Speed
    download_rate_limit: u64,
    upload_rate_limit: u64,
    alt_download_rate_limit: u64,
    alt_upload_rate_limit: u64,
    alt_speed_enabled: bool,
    rate_limit_includes_overhead: bool,
    rate_limit_utp: bool,
    rate_limit_lan: bool,
    // BitTorrent
    enable_dht: bool,
    enable_pex: bool,
    enable_lsd: bool,
    encryption_mode: String,
    anonymous_mode: bool,
    queueing_enabled: bool,
    // Advanced
    hashing_threads: usize,
    save_resume_interval_secs: u64,
    enable_utp: bool,
    enable_fast_extension: bool,
    enable_holepunch: bool,
    enable_bep40_eviction: bool,
}

impl FormBody {
    /// Build a form body matching `Settings::default()` post-test-isolation.
    /// All checkboxes default off; toggling a `bool` field to `true` makes
    /// the encoded body emit the `name=on` pair (HTML checkbox semantics).
    fn default_for(download_dir: &str) -> Self {
        let defaults = Settings::default();
        Self {
            notify_on_complete: defaults.notify_on_complete,
            notify_on_error: defaults.notify_on_error,
            default_add_paused: defaults.default_add_paused,
            download_dir: download_dir.to_string(),
            use_incomplete_dir: defaults.use_incomplete_dir,
            incomplete_dir: String::new(),
            create_subfolder: defaults.create_subfolder,
            preallocate_mode: "sparse".to_string(),
            watched_folder: String::new(),
            delete_torrent_after_add: defaults.delete_torrent_after_add,
            move_completed_enabled: defaults.move_completed_enabled,
            move_completed_to: String::new(),
            listen_port: 0,
            randomize_port_on_startup: defaults.randomize_port_on_startup,
            enable_upnp: false,
            enable_natpmp: false,
            max_connections_global: defaults.max_connections_global,
            max_peers_per_torrent: defaults.max_peers_per_torrent,
            active_downloads: defaults.active_downloads,
            active_seeds: defaults.active_seeds,
            network_interface: String::new(),
            download_rate_limit: defaults.download_rate_limit,
            upload_rate_limit: defaults.upload_rate_limit,
            alt_download_rate_limit: defaults.alt_download_rate_limit,
            alt_upload_rate_limit: defaults.alt_upload_rate_limit,
            alt_speed_enabled: defaults.alt_speed_enabled,
            rate_limit_includes_overhead: defaults.rate_limit_includes_overhead,
            rate_limit_utp: defaults.rate_limit_utp,
            rate_limit_lan: defaults.rate_limit_lan,
            enable_dht: false,
            enable_pex: defaults.enable_pex,
            enable_lsd: false,
            encryption_mode: "disabled".to_string(),
            anonymous_mode: defaults.anonymous_mode,
            queueing_enabled: defaults.queueing_enabled,
            hashing_threads: defaults.hashing_threads,
            save_resume_interval_secs: 0,
            enable_utp: defaults.enable_utp,
            enable_fast_extension: defaults.enable_fast_extension,
            enable_holepunch: defaults.enable_holepunch,
            enable_bep40_eviction: defaults.enable_bep40_eviction,
        }
    }

    fn encode(&self) -> String {
        // URL-encode in the same way `serde_urlencoded` does on the server
        // side: each `Option<String>` checkbox is only present when on.
        // Manual percent-encoder (the `url` crate is feature-gated behind
        // `webui` and not in dev-dependencies).
        let enc = |s: &str| {
            let mut out = String::with_capacity(s.len());
            for &b in s.as_bytes() {
                match b {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        out.push(b as char);
                    }
                    b' ' => out.push('+'),
                    _ => {
                        let _ = write!(out, "%{b:02X}");
                    }
                }
            }
            out
        };
        let mut parts: Vec<String> = Vec::new();
        push_bool(&mut parts, "notify_on_complete", self.notify_on_complete);
        push_bool(&mut parts, "notify_on_error", self.notify_on_error);
        push_bool(&mut parts, "default_add_paused", self.default_add_paused);
        parts.push(format!("download_dir={}", enc(&self.download_dir)));
        push_bool(&mut parts, "use_incomplete_dir", self.use_incomplete_dir);
        parts.push(format!("incomplete_dir={}", enc(&self.incomplete_dir)));
        push_bool(&mut parts, "create_subfolder", self.create_subfolder);
        parts.push(format!("preallocate_mode={}", enc(&self.preallocate_mode)));
        parts.push(format!("watched_folder={}", enc(&self.watched_folder)));
        push_bool(&mut parts, "delete_torrent_after_add", self.delete_torrent_after_add);
        push_bool(&mut parts, "move_completed_enabled", self.move_completed_enabled);
        parts.push(format!("move_completed_to={}", enc(&self.move_completed_to)));
        parts.push(format!("listen_port={}", self.listen_port));
        push_bool(&mut parts, "randomize_port_on_startup", self.randomize_port_on_startup);
        push_bool(&mut parts, "enable_upnp", self.enable_upnp);
        push_bool(&mut parts, "enable_natpmp", self.enable_natpmp);
        parts.push(format!("max_connections_global={}", self.max_connections_global));
        parts.push(format!("max_peers_per_torrent={}", self.max_peers_per_torrent));
        parts.push(format!("active_downloads={}", self.active_downloads));
        parts.push(format!("active_seeds={}", self.active_seeds));
        parts.push(format!("network_interface={}", enc(&self.network_interface)));
        parts.push(format!("download_rate_limit={}", self.download_rate_limit));
        parts.push(format!("upload_rate_limit={}", self.upload_rate_limit));
        parts.push(format!("alt_download_rate_limit={}", self.alt_download_rate_limit));
        parts.push(format!("alt_upload_rate_limit={}", self.alt_upload_rate_limit));
        push_bool(&mut parts, "alt_speed_enabled", self.alt_speed_enabled);
        push_bool(&mut parts, "rate_limit_includes_overhead", self.rate_limit_includes_overhead);
        push_bool(&mut parts, "rate_limit_utp", self.rate_limit_utp);
        push_bool(&mut parts, "rate_limit_lan", self.rate_limit_lan);
        push_bool(&mut parts, "enable_dht", self.enable_dht);
        push_bool(&mut parts, "enable_pex", self.enable_pex);
        push_bool(&mut parts, "enable_lsd", self.enable_lsd);
        parts.push(format!("encryption_mode={}", enc(&self.encryption_mode)));
        push_bool(&mut parts, "anonymous_mode", self.anonymous_mode);
        push_bool(&mut parts, "queueing_enabled", self.queueing_enabled);
        parts.push(format!("hashing_threads={}", self.hashing_threads));
        parts.push(format!("save_resume_interval_secs={}", self.save_resume_interval_secs));
        push_bool(&mut parts, "enable_utp", self.enable_utp);
        push_bool(&mut parts, "enable_fast_extension", self.enable_fast_extension);
        push_bool(&mut parts, "enable_holepunch", self.enable_holepunch);
        push_bool(&mut parts, "enable_bep40_eviction", self.enable_bep40_eviction);
        parts.join("&")
    }
}

async fn fetch_preferences(router: &axum::Router) -> String {
    let req = Request::get("/webui/preferences")
        .body(Body::empty())
        .expect("build prefs request");
    let response = router.clone().oneshot(req).await.expect("prefs");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    String::from_utf8_lossy(&body).to_string()
}

async fn post_preferences(router: &axum::Router, body: String) -> axum::response::Response {
    let req = Request::builder()
        .method("POST")
        .uri("/webui/preferences/save")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .expect("build save request");
    router.clone().oneshot(req).await.expect("save")
}

// ---------------------------------------------------------------------------
// AC1: GET renders all 8 tabs
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_full_get_renders_8_tabs() {
    let (router, _tempdir) = test_router_isolated().await;
    let text = fetch_preferences(&router).await;

    // The 8 tab panel ids, in order, must all be present.
    for tab in [
        "tab-behaviour",
        "tab-downloads",
        "tab-connection",
        "tab-speed",
        "tab-bittorrent",
        "tab-webui",
        "tab-advanced",
        "tab-about",
    ] {
        assert!(
            text.contains(&format!("id=\"{tab}\"")),
            "preferences page must render tab {tab}, got: {}",
            &text[..text.len().min(400)]
        );
    }
    assert!(
        text.contains("role=\"tablist\""),
        "preferences must include ARIA tablist landmark"
    );
    assert!(
        text.contains("hx-post=\"/webui/preferences/save\""),
        "form must POST to /webui/preferences/save"
    );
}

// ---------------------------------------------------------------------------
// AC2: POST writes settings and round-trips through GET
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_full_post_writes_settings() {
    let (router, tempdir) = test_router_isolated().await;
    let mut form = FormBody::default_for(&tempdir.path().join("downloads").to_string_lossy());
    form.max_peers_per_torrent = 73;
    form.active_downloads = 9;

    let response = post_preferences(&router, form.encode()).await;
    assert_eq!(response.status(), StatusCode::OK);

    // HX-Trigger payload should be nested-form JSON so HTMX 2.x fires a
    // single `settingsSaved` event whose detail carries restartPending.
    // See project_irontide_htmx2_flat_hx_vals: a flat
    // `{"settingsSaved": true, "restartPending": [...]}` payload would
    // fire two distinct events and leave ev.detail.restartPending
    // undefined on the listener.
    let trigger = response
        .headers()
        .get("HX-Trigger")
        .and_then(|v| v.to_str().ok())
        .expect("HX-Trigger present");
    assert!(
        trigger.contains("\"settingsSaved\":{"),
        "HX-Trigger settingsSaved must be an object (not a bool) so its detail carries restartPending, got {trigger}"
    );
    assert!(
        trigger.contains("\"restartPending\":["),
        "HX-Trigger must nest restartPending under settingsSaved, got {trigger}"
    );

    // GET should now reflect the new values.
    let text = fetch_preferences(&router).await;
    assert!(
        text.contains("value=\"73\""),
        "GET should reflect max_peers_per_torrent=73, got: {}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.contains("value=\"9\""),
        "GET should reflect active_downloads=9"
    );
}

// ---------------------------------------------------------------------------
// AC3: invalid select value returns 422 fragment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_full_post_validate_failure_returns_422_fragment() {
    let (router, tempdir) = test_router_isolated().await;
    let mut form = FormBody::default_for(&tempdir.path().join("downloads").to_string_lossy());
    form.encryption_mode = "garbage_value".to_string();

    let response = post_preferences(&router, form.encode()).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        response.headers().get("HX-Trigger").is_none(),
        "422 fragment must not emit HX-Trigger"
    );
}

// ---------------------------------------------------------------------------
// AC4: changing a restart-required field surfaces in HX-Trigger payload
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_full_post_restart_pending_for_encryption_change() {
    let (router, tempdir) = test_router_isolated().await;
    let mut form = FormBody::default_for(&tempdir.path().join("downloads").to_string_lossy());
    // Default is "disabled" — switching to "enabled" flips encryption_mode
    // which classify_restart_required reports as "encryption".
    form.encryption_mode = "enabled".to_string();

    let response = post_preferences(&router, form.encode()).await;
    assert_eq!(response.status(), StatusCode::OK);

    let trigger = response
        .headers()
        .get("HX-Trigger")
        .and_then(|v| v.to_str().ok())
        .expect("HX-Trigger present");
    assert!(
        trigger.contains("\"encryption\""),
        "restart_required must include `encryption`, got {trigger}"
    );
}

// ---------------------------------------------------------------------------
// AC5: a no-op save produces an empty restartPending list and no banner
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_full_post_restart_pending_empty_no_banner() {
    let (router, tempdir) = test_router_isolated().await;
    let form = FormBody::default_for(&tempdir.path().join("downloads").to_string_lossy());

    let response = post_preferences(&router, form.encode()).await;
    assert_eq!(response.status(), StatusCode::OK);

    let trigger = response
        .headers()
        .get("HX-Trigger")
        .and_then(|v| v.to_str().ok())
        .expect("HX-Trigger present")
        .to_string();
    assert!(
        trigger.contains("\"restartPending\":[]"),
        "no-op save must produce empty restartPending, got {trigger}"
    );

    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);
    assert!(
        !text.contains("Restart pending"),
        "empty restart_required must not render the banner card"
    );
}

// ---------------------------------------------------------------------------
// AC6: empty path string clears the engine field to its default (None)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_full_post_empty_path_clears_to_engine_default() {
    let (router, tempdir) = test_router_isolated().await;
    let dir = tempdir.path().join("downloads").to_string_lossy().to_string();

    // First, set a watched_folder.
    let mut form = FormBody::default_for(&dir);
    form.watched_folder = tempdir.path().join("watch").to_string_lossy().to_string();
    let response = post_preferences(&router, form.encode()).await;
    assert_eq!(response.status(), StatusCode::OK);

    // Then clear it via empty string.
    let mut form = FormBody::default_for(&dir);
    form.watched_folder = String::new();
    let response = post_preferences(&router, form.encode()).await;
    assert_eq!(response.status(), StatusCode::OK);

    // GET should now render watched_folder with an empty value attribute.
    let text = fetch_preferences(&router).await;
    let watched_line = text
        .lines()
        .find(|l| l.contains("name=\"watched_folder\""))
        .expect("watched_folder input present");
    assert!(
        watched_line.contains("value=\"\"") || !watched_line.contains("value=\"/"),
        "watched_folder should be empty after clear, got: {watched_line}"
    );
}

// ---------------------------------------------------------------------------
// AC7: legacy `/webui/settings` redirects to `/webui/preferences`
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_old_settings_endpoint_redirects_to_preferences() {
    let (router, _tempdir) = test_router_isolated().await;

    let req = Request::get("/webui/settings")
        .body(Body::empty())
        .expect("build legacy settings request");
    let response = router.clone().oneshot(req).await.expect("legacy settings");
    assert_eq!(response.status(), StatusCode::FOUND);
    let location = response
        .headers()
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .expect("Location header present");
    assert_eq!(location, "/webui/preferences");
}

// ---------------------------------------------------------------------------
// AC8: About tab renders version + license + Codeberg + daemon endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_full_get_renders_about_tab() {
    let (router, _tempdir) = test_router_isolated().await;
    let text = fetch_preferences(&router).await;

    assert!(
        text.contains("id=\"tab-about\""),
        "About tab section must be present"
    );
    assert!(
        text.contains(env!("CARGO_PKG_VERSION")),
        "About tab must render the running daemon version ({})",
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        text.contains("GPL-3.0-or-later"),
        "About tab must declare the GPL-3.0-or-later license"
    );
    assert!(
        text.contains("codeberg.org/alan09086/irontide"),
        "About tab must link the Codeberg primary remote"
    );
    assert!(
        text.contains("github.com/alan09086/irontide"),
        "About tab must link the GitHub mirror"
    );
    assert!(
        text.contains("(this page is served by the daemon)"),
        "daemon endpoint fallback must render when context value is empty"
    );
}

// ---------------------------------------------------------------------------
// AC9: user-controlled path strings are Askama-escaped in the response
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_full_post_xss_escapes_user_provided_paths() {
    let (router, tempdir) = test_router_isolated().await;
    let mut form = FormBody::default_for(&tempdir.path().join("downloads").to_string_lossy());
    // Inject HTML-special characters into a path-bearing field.
    form.watched_folder = "/tmp/<script>alert(1)</script>".to_string();

    let response = post_preferences(&router, form.encode()).await;
    let status = response.status();

    let body = response
        .into_body()
        .collect()
        .await
        .expect("collect body")
        .to_bytes();
    let text = String::from_utf8_lossy(&body);
    let watched_line = text
        .lines()
        .find(|l| l.contains("watched_folder"))
        .unwrap_or("<no watched_folder line found>");
    assert_eq!(
        status,
        StatusCode::OK,
        "save should accept /tmp path with HTML chars (validated absolute, not blacklisted); watched_folder line: {watched_line}"
    );
    assert!(
        !text.contains("<script>alert(1)</script>"),
        "Askama must escape user-provided paths; raw <script> tag leaked. watched_folder line: {watched_line}"
    );
    // Askama's html escaper turns `<` → `&lt;` and `>` → `&gt;`; `/` is left
    // alone (per OWASP — slashes carry no meaning inside an attribute or
    // text node). Assert the dangerous tag delimiters are entity-encoded.
    // Askama 0.15 emits numeric character references (`&#60;`, `&#62;`)
    // rather than named entities. Both are valid HTML; assert against the
    // numeric form that Askama actually produces.
    assert!(
        text.contains("&#60;script&#62;") || text.contains("&lt;script&gt;"),
        "user-provided path must HTML-entity-escape `<script>`. watched_folder line: {watched_line}"
    );
    assert!(
        text.contains("&#60;/script&#62;") || text.contains("&lt;/script&gt;"),
        "user-provided path must HTML-entity-escape `</script>`. watched_folder line: {watched_line}"
    );
}

// ---------------------------------------------------------------------------
// AC10 (smoke): each tab is reachable via a `#tab-X` anchor hash
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preferences_form_advertises_tab_hash_routing() {
    let (router, _tempdir) = test_router_isolated().await;
    let text = fetch_preferences(&router).await;

    for slug in [
        "behaviour", "downloads", "connection", "speed", "bittorrent", "webui", "advanced",
        "about",
    ] {
        let href = format!("href=\"#tab-{slug}\"");
        assert!(
            text.contains(&href),
            "tab nav must include hash anchor {href}, got: {}",
            &text[..text.len().min(600)]
        );
    }
}
