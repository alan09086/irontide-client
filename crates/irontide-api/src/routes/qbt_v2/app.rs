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
/// qBt's WebUI v2 historically POSTs this as
/// `application/x-www-form-urlencoded` with a single `json=<stringified
/// JSON>` field, but recent `*arr` versions just POST an
/// `application/json` body. The handler accepts either.
///
/// When any field in the patch requires a session restart to take effect
/// (listen_port, dht, lsd, pex, encryption, anonymous_mode, save_path) the
/// response carries an `X-IronTide-Restart-Pending: <comma-joined-fields>`
/// header. Immediate fields (rate limiters, peer cap, queueing, ratio
/// action, create_subfolder, auto_tmm, max_ratio) produce no header.
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

    apply_preferences_patch(&mut settings, patch)?;

    settings
        .validate()
        .map_err(|e| QbtError::BadRequest(format!("invalid settings: {e}")))?;

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
        .map_err(|e| QbtError::Internal(format!("apply settings: {e}")))?;

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
///   `max_peers_per_torrent` (IronTide has no session-wide cap distinct from
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
