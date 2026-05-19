//! HTTP response helpers for the qBittorrent `WebUI` v2 compatibility surface.
//!
//! The qBt v2 API is quirky:
//! - `auth/login` / `auth/logout` return **plain text** (`Ok.` / `Fails.`).
//! - `app/version` / `app/webapiVersion` return **plain text** bodies.
//! - `app/buildInfo`, `app/preferences`, `torrents/info`, etc. return **JSON**.
//! - 403 responses (auth failure) return the literal body `Fails.`.
//!
//! `QbtResponse` centralises Content-Type handling so handlers don't drift.

use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Unified response type for the qBt v2 compatibility layer.
///
/// Pick the variant that matches the upstream qBt behaviour — plain text for
/// auth and version endpoints, JSON for data endpoints.
pub enum QbtResponse {
    /// Plain-text 200 body with `Content-Type: text/plain; charset=utf-8`.
    PlainText(String),
    /// JSON 200 body serialised via `serde_json`.
    Json(serde_json::Value),
    /// A 200 `Ok.` body with an optional `Set-Cookie` header. Used by
    /// `auth/login` (with cookie) and `auth/logout` (without).
    Ok { set_cookie: Option<String> },
}

impl QbtResponse {
    /// Convenience for "empty success" with no cookie.
    #[must_use]
    pub fn ok() -> Self {
        Self::Ok { set_cookie: None }
    }
}

impl IntoResponse for QbtResponse {
    fn into_response(self) -> Response {
        match self {
            Self::PlainText(body) => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
                (StatusCode::OK, headers, body).into_response()
            }
            Self::Json(value) => {
                // axum::Json handles Content-Type: application/json
                (StatusCode::OK, axum::Json(value)).into_response()
            }
            Self::Ok { set_cookie } => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
                if let Some(cookie) = set_cookie
                    && let Ok(val) = HeaderValue::from_str(&cookie)
                {
                    headers.insert(header::SET_COOKIE, val);
                }
                (StatusCode::OK, headers, "Ok.").into_response()
            }
        }
    }
}

/// Error variants that handlers can return to map to qBt-style HTTP responses.
///
/// qBt is permissive with status codes — it uses 403 for most auth / access
/// failures (with body `Fails.`) and 400 for malformed input.
#[derive(Debug)]
pub enum QbtError {
    /// 403 with body `Fails.` — auth / session invalid / wrong creds.
    Forbidden,
    /// 400 with a plain-text message.
    BadRequest(String),
    /// 404 with no body — used when the daemon is enabled but a referenced
    /// entity (hash, setting) is missing.
    NotFound,
    /// 409 Conflict — e.g. duplicate torrent on add.
    Conflict(String),
    /// Anything else — 500 with the message.
    Internal(String),
}

impl IntoResponse for QbtError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
                (StatusCode::FORBIDDEN, headers, "Fails.").into_response()
            }
            Self::BadRequest(msg) => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
                (StatusCode::BAD_REQUEST, headers, msg).into_response()
            }
            Self::NotFound => StatusCode::NOT_FOUND.into_response(),
            Self::Conflict(msg) => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
                (StatusCode::CONFLICT, headers, msg).into_response()
            }
            Self::Internal(msg) => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
                (StatusCode::INTERNAL_SERVER_ERROR, headers, msg).into_response()
            }
        }
    }
}

/// Convenience: serialise any `Serialize` value straight into a `QbtResponse::Json`.
///
/// # Errors
///
/// Returns an error if JSON serialization fails.
#[allow(dead_code)]
pub fn json_ok<T: Serialize>(value: &T) -> Result<QbtResponse, QbtError> {
    let v =
        serde_json::to_value(value).map_err(|e| QbtError::Internal(format!("serialise: {e}")))?;
    Ok(QbtResponse::Json(v))
}
