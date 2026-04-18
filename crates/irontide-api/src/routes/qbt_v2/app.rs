//! qBt v2 `app/*` endpoints (M168).
//!
//! Implemented:
//! - `GET /api/v2/app/version` — plain-text app version string.
//! - `GET /api/v2/app/webapiVersion` — plain-text webapi version string.
//! - `GET /api/v2/app/buildInfo` — JSON `{qt, libtorrent, boost, openssl, bitness}`.
//! - `GET /api/v2/app/preferences` — JSON with *arr-required fields.
//!
//! Deferred to M170:
//! - `POST /api/v2/app/setPreferences`
//! - `POST /api/v2/app/shutdown`

use axum::extract::State;

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
