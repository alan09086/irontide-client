//! qBt v2 authentication endpoints + middleware (M168).
//!
//! - `POST /api/v2/auth/login` — issues the `SID=` cookie on success.
//! - `POST /api/v2/auth/logout` — idempotent; invalidates the cookie.
//! - `qbt_gate` — returns 404 when `qbt_compat.enabled == false`.
//! - `require_sid` — returns 403 `Fails.` when the SID cookie is missing,
//!    malformed, or expired.
//!
//! # Threat model
//! Plaintext-password compare is intentional for M168 — argon2 lands in M171
//! together with CSRF and brute-force ban. Local-only binding (daemon flag
//! `--api-bind 127.0.0.1`) mitigates timing-side-channel extraction on a
//! shared subnet; a constant-time compare would close the remaining gap but
//! is deferred to M171's full auth rewrite.

use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;

use super::response::{QbtError, QbtResponse};
use super::session_store::SessionStore;
use super::state::QbtState;

/// Form body for `POST /api/v2/auth/login`.
///
/// `username` / `password` are required. Missing fields fall through to axum
/// returning 422 automatically (acceptable — real qBt returns 400, but *arr
/// clients never hit this path with malformed bodies).
#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

/// `POST /api/v2/auth/login` handler.
///
/// Form body is URL-encoded. Correct creds return 200 `Ok.` with a
/// `Set-Cookie: SID=<token>; HttpOnly; Path=/; SameSite=Lax` header; wrong
/// creds return 403 `Fails.`.
pub async fn login(
    State(state): State<QbtState>,
    axum::extract::Form(form): axum::extract::Form<LoginForm>,
) -> Result<QbtResponse, QbtError> {
    let settings = state
        .session
        .settings()
        .await
        .map_err(|e| QbtError::Internal(format!("read settings: {e}")))?;
    let cfg = &settings.qbt_compat;

    // Username + password match check. Plaintext for M168.
    if form.username != cfg.username || form.password != cfg.password {
        return Err(QbtError::Forbidden);
    }

    let sid = state
        .store
        .create(form.username.clone())
        .map_err(|e| QbtError::Internal(format!("token gen: {e}")))?;

    // Cookie attributes: HttpOnly (JS can't read it), Path=/ (sent to all
    // sub-paths), SameSite=Lax (blocks CSRF from cross-site POST). Secure
    // deferred to M171+ when TLS termination lands.
    let cookie_value = format!("SID={sid}; HttpOnly; Path=/; SameSite=Lax");
    Ok(QbtResponse::Ok {
        set_cookie: Some(cookie_value),
    })
}

/// `POST /api/v2/auth/logout` handler.
///
/// Idempotent: always returns 200 `Ok.`, regardless of whether the cookie is
/// valid, expired, or absent. Matches real qBt behaviour.
pub async fn logout(State(state): State<QbtState>, jar: CookieJar) -> QbtResponse {
    if let Some(cookie) = jar.get("SID") {
        state.store.invalidate(cookie.value());
    }
    QbtResponse::ok()
}

/// Middleware: 404 when `qbt_compat.enabled == false`.
///
/// Runs before `require_sid` so that a disabled daemon never leaks the
/// presence of the `/api/v2/*` routes. Returning 403 (auth failure) or 501
/// (not implemented) would both be worse: 404 says "there is no such URL",
/// which matches how a vanilla IronTide would respond.
pub async fn qbt_gate(State(state): State<QbtState>, req: Request, next: Next) -> Response {
    // settings() is a channel round-trip — cheap, and allows runtime toggle.
    let enabled = state
        .session
        .settings()
        .await
        .map(|s| s.qbt_compat.enabled)
        .unwrap_or(false);

    if !enabled {
        return (StatusCode::NOT_FOUND, Body::empty()).into_response();
    }
    next.run(req).await
}

/// Middleware: 403 `Fails.` when the SID cookie is missing, malformed, or
/// has expired. Applies to every `/api/v2/*` route EXCEPT `auth/login` — the
/// login route is registered on a sub-router that does not include this
/// middleware.
pub async fn require_sid(State(state): State<QbtState>, jar: CookieJar, req: Request, next: Next) -> Response {
    let Some(sid_cookie) = jar.get("SID") else {
        return QbtError::Forbidden.into_response();
    };
    let sid = sid_cookie.value();
    if sid.is_empty() {
        return QbtError::Forbidden.into_response();
    }
    if state.store.validate(sid).is_none() {
        return QbtError::Forbidden.into_response();
    }
    next.run(req).await
}

/// Build the auth store handle that the rest of the qbt_v2 module shares.
pub fn build_session_store(ttl_secs: u64, max_sessions: usize) -> Arc<SessionStore> {
    Arc::new(SessionStore::new(
        std::time::Duration::from_secs(ttl_secs),
        max_sessions,
    ))
}
