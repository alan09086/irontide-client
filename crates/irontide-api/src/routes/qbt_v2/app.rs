#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "M175: qBt wire format — preferences/transferInfo follow qBittorrent's signed-i64 encoding for unsigned counters"
)]

//! qBt v2 `app/*` endpoints (M168, M171).
//!
//! Implemented:
//! - `GET /api/v2/app/version` — plain-text app version string.
//! - `GET /api/v2/app/webapiVersion` — plain-text webapi version string.
//! - `GET /api/v2/app/buildInfo` — JSON `{qt, libtorrent, boost, openssl, bitness}`.
//! - `GET /api/v2/app/preferences` — JSON with *arr-required fields.
//! - `POST /api/v2/app/setPreferences` — writable-field allowlist patch
//!   (M171 D3). Accepts JSON body or qBt's legacy `json=<string>` URL-encoded
//!   form. Validates via [`Settings::validate`] and applies via
//!   [`apply_settings_classified`](irontide::session::SessionHandle::apply_settings_classified);
//!   restart-required fields are surfaced as the `X-IronTide-Restart-Pending`
//!   response header (M171 D3.5) — comma-joined list so clients can render a
//!   "restart to apply" UX affordance.

use axum::extract::State;
use axum::http::{HeaderName, HeaderValue};
use axum::response::{IntoResponse, Response};

use irontide::prelude::EncryptionMode;
use irontide::session::MaxRatioAction;
use serde::Deserialize;

use super::preferences::QbtPreferences;
use super::response::{QbtError, QbtResponse};
use super::state::QbtState;

pub async fn version(State(state): State<QbtState>) -> Result<QbtResponse, QbtError> {
    let settings = state
        .session
        .settings()
        .await
        .map_err(|e| QbtError::Internal(format!("read settings: {e}")))?;
    Ok(QbtResponse::PlainText(
        settings.qbt_compat.spoof_app_version.clone(),
    ))
}

pub async fn webapi_version(State(state): State<QbtState>) -> Result<QbtResponse, QbtError> {
    let settings = state
        .session
        .settings()
        .await
        .map_err(|e| QbtError::Internal(format!("read settings: {e}")))?;
    Ok(QbtResponse::PlainText(
        settings.qbt_compat.spoof_webapi_version.clone(),
    ))
}

/// `GET /api/v2/app/buildInfo` — pinned hardcoded values mirror a recent qBt
/// release. Bitness is derived from `usize` so 32-bit ARM / x86 report 32.
pub async fn build_info() -> QbtResponse {
    let bitness = (std::mem::size_of::<usize>() as u32) * 8;
    QbtResponse::Json(serde_json::json!({
        "qt": "6.5.3",
        "libtorrent": "2.0.9",
        "boost": "1.83.0",
        "openssl": "3.0.11",
        "bitness": bitness,
    }))
}

/// `GET /api/v2/app/preferences` — projects the live `Settings` onto the qBt
/// preferences DTO shape that `*arr` clients expect.
pub async fn preferences(State(state): State<QbtState>) -> Result<QbtResponse, QbtError> {
    let settings = state
        .session
        .settings()
        .await
        .map_err(|e| QbtError::Internal(format!("read settings: {e}")))?;
    let prefs = QbtPreferences::from(&settings);
    Ok(QbtResponse::Json(serde_json::to_value(&prefs).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

/// M171 D3: Writable-field allowlist for `POST /api/v2/app/setPreferences`.
///
/// Every field is `Option<T>` so partial updates don't zero out untouched
/// settings. Unknown fields are silently ignored by serde (no `deny_unknown`)
/// — matches qBt behaviour where *arr frequently sends extra keys.
///
/// Wire quirks mirrored exactly:
/// * `dl_limit` / `up_limit` — negative values mean "unlimited" and map to
///   `0` in our model (which is our own unlimited sentinel).
/// * `max_ratio` — negative means "no limit" (maps to `None`); `NaN` is
///   rejected as 400 Bad Request.
/// * `max_ratio_enabled = false` wipes `seed_ratio_limit` to `None` even if
///   `max_ratio` is sent as a positive number in the same patch.
/// * `max_seeding_time` / `max_inactive_seeding_time` are in MINUTES on the
///   wire — our storage is seconds, so multiply by 60.
/// * `encryption` is an integer 0/1/2 — 0=Prefer, 1=Force, 2=Disable.
#[derive(Debug, Default, Deserialize)]
struct QbtPreferencesPatch {
    #[serde(default)]
    max_connec: Option<u32>,
    #[serde(default)]
    max_connec_per_torrent: Option<u32>,
    #[serde(default)]
    max_uploads: Option<u32>,
    #[serde(default)]
    #[allow(dead_code)] // reserved — qBt exposes per-torrent upload slot caps;
    // IronTide has no Settings-wide field for this yet.
    max_uploads_per_torrent: Option<u32>,
    #[serde(default)]
    dl_limit: Option<i64>,
    #[serde(default)]
    up_limit: Option<i64>,
    #[serde(default)]
    dht: Option<bool>,
    #[serde(default)]
    lsd: Option<bool>,
    #[serde(default)]
    pex: Option<bool>,
    #[serde(default)]
    encryption: Option<i32>,
    #[serde(default)]
    anonymous_mode: Option<bool>,
    #[serde(default)]
    queueing_enabled: Option<bool>,
    #[serde(default)]
    max_active_downloads: Option<i32>,
    #[serde(default)]
    max_active_torrents: Option<i32>,
    #[serde(default)]
    max_active_uploads: Option<i32>,
    #[serde(default)]
    save_path: Option<String>,
    #[serde(default)]
    max_ratio: Option<f64>,
    #[serde(default)]
    max_ratio_enabled: Option<bool>,
    #[serde(default)]
    max_ratio_act: Option<String>,
    #[serde(default)]
    create_subfolder_enabled: Option<bool>,
    #[serde(default)]
    auto_tmm_enabled: Option<bool>,
    #[serde(default)]
    listen_port: Option<u16>,
    #[serde(default)]
    max_seeding_time: Option<i64>,
    #[serde(default)]
    max_seeding_time_enabled: Option<bool>,
    #[serde(default)]
    max_inactive_seeding_time: Option<i64>,
    #[serde(default)]
    max_inactive_seeding_time_enabled: Option<bool>,
    /// M172a: qBt's wire field for rotating the Web UI password. Input-only
    /// — the handler hashes this immediately via
    /// [`irontide::session::hash_qbt_password`] and writes into
    /// `settings.qbt_compat.password_hash`. Never echoed back on the GET
    /// side (see `QbtPreferences`, which has no password/hash field).
    #[serde(default)]
    web_ui_password: Option<String>,

    // M172a Lane B: CSRF + reverse-proxy wire fields. qBt uses semicolon-
    // joined CIDRs on the wire for the proxies list.
    #[serde(default)]
    web_ui_csrf_protection_enabled: Option<bool>,
    #[serde(default)]
    web_ui_host_header_validation_enabled: Option<bool>,
    #[serde(default)]
    web_ui_reverse_proxy_enabled: Option<bool>,
    #[serde(default)]
    web_ui_reverse_proxies_list: Option<String>,

    // ── M172a Lane C: brute-force ban ─────────────────────────────────
    /// qBt wire field for [`QbtCompatSettings::max_failed_auth_count`].
    /// Forwarded verbatim, no unit conversion.
    #[serde(default)]
    web_ui_max_auth_fail_count: Option<u32>,
    /// qBt wire field for [`QbtCompatSettings::ban_duration_secs`].
    /// Wire semantics: `-1` = "leave unchanged" sentinel (qBt uses this
    /// to distinguish "not set in UI" from "set to zero"); values `< -1`
    /// are ignored; positive values are forwarded verbatim after a
    /// `try_into::<u64>` cast.
    #[serde(default)]
    web_ui_ban_duration: Option<i64>,
    /// qBt wire field for [`QbtCompatSettings::bypass_local_auth`].
    #[serde(default)]
    bypass_local_auth: Option<bool>,
    /// qBt wire field: `true` enables the subnet whitelist, `false`
    /// clears it. Applied BEFORE [`Self::bypass_auth_subnet_whitelist`]
    /// so the list survives a disable-then-enable round trip within a
    /// single patch.
    #[serde(default)]
    bypass_auth_subnet_whitelist_enabled: Option<bool>,
    /// qBt wire field: newline-separated CIDR strings. Replaces the
    /// entire list on each patch (matches qBt UX — the textarea is the
    /// authoritative source). Empty string clears the list.
    #[serde(default)]
    bypass_auth_subnet_whitelist: Option<String>,
}

/// `POST /api/v2/app/setPreferences` (M171 D3 + D3.5).
///
/// qBt's `WebUI` v2 historically POSTs this as
/// `application/x-www-form-urlencoded` with a single `json=<stringified
/// JSON>` field, but recent `*arr` versions just POST an
/// `application/json` body. The handler accepts either.
///
/// When any field in the patch requires a session restart to take effect
/// (`listen_port`, dht, lsd, pex, encryption, `anonymous_mode`, `save_path`) the
/// response carries an `X-IronTide-Restart-Pending: <comma-joined-fields>`
/// header. Immediate fields (rate limiters, peer cap, queueing, ratio
/// action, `create_subfolder`, `auto_tmm`, `max_ratio`) produce no header.
pub async fn set_preferences(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<Response, QbtError> {
    let bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .map_err(|e| QbtError::BadRequest(format!("read body: {e}")))?;

    let patch = parse_preferences_patch(&bytes)?;

    let mut settings = state
        .session
        .settings()
        .await
        .map_err(|e| QbtError::Internal(format!("read settings: {e}")))?;

    // M172a Lane B: snapshot the list BEFORE the patch so we can detect a
    // mutation after apply — the RwLock cache must be refreshed atomically
    // when the CIDRs change so new requests see the new trust set.
    let prev_proxies = settings.qbt_compat.web_ui_reverse_proxies_list.clone();

    apply_preferences_patch(&mut settings, patch)?;

    settings
        .validate()
        .map_err(|e| QbtError::BadRequest(format!("invalid settings: {e}")))?;

    let proxies_changed = settings.qbt_compat.web_ui_reverse_proxies_list != prev_proxies;
    let new_proxies_raw = settings.qbt_compat.web_ui_reverse_proxies_list.clone();

    // M172a Lane C: sync the shared bypass-whitelist RwLock BEFORE the
    // engine applies the settings. Failing validation above aborts early
    // so we only write the lock on a patch that's about to land; the
    // write happens before `apply_settings_classified` so that on a rare
    // timeout between validate and apply, any login that races the
    // settings apply consults the new whitelist. Settings::validate
    // rejected any malformed CIDR, so unwrap_or retains unchanged
    // entries on the impossible "was fine a moment ago" branch.
    {
        let parsed: Vec<ipnet::IpNet> = settings
            .qbt_compat
            .bypass_auth_subnet_whitelist
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();
        *state.bypass_auth_subnet_whitelist.write() = parsed;
    }

    let applied = state
        .session
        .apply_settings_classified(settings)
        .await
        .map_err(|e| match e {
            // M173 Lane B (B11): concurrent setPreferences hit the
            // in-flight guard. qBt clients can retry shortly.
            irontide::session::Error::ConcurrentReconfig => QbtError::Conflict(e.to_string()),
            _ => QbtError::Internal(format!("apply settings: {e}")),
        })?;

    // A7: swap the RwLock under an exclusive write — never during a read.
    // Already-in-flight CSRF checks reading the old list complete with the
    // pre-swap data; subsequent requests see the new CIDRs. `validate()`
    // has already rejected any malformed entry so this parse is infallible
    // in practice; `filter_map` silently drops anything that somehow slipped
    // through.
    if proxies_changed {
        let parsed: Vec<ipnet::IpNet> = new_proxies_raw
            .iter()
            .filter_map(|s| s.parse::<ipnet::IpNet>().ok())
            .collect();
        *state.reverse_proxies_list.write() = parsed;
    }

    let mut response = QbtResponse::ok().into_response();
    if !applied.restart_required.is_empty() {
        // All field names are `&'static str` ASCII identifiers, so the
        // joined value is always a valid ASCII HeaderValue.
        let value = applied.restart_required.join(",");
        let header_value = HeaderValue::try_from(value)
            .expect("restart_required field names are ASCII identifiers");
        response.headers_mut().insert(
            HeaderName::from_static("x-irontide-restart-pending"),
            header_value,
        );
    }
    Ok(response)
}

/// Detect JSON body first, fall back to qBt's legacy `json=<...>` URL-encoded
/// form. Errors as 400 Bad Request in either case with a descriptive prefix.
fn parse_preferences_patch(bytes: &[u8]) -> Result<QbtPreferencesPatch, QbtError> {
    if bytes.is_empty() {
        return Ok(QbtPreferencesPatch::default());
    }

    if let Ok(patch) = serde_json::from_slice::<QbtPreferencesPatch>(bytes) {
        return Ok(patch);
    }

    #[derive(Deserialize)]
    struct JsonForm {
        json: String,
    }
    let form: JsonForm = serde_urlencoded::from_bytes(bytes)
        .map_err(|e| QbtError::BadRequest(format!("parse form: {e}")))?;
    serde_json::from_str(&form.json).map_err(|e| QbtError::BadRequest(format!("parse json: {e}")))
}

/// Apply the allowlist patch onto `settings` in place.
///
/// # Semantics
///
/// * `max_connec` and `max_connec_per_torrent` both map onto
///   `max_peers_per_torrent` (`IronTide` has no session-wide cap distinct from
///   per-torrent). `max_connec` is applied last so that it wins when both are
///   sent — matches qBt's UI order where the global cap is authoritative.
/// * Negative `dl_limit` / `up_limit` means "unlimited" in qBt and maps to
///   `0` (our unlimited sentinel).
/// * `max_ratio_enabled=false` clears `seed_ratio_limit` irrespective of any
///   `max_ratio` sent alongside — qBt treats the enabled flag as authoritative.
/// * `max_seeding_time_enabled=false` clears `seed_time_limit_secs`. Same for
///   the inactive variant. Wire values are in MINUTES — we multiply by 60.
fn apply_preferences_patch(
    settings: &mut irontide::session::Settings,
    patch: QbtPreferencesPatch,
) -> Result<(), QbtError> {
    // Order matters for max_connec vs max_connec_per_torrent (last write wins).
    if let Some(v) = patch.max_connec_per_torrent {
        settings.max_peers_per_torrent = v as usize;
    }
    if let Some(v) = patch.max_connec {
        settings.max_peers_per_torrent = v as usize;
    }
    if let Some(v) = patch.max_uploads {
        settings.auto_upload_slots_max = v as usize;
    }
    if let Some(v) = patch.dl_limit {
        settings.download_rate_limit = if v < 0 { 0 } else { v as u64 };
    }
    if let Some(v) = patch.up_limit {
        settings.upload_rate_limit = if v < 0 { 0 } else { v as u64 };
    }
    if let Some(v) = patch.dht {
        settings.enable_dht = v;
    }
    if let Some(v) = patch.lsd {
        settings.enable_lsd = v;
    }
    if let Some(v) = patch.pex {
        settings.enable_pex = v;
    }
    if let Some(v) = patch.encryption {
        settings.encryption_mode = match v {
            0 => EncryptionMode::Enabled,
            1 => EncryptionMode::Forced,
            2 => EncryptionMode::Disabled,
            _ => {
                return Err(QbtError::BadRequest(format!("invalid encryption: {v}")));
            }
        };
    }
    if let Some(v) = patch.anonymous_mode {
        settings.anonymous_mode = v;
    }
    if let Some(v) = patch.queueing_enabled {
        settings.queueing_enabled = v;
    }
    if let Some(v) = patch.max_active_downloads {
        settings.active_downloads = v;
    }
    if let Some(v) = patch.max_active_torrents {
        settings.active_limit = v;
    }
    if let Some(v) = patch.max_active_uploads {
        settings.active_seeds = v;
    }
    if let Some(v) = patch.save_path {
        settings.download_dir = std::path::PathBuf::from(v);
    }
    if let Some(v) = patch.max_ratio {
        if v.is_nan() {
            return Err(QbtError::BadRequest("max_ratio NaN".into()));
        }
        settings.seed_ratio_limit = if v < 0.0 { None } else { Some(v) };
    }
    if let Some(v) = patch.max_ratio_enabled
        && !v
    {
        settings.seed_ratio_limit = None;
    }
    if let Some(v) = patch.max_ratio_act {
        settings.max_ratio_action = match v.as_str() {
            "pause" => MaxRatioAction::Pause,
            "remove" => MaxRatioAction::Remove,
            "enable_super_seeding" => MaxRatioAction::EnableSuperSeeding,
            _ => {
                return Err(QbtError::BadRequest(format!("invalid max_ratio_act: {v}")));
            }
        };
    }
    if let Some(v) = patch.create_subfolder_enabled {
        settings.create_subfolder = v;
    }
    if let Some(v) = patch.auto_tmm_enabled {
        settings.auto_manage_torrents = v;
    }
    if let Some(v) = patch.listen_port {
        settings.listen_port = v;
    }
    // Wire is minutes; our Settings is seconds.
    if let Some(v) = patch.max_seeding_time {
        settings.seed_time_limit_secs = if v < 0 { None } else { Some((v as u64) * 60) };
    }
    if let Some(v) = patch.max_seeding_time_enabled
        && !v
    {
        settings.seed_time_limit_secs = None;
    }
    if let Some(v) = patch.max_inactive_seeding_time {
        settings.inactive_seed_time_limit_secs = if v < 0 { None } else { Some((v as u64) * 60) };
    }
    if let Some(v) = patch.max_inactive_seeding_time_enabled
        && !v
    {
        settings.inactive_seed_time_limit_secs = None;
    }

    // M172a: rotating the web UI password. Hash on write, scrub the
    // plaintext before this function returns. Failure maps to 500 (not 400)
    // because a transient argon2 error is an internal-engine problem from
    // the caller's perspective — the input was valid, we failed to process.
    if let Some(pw) = patch.web_ui_password {
        let plaintext = zeroize::Zeroizing::new(pw);
        if !plaintext.is_empty() {
            let hash = irontide::session::hash_qbt_password(&plaintext)
                .map_err(|e| QbtError::Internal(format!("argon2 hash: {e}")))?;
            settings.qbt_compat.password_hash = hash;
            // Clear any residual legacy plaintext — the admin just rotated
            // the password, so the pre-migration value is irrelevant.
            settings.qbt_compat.password.clear();
        }
    }

    // M172a Lane B: CSRF + reverse-proxy toggles. CIDR list is parsed here
    // as a strictness gate (invalid entries produce 400 before validate()
    // runs, so the operator sees a precise error not a generic validation
    // failure). The on-disk form is still `Vec<String>`; parsing into
    // `Vec<IpNet>` happens lazily in the middleware.
    if let Some(v) = patch.web_ui_csrf_protection_enabled {
        settings.qbt_compat.csrf_protection_enabled = v;
    }
    if let Some(v) = patch.web_ui_host_header_validation_enabled {
        settings.qbt_compat.host_header_validation_enabled = v;
    }
    if let Some(v) = patch.web_ui_reverse_proxy_enabled {
        settings.qbt_compat.web_ui_reverse_proxy_enabled = v;
    }
    if let Some(raw) = patch.web_ui_reverse_proxies_list {
        // qBt serialises the list as a semicolon-joined string on the wire;
        // empty entries (from a trailing `;`) are dropped.
        let parsed: Vec<String> = raw
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        // Validate each entry parses before accepting the whole list.
        for entry in &parsed {
            if entry.parse::<ipnet::IpNet>().is_err() {
                return Err(QbtError::BadRequest(format!(
                    "invalid CIDR in web_ui_reverse_proxies_list: {entry}"
                )));
            }
        }
        settings.qbt_compat.web_ui_reverse_proxies_list = parsed;
    }

    // M172a Lane C: brute-force ban fields.
    if let Some(v) = patch.web_ui_max_auth_fail_count {
        settings.qbt_compat.max_failed_auth_count = v;
    }
    if let Some(v) = patch.web_ui_ban_duration {
        // qBt -1 sentinel = "leave unchanged"; `< -1` same handling
        // (matches qBt: any negative value means "ignore"). Positive
        // values must fit u64, which they always do for i64-positive.
        if v >= 0 {
            settings.qbt_compat.ban_duration_secs =
                u64::try_from(v).unwrap_or(settings.qbt_compat.ban_duration_secs);
        }
    }
    if let Some(v) = patch.bypass_local_auth {
        settings.qbt_compat.bypass_local_auth = v;
    }
    // Order: `_enabled` applied first so a patch that sets
    // `_enabled=false` + `_whitelist=..` produces a disabled list, and
    // `_enabled=true` + `_whitelist=..` produces the new list.
    if let Some(v) = patch.bypass_auth_subnet_whitelist_enabled
        && !v
    {
        settings.qbt_compat.bypass_auth_subnet_whitelist.clear();
    }
    if let Some(v) = patch.bypass_auth_subnet_whitelist {
        let parsed: Vec<String> = v
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        // Validate each CIDR so we fail the patch loudly rather than
        // silently-stripping a typo. Settings::validate also checks
        // this post-merge, but failing here keeps the BadRequest body
        // specific to the offending line.
        for cidr in &parsed {
            if cidr.parse::<ipnet::IpNet>().is_err() {
                return Err(QbtError::BadRequest(format!(
                    "invalid CIDR in bypass_auth_subnet_whitelist: {cidr}"
                )));
            }
        }
        settings.qbt_compat.bypass_auth_subnet_whitelist = parsed;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    //! D3.3 (M173 Lane C): unit coverage for `app.rs`.
    //!
    //! The async handlers (`version`, `webapi_version`, `preferences`,
    //! `set_preferences`) require a live `QbtState`, which in turn needs
    //! a full tokio `SessionHandle` — that's the integration-test
    //! ground covered by `qbt_v2_app.rs` and `qbt_v2_set_preferences.rs`.
    //!
    //! This module focuses on the pure logic that used to have no
    //! coverage: `build_info()` bitness / hardcoded versions,
    //! `parse_preferences_patch` (JSON + legacy `json=<...>` form paths),
    //! `apply_preferences_patch` field mapping, and rejection cases that
    //! must surface as `QbtError::BadRequest`.
    use super::*;
    use irontide::session::Settings;

    /// Handy small helper: call the internal parse + apply pipeline
    /// against a default `Settings` and return `(settings, applied_patch_flag)`.
    fn apply(patch_json: &str) -> Result<Settings, QbtError> {
        let patch: QbtPreferencesPatch = serde_json::from_str(patch_json).expect("parse json");
        let mut settings = Settings::default();
        apply_preferences_patch(&mut settings, patch)?;
        Ok(settings)
    }

    #[tokio::test]
    async fn build_info_reports_pinned_versions_and_computed_bitness() {
        // `build_info()` pins qt/libtorrent/boost/openssl to values that
        // mirror a recent qBt release. Bitness is computed from
        // `size_of::<usize>()` so we just assert it's in {32, 64} —
        // the canonical values for every production IronTide target.
        let resp = build_info().await;
        let json = match resp {
            QbtResponse::Json(v) => v,
            _ => panic!("build_info must return JSON"),
        };
        assert_eq!(json["qt"], "6.5.3");
        assert_eq!(json["libtorrent"], "2.0.9");
        assert_eq!(json["boost"], "1.83.0");
        assert_eq!(json["openssl"], "3.0.11");
        let bitness = json["bitness"].as_u64().expect("bitness is u64");
        assert!(
            bitness == 32 || bitness == 64,
            "bitness must be 32 or 64, got {bitness}"
        );
        // Sanity: match the live target.
        let expected = (std::mem::size_of::<usize>() as u64) * 8;
        assert_eq!(bitness, expected);
    }

    #[test]
    fn parse_preferences_patch_empty_body_yields_default() {
        let patch = parse_preferences_patch(b"").expect("empty body -> default");
        // Every field defaults to None.
        assert!(patch.dl_limit.is_none());
        assert!(patch.up_limit.is_none());
        assert!(patch.dht.is_none());
    }

    #[test]
    fn parse_preferences_patch_accepts_json_body() {
        let body = br#"{"dl_limit":1024,"dht":true,"encryption":1}"#;
        let patch = parse_preferences_patch(body).expect("json parse");
        assert_eq!(patch.dl_limit, Some(1024));
        assert_eq!(patch.dht, Some(true));
        assert_eq!(patch.encryption, Some(1));
    }

    #[test]
    fn parse_preferences_patch_falls_back_to_legacy_form() {
        // qBt's historical WebUI v2 POSTs `application/x-www-form-urlencoded`
        // with a single `json=<...>` field.
        let body = b"json=%7B%22dl_limit%22%3A42%7D"; // url-encoded {"dl_limit":42}
        let patch = parse_preferences_patch(body).expect("form fallback");
        assert_eq!(patch.dl_limit, Some(42));
    }

    #[test]
    fn parse_preferences_patch_rejects_garbage() {
        // Not JSON and not a well-formed `json=` form — must surface as
        // BadRequest so the client sees a descriptive 400.
        let body = b"this is neither json nor an urlencoded pair";
        let err = parse_preferences_patch(body).expect_err("garbage must error");
        assert!(matches!(err, QbtError::BadRequest(_)));
    }

    #[test]
    fn negative_dl_limit_clamps_to_zero_sentinel() {
        // qBt uses negative for "unlimited"; our model uses 0 as the
        // unlimited sentinel, so the wire -1 must round to 0.
        let s = apply(r#"{"dl_limit":-1}"#).expect("apply");
        assert_eq!(s.download_rate_limit, 0);
        let s = apply(r#"{"up_limit":-99}"#).expect("apply");
        assert_eq!(s.upload_rate_limit, 0);
    }

    #[test]
    fn max_ratio_nan_is_rejected_as_bad_request() {
        // The allowlist explicitly rejects NaN because it would render
        // as invalid JSON on the read side and break all *arr clients.
        // JSON has no syntactic NaN; build the patch directly via the
        // Rust type instead.
        let patch = QbtPreferencesPatch {
            max_ratio: Some(f64::NAN),
            ..QbtPreferencesPatch::default()
        };
        let mut settings = Settings::default();
        let err =
            apply_preferences_patch(&mut settings, patch).expect_err("NaN max_ratio must fail");
        assert!(
            matches!(err, QbtError::BadRequest(ref m) if m.contains("NaN")),
            "expected BadRequest(NaN); got {err:?}"
        );
    }

    #[test]
    fn encryption_unknown_int_is_rejected() {
        // Only 0/1/2 are valid on the wire. Anything else must 400.
        let err = apply(r#"{"encryption":7}"#).expect_err("bad encryption must fail");
        assert!(matches!(err, QbtError::BadRequest(_)));
    }

    #[test]
    fn max_ratio_enabled_false_clears_seed_ratio_even_with_value() {
        // qBt parity: the _enabled flag is authoritative. A patch that
        // sets both `max_ratio=1.5` and `max_ratio_enabled=false` must
        // produce a cleared seed_ratio_limit.
        let s = apply(r#"{"max_ratio":1.5,"max_ratio_enabled":false}"#).expect("apply");
        assert!(
            s.seed_ratio_limit.is_none(),
            "max_ratio_enabled=false overrides max_ratio"
        );
    }

    #[test]
    fn seed_time_minutes_convert_to_seconds_on_apply() {
        // Inverse of the DTO-side test in preferences.rs: wire is
        // minutes, storage is seconds, so a patch of 60 minutes must
        // land as 3600 seconds.
        let s = apply(r#"{"max_seeding_time":60}"#).expect("apply");
        assert_eq!(s.seed_time_limit_secs, Some(3600));
        // Negative means unset.
        let s = apply(r#"{"max_seeding_time":-1}"#).expect("apply");
        assert_eq!(s.seed_time_limit_secs, None);
    }

    #[test]
    fn max_ratio_act_invalid_slug_is_rejected() {
        let err = apply(r#"{"max_ratio_act":"pulverise"}"#).expect_err("unknown slug must fail");
        assert!(matches!(err, QbtError::BadRequest(_)));
    }

    #[test]
    fn invalid_cidr_in_reverse_proxies_rejected_with_specific_message() {
        // The patch validates each CIDR before accepting the list, so
        // an operator typo produces a precise error instead of the
        // generic post-merge validate() failure.
        let err = apply(r#"{"web_ui_reverse_proxies_list":"10.0.0.0/8;not-a-cidr"}"#)
            .expect_err("invalid CIDR must fail");
        match err {
            QbtError::BadRequest(msg) => {
                assert!(
                    msg.contains("not-a-cidr"),
                    "error must name the bad entry; got {msg}"
                );
            }
            other => panic!("expected BadRequest, got {other:?}"),
        }
    }

    #[test]
    fn max_connec_wins_over_per_torrent_when_both_set() {
        // Documented precedence: `max_connec` is applied last, so a
        // patch setting both fields resolves to the global value.
        let s = apply(r#"{"max_connec_per_torrent":50,"max_connec":200}"#).expect("apply");
        assert_eq!(s.max_peers_per_torrent, 200);
    }
}
