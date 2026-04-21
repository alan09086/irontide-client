//! qBt v2 authentication endpoints + middleware (M168, argon2 in M172a).
//!
//! - `POST /api/v2/auth/login` — issues the `SID=` cookie on success.
//! - `POST /api/v2/auth/logout` — idempotent; invalidates the cookie.
//! - `qbt_gate` — returns 404 when `qbt_compat.enabled == false`.
//! - `require_sid` — returns 403 `Fails.` when the SID cookie is missing,
//!   malformed, or expired.
//!
//! # Threat model (M172a)
//! Passwords are stored as argon2id PHC strings and verified via
//! `argon2::Argon2::verify_password`, which is internally constant-time —
//! closing the `!=` timing-side-channel gap M168 had. Concurrent verifies
//! are bounded by a shared `tokio::sync::Semaphore` so a login flood can
//! consume at most `permits × 19 MiB` of memory. CSRF + brute-force ban
//! land in Lanes B and C of M172.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use zeroize::Zeroizing;

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
    /// Plaintext password supplied by the caller. See [`login`] — we wrap
    /// this in [`Zeroizing`] during verification to minimise residual copies
    /// on the stack (serde/axum already copied the body upstream, so this is
    /// partial defence-in-depth rather than substantive scrub).
    pub password: String,
}

/// `POST /api/v2/auth/login` handler.
///
/// Form body is URL-encoded. Correct creds return 200 `Ok.` with a
/// `Set-Cookie: SID=<token>; HttpOnly; Path=/; SameSite=Lax` header; wrong
/// creds return 403 `Fails.`.
///
/// # M172a verification flow
///
/// 1. Acquire a permit from `state.argon2_semaphore` (bounds concurrent
///    argon2 CPU/memory consumption).
/// 2. Parse `settings.qbt_compat.password_hash` as a PHC string. A malformed
///    hash is treated identically to a mismatch (C2) — returns 403 `Fails.`
///    without leaking the distinct failure mode.
/// 3. If the hash is empty, fall back to constant-time plaintext compare
///    against the legacy `settings.qbt_compat.password` (grandfather path:
///    serves the in-flight boot where `migrate_qbt_credentials` failed but
///    kept the plaintext in memory).
/// 4. Zero the plaintext form buffer immediately after the verify, regardless
///    of outcome.
///
/// # Rejected paths
///
/// `ConnectInfo<SocketAddr>` is a required extractor: if axum was started
/// without `into_make_service_with_connect_info` we return 500 rather than
/// silently authenticating without a client IP. This matters for Lanes B
/// and C that use the client IP for trust-hop resolution and brute-force
/// rate-limiting. See `ApiServer::run` at `crates/irontide-api/src/lib.rs`
/// for the serve site.
pub async fn login(
    State(state): State<QbtState>,
    ConnectInfo(_peer): ConnectInfo<SocketAddr>,
    axum::extract::Form(form): axum::extract::Form<LoginForm>,
) -> Result<QbtResponse, QbtError> {
    // `ConnectInfo<SocketAddr>` is a REQUIRED extractor (C3). If the API
    // server was bound via `axum::serve(listener, router)` instead of
    // `axum::serve(listener, router.into_make_service_with_connect_info
    // ::<SocketAddr>())`, extraction rejects with a 500 — that's intentional
    // because Lanes B/C need the peer address to key CSRF trust-hop and
    // brute-force ban. Test fixtures must use `TcpListener::bind` +
    // `into_make_service_with_connect_info` (see `tests/qbt_v2_auth.rs`
    // `test_session_with_qbt_tcp`).
    let _peer_addr = _peer;
    let settings = state
        .session
        .settings()
        .await
        .map_err(|e| QbtError::Internal(format!("read settings: {e}")))?;
    let cfg = &settings.qbt_compat;

    // Wrap the plaintext in Zeroizing to minimise stack residue. Serde
    // already made at least one intermediate copy so this is partial scrub
    // rather than full. The migration-path zeroize in `migrate_qbt_credentials`
    // is the substantive one.
    let plaintext = Zeroizing::new(form.password);

    if form.username != cfg.username {
        return Err(QbtError::Forbidden);
    }

    // G2: bounded concurrent verifications — protects peak memory under flood.
    let _permit = state
        .argon2_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| QbtError::Internal(format!("argon2 semaphore closed: {e}")))?;

    if verify_qbt_password(cfg, &plaintext) {
        let sid = state
            .store
            .create(form.username.clone())
            .map_err(|e| QbtError::Internal(format!("token gen: {e}")))?;

        // Cookie attributes: HttpOnly (JS can't read it), Path=/ (sent to all
        // sub-paths), SameSite=Lax (blocks CSRF from cross-site POST). Secure
        // deferred to M172b+ when TLS termination lands.
        let cookie_value = format!("SID={sid}; HttpOnly; Path=/; SameSite=Lax");
        Ok(QbtResponse::Ok {
            set_cookie: Some(cookie_value),
        })
    } else {
        Err(QbtError::Forbidden)
    }
}

/// Verify `plaintext` against the configured credentials (M172a).
///
/// Prefers the argon2id PHC hash in `cfg.password_hash`. Any hash-side error
/// (malformed PHC string, parameter mismatch, verify mismatch) is mapped to
/// `false` — indistinguishable from a wrong-password rejection (C2, no side
/// channel on malformed hashes).
///
/// Falls back to constant-time plaintext compare against `cfg.password` when
/// the hash is empty — the grandfather path for a boot where in-memory
/// migration failed.
fn verify_qbt_password(cfg: &irontide::session::QbtCompatSettings, plaintext: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    use argon2::Argon2;

    if !cfg.password_hash.is_empty() {
        let Ok(parsed) = PasswordHash::new(&cfg.password_hash) else {
            // C2: malformed hash → indistinguishable from a wrong-password
            // result. Do not leak via distinct error.
            return false;
        };
        return Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok();
    }

    // Legacy plaintext grandfather path: constant-time compare using subtle
    // via argon2's re-export. Falls back to a byte-wise scan if lengths
    // differ — any length difference is already a mismatch.
    if plaintext.len() != cfg.password.len() {
        return false;
    }
    constant_time_eq(plaintext.as_bytes(), cfg.password.as_bytes())
}

/// Byte-wise constant-time equality for equal-length slices.
#[inline]
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
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
