//! Shared state for the qBt v2 sub-router.
//!
//! Bundles the upstream `SessionHandle` with the in-memory `SessionStore`
//! that tracks authenticated cookies. This is the only place where the
//! qBt v2 surface intersects the main engine state.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::Request;
use axum::http::HeaderMap;
use ipnet::IpNet;
use parking_lot::RwLock;
use tokio::sync::Semaphore;

use irontide::session::SessionHandle;

use super::brute_force::BruteForceRegistry;
use super::session_store::SessionStore;

/// Cheap-to-clone state for every qBt v2 handler and middleware.
#[derive(Clone)]
pub struct QbtState {
    pub session: Arc<SessionHandle>,
    pub store: Arc<SessionStore>,
    /// M172a G2: bounded semaphore that gates concurrent argon2 verifications.
    /// Default size: `num_cpus::get() * 2`, clamped to `[2, 16]`. Caps peak
    /// memory under a login-flood at `permits * 19 MiB ≈ 300 MiB` worst case.
    /// Overrideable via `Settings.qbt_compat.max_concurrent_argon2_ops`.
    pub argon2_semaphore: Arc<Semaphore>,
    /// M172a A7 scaffold: CIDRs trusted to supply `X-Forwarded-For` headers,
    /// consulted by [`resolve_client_ip`]. Populated by Lane B (`webUiReverseProxiesEnabled`
    /// / `webUiReverseProxiesList`) — Lane A ships this empty.
    /// FIXME(M172b Lane B): populate from `Settings.qbt_compat.web_ui_reverse_proxies_list`.
    pub reverse_proxies_list: Arc<RwLock<Vec<IpNet>>>,
    /// M172a Lane C: CIDRs that bypass qBt v2 authentication entirely.
    /// Populated at router construction from
    /// `Settings.qbt_compat.bypass_auth_subnet_whitelist` (best-effort, via a
    /// fire-and-forget startup task — `build_router` is sync, see
    /// [`super::build_router`]) and kept in sync by the `setPreferences`
    /// apply path (see `classify_immediate` in irontide-session).
    pub bypass_auth_subnet_whitelist: Arc<RwLock<Vec<IpNet>>>,
    /// M172a Lane C: per-IP brute-force-ban registry. Shared across the
    /// auth handler and the `setPreferences` apply path so runtime-reconfig
    /// of `max_failed_auth_count` / `ban_duration_secs` takes effect on the
    /// next login attempt. The registry's *capacity* is fixed at router
    /// construction — runtime changes to
    /// `qbt_compat.brute_force_registry_capacity` only affect the daemon
    /// on next restart (documented in `classify_immediate`).
    pub brute_force: Arc<BruteForceRegistry>,
}

impl QbtState {
    /// Construct a cheap-to-clone state bundle. The CIDR lists ship empty;
    /// they are populated from `Settings.qbt_compat` either by the async
    /// startup task in [`super::build_router`] or by the
    /// `setPreferences` apply path. The argon2 semaphore is sized from
    /// `num_cpus::get() * 2` clamped to `[2, 16]`, with an optional override
    /// from `qbt_compat.max_concurrent_argon2_ops`. The brute-force registry
    /// uses `brute_force_capacity` (typically
    /// `Settings.qbt_compat.brute_force_registry_capacity` or the
    /// `DEFAULT_REGISTRY_CAPACITY` fallback).
    #[must_use]
    pub fn new(
        session: Arc<SessionHandle>,
        store: Arc<SessionStore>,
        argon2_permits: usize,
        brute_force_capacity: usize,
    ) -> Self {
        Self {
            session,
            store,
            argon2_semaphore: Arc::new(Semaphore::new(argon2_permits)),
            reverse_proxies_list: Arc::new(RwLock::new(Vec::new())),
            bypass_auth_subnet_whitelist: Arc::new(RwLock::new(Vec::new())),
            brute_force: BruteForceRegistry::new(brute_force_capacity),
        }
    }
}

/// Compute the default argon2 verification-concurrency permit count
/// (M172a G2).
///
/// Formula: `num_cpus::get() * 2`, saturating at both ends to `[2, 16]`.
/// Callers can override via `Settings.qbt_compat.max_concurrent_argon2_ops`.
#[must_use]
pub fn default_argon2_permits(override_value: Option<u32>) -> usize {
    if let Some(n) = override_value
        && n > 0
    {
        return usize::try_from(n).unwrap_or(16).min(16);
    }
    num_cpus::get().saturating_mul(2).clamp(2, 16)
}

/// Resolve the originating client IP for a request (M172a C1 / G5).
///
/// Walks the chain `[XFF-entries..., peer]` from rightmost (nearest hop) to
/// leftmost (furthest claimed hop), returning the first address that is NOT
/// inside `state.reverse_proxies_list`. If every hop claims to be a trusted
/// proxy, falls back to the leftmost XFF entry (original client's claimed
/// address). If `XFF` is absent or unparseable, returns the TCP peer address.
///
/// Both Lane B (CSRF origin-check whitelisting for proxied hosts) and Lane C
/// (brute-force ban keyed on real client IP) consume this helper.
#[must_use]
pub fn resolve_client_ip(req: &Request, state: &QbtState) -> IpAddr {
    let peer = req
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip());
    resolve_client_ip_from_parts(peer, req.headers(), state)
}

/// Same XFF trust-hop algorithm as [`resolve_client_ip`], but callable
/// from a handler that has already destructured the request (no `&Request`
/// available). Used by the qBt v2 login handler (Lane C) which receives
/// `ConnectInfo<SocketAddr>` + `HeaderMap` directly.
#[must_use]
pub fn resolve_client_ip_from_parts(
    peer: Option<IpAddr>,
    headers: &HeaderMap,
    state: &QbtState,
) -> IpAddr {
    let xff_entries = parse_xff_header(headers);
    let trusted = state.reverse_proxies_list.read();

    // chain = [XFF entries..., peer]
    let mut chain: Vec<IpAddr> = xff_entries;
    if let Some(p) = peer {
        chain.push(p);
    }

    // Rightmost-first scan: return the first non-trusted address.
    for addr in chain.iter().rev() {
        if !ip_in_any_cidr(*addr, &trusted) {
            return *addr;
        }
    }

    // All hops were trusted → fall back to the leftmost claimed client.
    if let Some(first) = chain.first() {
        return *first;
    }

    // No headers, no connect-info: unspecified sentinel. This path should be
    // unreachable in production (ConnectInfo is always set by the TCP-listener
    // entry point) but returning `0.0.0.0` is safer than panicking.
    IpAddr::from([0_u8, 0, 0, 0])
}

/// Parse the `X-Forwarded-For` header into its left-to-right list of
/// addresses, silently dropping malformed entries.
fn parse_xff_header(headers: &HeaderMap) -> Vec<IpAddr> {
    let Some(value) = headers.get("x-forwarded-for") else {
        return Vec::new();
    };
    let Ok(s) = value.to_str() else {
        return Vec::new();
    };
    s.split(',')
        .filter_map(|hop| hop.trim().parse::<IpAddr>().ok())
        .collect()
}

/// `true` when `addr` is contained within any CIDR in `cidrs`.
fn ip_in_any_cidr(addr: IpAddr, cidrs: &[IpNet]) -> bool {
    cidrs.iter().any(|net| net.contains(&addr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_xff(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            "x-forwarded-for",
            HeaderValue::from_str(value).expect("valid header"),
        );
        h
    }

    #[test]
    fn parse_xff_drops_malformed_entries() {
        let h = headers_with_xff("203.0.113.5, not-an-ip, 198.51.100.7");
        let parsed = parse_xff_header(&h);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].to_string(), "203.0.113.5");
        assert_eq!(parsed[1].to_string(), "198.51.100.7");
    }

    #[test]
    fn parse_xff_returns_empty_on_missing_header() {
        let h = HeaderMap::new();
        assert!(parse_xff_header(&h).is_empty());
    }

    #[test]
    fn default_argon2_permits_respects_override() {
        assert_eq!(default_argon2_permits(Some(4)), 4);
    }

    #[test]
    fn default_argon2_permits_ignores_zero_override() {
        // zero is rejected by Settings::validate; this is belt-and-braces.
        let permits = default_argon2_permits(Some(0));
        assert!((2..=16).contains(&permits));
    }

    #[test]
    fn default_argon2_permits_clamps_override_to_16() {
        assert_eq!(default_argon2_permits(Some(1_000)), 16);
    }

    #[test]
    fn default_argon2_permits_has_min_2_max_16() {
        let permits = default_argon2_permits(None);
        assert!(
            (2..=16).contains(&permits),
            "permits must be in [2, 16], got {permits}"
        );
    }

    #[test]
    fn ip_in_any_cidr_matches_subnet() {
        let cidrs = vec!["10.0.0.0/8".parse::<IpNet>().unwrap()];
        assert!(ip_in_any_cidr("10.1.2.3".parse().unwrap(), &cidrs));
        assert!(!ip_in_any_cidr("11.0.0.1".parse().unwrap(), &cidrs));
    }
}
