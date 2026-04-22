//! qBt v2 authentication endpoints + middleware (M168, argon2 in M172a).
//!
//! - `POST /api/v2/auth/login` — issues the `SID=` cookie on success.
//! - `POST /api/v2/auth/logout` — idempotent; invalidates the cookie.
//! - `qbt_gate` — returns 404 when `qbt_compat.enabled == false` (explicit
//!   operator opt-out as of v0.172.1; enabled is the default).
//! - `require_sid` — returns 403 `Fails.` when the SID cookie is missing,
//!   malformed, or expired.
//!
//! # Threat model (M172a)
//! Passwords are stored as argon2id PHC strings and verified via
//! `argon2::Argon2::verify_password`, which is internally constant-time —
//! closing the `!=` timing-side-channel gap M168 had. Concurrent verifies
//! are bounded by a shared `tokio::sync::Semaphore` so a login flood can
//! consume at most `permits × 19 MiB` of memory. The verify itself runs on
//! the blocking pool (see [`login`]) so the async event loop is never
//! stalled by the ~80-120 ms of argon2 CPU work. The username-equality
//! check is DELIBERATELY followed by a dummy argon2 verify on mismatch so
//! the total wall-clock cost of wrong-user / wrong-password / correct-user
//! paths are equalised — closing the classic "fast-fail on unknown username"
//! enumeration oracle. CSRF + brute-force ban land in Lanes B and C of
//! M172.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use zeroize::Zeroizing;

use irontide::session::DEFAULT_ADMINADMIN_HASH;

use super::brute_force::AdmitGuard;
use super::response::{QbtError, QbtResponse};
use super::session_store::SessionStore;
use super::state::{QbtState, resolve_client_ip_from_parts};

/// Placeholder password fed into the argon2 verify on the username-mismatch
/// timing-equaliser path. The bytes never match anything (the real hash is
/// `DEFAULT_ADMINADMIN_HASH` of the string `"adminadmin"`), so this will always
/// verify to `false`; the only observable is the wall-clock cost which by
/// design matches the real-password path.
const USERNAME_MISMATCH_EQUALIZER_INPUT: &str = "wrong-password-timing-equalizer";

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
/// 1. Capture the `username == cfg.username` boolean but do NOT short-circuit
///    yet — a fast-fail on username mismatch would leak "unknown user" vs
///    "user exists, wrong password" via wall-clock timing (argon2 costs
///    80-120 ms). The real qBt factory default is `admin`, so any operator
///    who kept the default would be fingerprint-enumerable.
/// 2. Acquire a permit from `state.argon2_semaphore` (bounds concurrent
///    argon2 CPU/memory consumption, even across the blocking pool).
/// 3. Dispatch the argon2 verify to [`tokio::task::spawn_blocking`]. On a
///    `current_thread` runtime, calling `verify_password` inline would stall
///    the entire async event loop for ~100 ms. On a multi-threaded runtime
///    with N workers, a burst of N inline verifies starves every other task.
///    The permit is held across the spawn so the total in-flight argon2
///    work stays bounded.
/// 4. If `username_matches` is false, feed `DEFAULT_ADMINADMIN_HASH` plus a
///    throwaway password into the verify so the wall-clock cost matches the
///    real-password path. The verify will always return `false`; the branch
///    below ignores the result when `!username_matches`.
/// 5. Branch on `username_matches && verified` only AFTER the verify
///    completes, so the three outcomes (wrong user / wrong pw / both-right)
///    resolve in statistically similar time.
/// 6. Zero the plaintext form buffer immediately after the verify, regardless
///    of outcome.
///
/// Malformed PHC hashes short-circuit to `false` inside the blocking task
/// BEFORE invoking the argon2 verify — this is a server-side configuration
/// leak (operator's own `password_hash` is corrupt), not an attacker-
/// distinguishable external oracle. We accept the opacity-vs-timing trade:
/// malformed hashes should be caught by `Settings::validate()` at startup
/// and be a near-zero-probability production state, whereas a correctly
/// configured server's user-mismatch / password-mismatch paths must run in
/// constant wall-clock time (G1 is about external attackers, not
/// misconfiguration forensics).
///
/// # Rejected paths
///
/// `ConnectInfo<SocketAddr>` is a required extractor: if axum was started
/// without `into_make_service_with_connect_info` we return 500 rather than
/// silently authenticating without a client IP. This matters for Lanes B
/// and C that use the client IP for trust-hop resolution and brute-force
/// rate-limiting. See `ApiServer::run` at `crates/irontide-api/src/lib.rs`
/// for the serve site.
///
/// # Errors
///
/// Returns [`QbtError::Forbidden`] on wrong credentials (including username
/// mismatch and malformed stored hash). Returns [`QbtError::Internal`] on
/// infrastructure failures: settings-read failure, semaphore poison, argon2
/// worker panic (a `JoinError` on the blocking task).
pub async fn login(
    State(state): State<QbtState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
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
    let settings = state
        .session
        .settings()
        .await
        .map_err(|e| QbtError::Internal(format!("read settings: {e}")))?;
    let cfg = &settings.qbt_compat;

    // M172a Lane C: resolve the real client IP via Lane A's trust-hop
    // algorithm — consults `X-Forwarded-For` with rightmost-non-trusted
    // selection when a reverse-proxy CIDR list is configured, otherwise
    // returns the raw TCP peer.
    let client_ip = resolve_client_ip_from_parts(Some(peer.ip()), &headers, &state);

    // M172a Lane C: CIDR-whitelist bypass — skip auth entirely for IPs
    // inside the operator's `bypass_auth_subnet_whitelist`. The list
    // lives in a shared RwLock so setPreferences updates take effect
    // on the next login without rebuilding the router.
    {
        let whitelist = state.bypass_auth_subnet_whitelist.read();
        if whitelist.iter().any(|cidr| cidr.contains(&client_ip)) {
            return finalise_login(&state, true, &form.username);
        }
    }

    // M172a Lane C: loopback bypass — `bypass_local_auth = true` lets
    // any IP inside 127.0.0.0/8 / ::1 log in without credentials. qBt
    // parity: their "Bypass authentication for clients on localhost"
    // toggle has the same semantics.
    if cfg.bypass_local_auth && client_ip.is_loopback() {
        return finalise_login(&state, true, &form.username);
    }

    // M172a Lane C: brute-force gate. Checks whether the attacker's IP
    // is already banned OR at the pending-cap, increments the in-flight
    // pending counter, and returns an AdmitGuard RAII token we MUST hold
    // until after we've recorded the outcome — that way a thundering-herd
    // flood cannot saturate argon2 beyond `max_failed_auth_count`
    // verifies per IP.
    let admit_guard = match state.brute_force.check_and_admit(
        client_ip,
        cfg.max_failed_auth_count,
        cfg.ban_duration_secs,
    ) {
        Ok(guard) => guard,
        Err(_) => {
            // C4 qBt parity: 403 `Fails.` — same response body as the
            // wrong-password path so an attacker can't distinguish "am
            // I banned?" from "did I guess wrong?".
            return Err(QbtError::Forbidden);
        }
    };

    // Wrap the plaintext in Zeroizing to minimise stack residue. Serde
    // already made at least one intermediate copy so this is partial scrub
    // rather than full. The migration-path zeroize in `migrate_qbt_credentials`
    // is the substantive one.
    let plaintext = Zeroizing::new(form.password);

    // Capture the equality bit BUT DO NOT BRANCH YET — short-circuiting here
    // is the timing-oracle bug we are closing. See the module-level threat
    // model and the step-by-step rustdoc above.
    let username_matches = form.username == cfg.username;

    // G2: bounded concurrent verifications — protects peak memory under flood.
    // `acquire_owned` so the permit survives the move into `spawn_blocking`.
    let permit = state
        .argon2_semaphore
        .clone()
        .acquire_owned()
        .await
        .map_err(|e| QbtError::Internal(format!("argon2 semaphore closed: {e}")))?;

    // Grandfather path: hash empty + plaintext set. The compare is O(len)
    // nanoseconds — cheap enough to stay on the async side even for the
    // timing-equalised dummy branch. We run the compare against the LEGACY
    // plaintext regardless of `username_matches` so both paths take the same
    // handful of nanoseconds inside this mode.
    if cfg.password_hash.is_empty() {
        let verified = if plaintext.len() == cfg.password.len() {
            constant_time_eq(plaintext.as_bytes(), cfg.password.as_bytes())
        } else {
            // Equaliser: still walk `cfg.password.len()` bytes in a
            // constant-time compare against itself so the branch cost does
            // not leak the plaintext length. Result is ignored.
            let _ = constant_time_eq(cfg.password.as_bytes(), cfg.password.as_bytes());
            false
        };
        drop(permit);
        return finalise_login_with_tracking(
            &state,
            client_ip,
            username_matches && verified,
            &form.username,
            cfg.max_failed_auth_count,
            cfg.ban_duration_secs,
            admit_guard,
        );
    }

    // Normal path: argon2id verify, routed through `spawn_blocking` so the
    // ~80-120 ms of CPU work does not stall the tokio scheduler (worst on a
    // `current_thread` runtime, but also harmful on multi-thread runtimes
    // with ≤ semaphore-permits workers).
    let hash_to_verify: String = if username_matches {
        cfg.password_hash.clone()
    } else {
        DEFAULT_ADMINADMIN_HASH.to_owned()
    };
    let pw_to_verify: Zeroizing<String> = if username_matches {
        Zeroizing::new(plaintext.as_str().to_owned())
    } else {
        Zeroizing::new(USERNAME_MISMATCH_EQUALIZER_INPUT.to_owned())
    };

    let verify_result: Result<bool, argon2::password_hash::Error> =
        tokio::task::spawn_blocking(move || {
            verify_qbt_password_blocking(&hash_to_verify, &pw_to_verify)
        })
        .await
        .map_err(|_| QbtError::Internal("argon2 worker panicked".to_owned()))?;

    drop(permit);

    // A `password_hash::Error` here means the argon2 crate rejected the
    // operation for a reason OTHER than `Error::Password` (which is already
    // folded into `Ok(false)` inside `verify_qbt_password_blocking`). Treat
    // as infrastructure failure — the operator should see a 500, not a 403
    // masquerading as wrong-creds, so they can diagnose the misconfiguration.
    let verified = verify_result.map_err(|e| QbtError::Internal(format!("argon2 verify: {e}")))?;

    // Authoritative branch: BOTH username matched AND password verified. The
    // work above (argon2 dummy verify on mismatch) is not skipped because
    // skipping it is exactly the timing oracle we're closing.
    finalise_login_with_tracking(
        &state,
        client_ip,
        username_matches && verified,
        &form.username,
        cfg.max_failed_auth_count,
        cfg.ban_duration_secs,
        admit_guard,
    )
}

/// Complete the login flow: issue a session token on success, or return
/// opaque `QbtError::Forbidden` on failure. Extracted so both the normal and
/// grandfather paths terminate through the same code.
fn finalise_login(
    state: &QbtState,
    authenticated: bool,
    username: &str,
) -> Result<QbtResponse, QbtError> {
    if !authenticated {
        return Err(QbtError::Forbidden);
    }
    let sid = state
        .store
        .create(username.to_owned())
        .map_err(|e| QbtError::Internal(format!("token gen: {e}")))?;

    // Cookie attributes: HttpOnly (JS can't read it), Path=/ (sent to all
    // sub-paths), SameSite=Lax (blocks CSRF from cross-site POST). Secure
    // deferred to M172b+ when TLS termination lands.
    let cookie_value = format!("SID={sid}; HttpOnly; Path=/; SameSite=Lax");
    Ok(QbtResponse::Ok {
        set_cookie: Some(cookie_value),
    })
}

/// M172a Lane C: terminate login with brute-force-registry bookkeeping.
///
/// Records the outcome (success clears the counter; failure increments
/// it and stamps `banned_until` on cross-threshold), THEN drops the
/// admission guard so the `pending` decrement happens strictly after the
/// state transition lands. Returning `Err(Forbidden)` here yields the
/// same `Fails.` 403 body as wrong-password and banned-IP paths — qBt
/// parity C4.
fn finalise_login_with_tracking(
    state: &QbtState,
    client_ip: IpAddr,
    authenticated: bool,
    username: &str,
    max: u32,
    ban_secs: u64,
    admit_guard: AdmitGuard,
) -> Result<QbtResponse, QbtError> {
    if authenticated {
        state.brute_force.record_success(client_ip);
        // Drop the guard AFTER record_success so the state transition is
        // observed before pending--.
        drop(admit_guard);
        let sid = state
            .store
            .create(username.to_owned())
            .map_err(|e| QbtError::Internal(format!("token gen: {e}")))?;
        let cookie_value = format!("SID={sid}; HttpOnly; Path=/; SameSite=Lax");
        Ok(QbtResponse::Ok {
            set_cookie: Some(cookie_value),
        })
    } else {
        state.brute_force.record_failure(client_ip, max, ban_secs);
        drop(admit_guard);
        Err(QbtError::Forbidden)
    }
}

/// Blocking-pool argon2id verify (M172a).
///
/// Invoked only via [`tokio::task::spawn_blocking`] so the ~80-120 ms of
/// CPU work does not stall the async scheduler. Returns:
/// - `Ok(true)` on a successful verify,
/// - `Ok(false)` on a mismatch (the argon2 crate's `Error::Password` case)
///   OR on a malformed PHC string (C2, see below),
/// - `Err(...)` on any other `password_hash::Error` — treated by the caller
///   as a 500 because it indicates a runtime misconfiguration rather than a
///   wrong password.
///
/// **C2 note (server-side opacity vs. timing leak):** a malformed PHC string
/// short-circuits to `Ok(false)` *before* the verify call. This means the
/// total wall-clock cost of "malformed hash + any password" is noticeably
/// shorter than a correct verify — a timing leak to the remote attacker
/// *if and only if* the operator managed to install an unparseable hash
/// (Settings::validate() should catch this at startup, so the probability
/// is near-zero in production). The alternative (always calling
/// `verify_password` even when we know the parse failed) would need a dummy
/// `PasswordHash` we could reliably construct; argon2 does not expose such a
/// helper. We accept the trade; see the rustdoc on [`login`] for the full
/// rationale.
fn verify_qbt_password_blocking(
    password_hash: &str,
    plaintext: &Zeroizing<String>,
) -> Result<bool, argon2::password_hash::Error> {
    use argon2::Argon2;
    use argon2::password_hash::{Error as PhcError, PasswordHash, PasswordVerifier};

    let parsed = match PasswordHash::new(password_hash) {
        Ok(p) => p,
        // C2: malformed operator-side hash. Opaque to the client; investigate
        // via server-side logging (logged at the call site if needed).
        Err(_) => return Ok(false),
    };
    match Argon2::default().verify_password(plaintext.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(PhcError::Password) => Ok(false),
        Err(other) => Err(other),
    }
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
/// Runs before `require_sid` so that an operator-disabled daemon never
/// leaks the presence of the `/api/v2/*` routes. Returning 403 (auth
/// failure) or 501 (not implemented) would both be worse: 404 says "there
/// is no such URL", which matches how a vanilla IronTide with the compat
/// surface opted-out would respond. Enabled-by-default as of v0.172.1.
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
pub async fn require_sid(
    State(state): State<QbtState>,
    jar: CookieJar,
    req: Request,
    next: Next,
) -> Response {
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
