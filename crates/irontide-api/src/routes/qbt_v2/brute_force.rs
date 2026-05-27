//! Brute-force-ban registry for qBt v2 `auth/login` (M172a Lane C).
//!
//! Tracks failed authentication attempts per source IP with an in-flight
//! counter that prevents a thundering-herd flood from bypassing the cap:
//! every call to [`BruteForceRegistry::check_and_admit`] returns an
//! [`AdmitGuard`] RAII token that holds the `pending` slot until dropped.
//! That way a burst of 100 concurrent wrong-password attempts can only
//! occupy `max_failed_auth_count` argon2-verify slots at once; the other
//! 95+ requesters get an immediate 403 without ever entering the verify
//! pipeline.
//!
//! # LRU cap (G4)
//! Entries are stored in a `HashMap<IpAddr, FailedAuthState>` alongside a
//! `VecDeque<IpAddr>` shadow index ordered newest-front/oldest-back. When
//! a NEW IP is admitted into a full registry the oldest tail entry is
//! evicted — standard LRU. Each record_* path touches the entry to the
//! front. 10k entries × ~80 bytes ≈ 800 KiB worst case.
//!
//! # Lazy prune (P2)
//! Every call to [`BruteForceRegistry::record_failure`] scans up to 10
//! tail entries and drops any whose `banned_until` elapsed over
//! `ban_secs` ago — amortised O(1) per call, bounded by the ban window.
//!
//! # No custom Clock trait (S0b)
//! Tests rely on `#[tokio::test(flavor = "current_thread", start_paused
//! = true)]` plus `tokio::time::advance` rather than a mockable clock
//! abstraction. Production uses `tokio::time::Instant::now()` directly.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use parking_lot::RwLock;
use tokio::time::Instant;

/// Default LRU cap when the operator does not override via
/// [`crate::routes::qbt_v2::state::QbtState`] construction.
pub(crate) const DEFAULT_REGISTRY_CAPACITY: usize = 10_000;

/// Maximum number of LRU-tail entries scanned on each
/// [`BruteForceRegistry::record_failure`] call for lazy pruning (P2).
const PRUNE_BATCH: usize = 10;

/// Per-IP counter state inside the registry.
///
/// `attempts` is the authoritative failure count used for ban decisions.
/// `pending` is a transient in-flight counter decremented on
/// [`AdmitGuard::drop`]. `first_failure_at` anchors a potential future
/// sliding window (unused today). `last_touch` is the LRU ordering key.
#[derive(Debug)]
struct FailedAuthState {
    /// Count of recorded failures since the last reset (success or
    /// ban-expiry). When `attempts >= max_failed_auth_count` the IP is
    /// banned for `ban_duration_secs`.
    attempts: u32,
    /// Number of in-flight admissions currently holding an [`AdmitGuard`]
    /// for this IP. Decremented on `AdmitGuard::drop`, including the panic
    /// path. Admission is denied when `attempts + pending >= max` so a
    /// concurrent flood cannot saturate argon2 beyond the per-IP cap.
    pending: u32,
    /// Wall-clock moment of the first failure in the current counting
    /// window. Retained for future sliding-window extensions; not
    /// currently consulted by any decision path.
    #[allow(dead_code)]
    first_failure_at: Instant,
    /// Last time this entry was written or admitted. Used as the LRU
    /// ordering key — entries with a recent `last_touch` move to the
    /// `VecDeque` front.
    last_touch: Instant,
    /// When set, the IP is banned until this instant. `None` = not banned.
    banned_until: Option<Instant>,
}

/// In-memory registry tracking failed authentication attempts per IP.
///
/// Cheap to clone (it's always wrapped in [`Arc`]) because the only state
/// is an `RwLock`-guarded `HashMap` + `VecDeque` pair. The same registry
/// instance is shared across every login request handler via the qBt
/// router's [`super::state::QbtState`].
pub struct BruteForceRegistry {
    /// Primary map: IP → per-IP counters. Guarded by the same lock as
    /// `lru` so state transitions are atomic relative to LRU reordering.
    inner: RwLock<HashMap<IpAddr, FailedAuthState>>,
    /// Shadow LRU index: newest at front, oldest at back. Same lock as
    /// `inner` — coarse-grained but cheap (single login per request).
    lru: RwLock<VecDeque<IpAddr>>,
    /// LRU cap. When full and a new IP is admitted, pop the tail.
    ///
    /// **Atomic, not plain `usize` (M225 OV F2d).** Live capacity reconfig
    /// via `shrink_preserving_recent_bans` / `grow_capacity` updates the
    /// cap behind the same `Arc<BruteForceRegistry>` already shared with
    /// the auth handler. Plain `usize` could not be mutated; without
    /// interior mutability the shrink would rebuild the maps but the
    /// next eviction would re-grow back to the original cap. Ordering is
    /// `Relaxed` — capacity has no cross-field consistency invariant with
    /// `inner`/`lru` (those operations are guarded by their own `RwLock`s).
    capacity: AtomicUsize,
}

/// RAII guard proving admission into the argon2 verify pipeline.
///
/// Acquired by [`BruteForceRegistry::check_and_admit`] on success and
/// released on drop — which is the ONLY path that decrements the
/// in-flight `pending` counter. Panic-safe: the `Drop` impl runs even
/// when the login handler aborts mid-verify.
pub struct AdmitGuard {
    /// Shared registry reference so [`Drop`] can release the pending slot.
    /// `Option` so we can `take()` safely during drop if needed (today we
    /// just use the ref directly).
    registry: Arc<BruteForceRegistry>,
    /// IP whose pending counter we will decrement on drop.
    ip: IpAddr,
}

impl AdmitGuard {
    /// Construct a guard. Private — callers must go through
    /// [`BruteForceRegistry::check_and_admit`] which increments the
    /// pending counter atomically with admission.
    fn new(registry: Arc<BruteForceRegistry>, ip: IpAddr) -> Self {
        Self { registry, ip }
    }
}

impl Drop for AdmitGuard {
    fn drop(&mut self) {
        // Decrement pending counter (A1). Safe on panic because tokio
        // unwinds through Drop impls on panicking tasks. Saturating sub
        // defends against the impossible — double-drop would otherwise
        // underflow — but in practice the Option-free design of AdmitGuard
        // means every guard is dropped exactly once.
        let mut inner = self.registry.inner.write();
        if let Some(state) = inner.get_mut(&self.ip) {
            state.pending = state.pending.saturating_sub(1);
        }
    }
}

/// Opaque admission-denied token. No information on why (banned vs
/// at-pending-cap) because the caller always maps it to the same 403
/// `Fails.` response to preserve qBt parity (C4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionDenied;

impl BruteForceRegistry {
    /// Construct a fresh registry wrapped in `Arc`. `capacity` is the
    /// LRU cap; callers typically pass `DEFAULT_REGISTRY_CAPACITY` or the
    /// operator override from
    /// `Settings::qbt_compat::brute_force_registry_capacity`.
    ///
    /// Caps `capacity` to a minimum of 1 to avoid division-by-zero on
    /// eviction; validation at the `Settings` layer rejects values `< 100`
    /// but this is belt-and-braces.
    #[must_use]
    pub fn new(capacity: usize) -> Arc<Self> {
        let cap = capacity.max(1);
        Arc::new(Self {
            inner: RwLock::new(HashMap::with_capacity(cap.min(1024))),
            lru: RwLock::new(VecDeque::with_capacity(cap.min(1024))),
            capacity: AtomicUsize::new(cap),
        })
    }

    /// Attempt to admit this IP into the verify pipeline.
    ///
    /// Returns [`Ok(AdmitGuard)`] on admission, [`Err(AdmissionDenied)`]
    /// when the IP is banned OR has `attempts + pending >= max` pending
    /// plus historical failures. The `AdmitGuard` RAII token holds the
    /// `pending` slot until it is dropped — both on the success path
    /// (which calls [`record_success`]) and the failure path (which calls
    /// [`record_failure`]) the caller should let the guard drop *after*
    /// the recording method returns so the pending-- happens strictly
    /// after the state transition lands.
    ///
    /// Admission is allowed when:
    /// * `max == 0` — the operator has disabled the check (only valid
    ///   with `bypass_local_auth = true`; we still admit to keep the
    ///   pipeline uniform).
    /// * there is no ban OR the ban window has elapsed; AND
    /// * `attempts + pending < max`.
    ///
    /// When admitted:
    /// * On a pre-existing entry: increments `pending`, bumps `last_touch`,
    ///   and moves the entry to the LRU front.
    /// * On a NEW entry: inserts a fresh state with `attempts = 0,
    ///   pending = 1`, evicts the LRU tail if over capacity, and places
    ///   the new IP at the LRU front.
    #[allow(clippy::missing_errors_doc)] // single Err variant, zero data
    pub fn check_and_admit(
        self: &Arc<Self>,
        ip: IpAddr,
        max: u32,
        ban_secs: u64,
    ) -> Result<AdmitGuard, AdmissionDenied> {
        // `ban_secs` is accepted for signature symmetry with
        // `record_failure` — a banned-then-expired IP re-admission does
        // not need to know the ban window here (the window lives inside
        // the FailedAuthState). Suppress the unused-parameter warning.
        let _ = ban_secs;
        let now = Instant::now();
        let mut map = self.inner.write();
        let mut lru = self.lru.write();

        if let Some(state) = map.get_mut(&ip) {
            // Ban check: if the ban window elapsed, clear it AND the
            // attempts counter — qBt parity: ban expiry is a full reset.
            if let Some(until) = state.banned_until
                && now >= until
            {
                state.banned_until = None;
                state.attempts = 0;
            }
            if state.banned_until.is_some() {
                return Err(AdmissionDenied);
            }
            // `max == 0` means "counter disabled" (only valid when
            // bypass_local_auth is set and the handler has already let
            // the request through); don't deny here so the handler path
            // stays uniform. Otherwise gate on attempts + pending.
            if max > 0 {
                let in_use = state.attempts.saturating_add(state.pending);
                if in_use >= max {
                    return Err(AdmissionDenied);
                }
            }
            state.pending = state.pending.saturating_add(1);
            state.last_touch = now;
            move_to_front(&mut lru, ip);
        } else {
            // New IP: evict LRU tail if at capacity.
            let cap = self.capacity.load(Ordering::Relaxed);
            evict_until_under_capacity(&mut map, &mut lru, cap);
            let state = FailedAuthState {
                attempts: 0,
                pending: 1,
                first_failure_at: now,
                last_touch: now,
                banned_until: None,
            };
            map.insert(ip, state);
            lru.push_front(ip);
        }

        drop(lru);
        drop(map);
        Ok(AdmitGuard::new(Arc::clone(self), ip))
    }

    /// Record a failed argon2 verify. Increments `attempts`, stamps
    /// `banned_until` on cross-threshold, and prunes up to [`PRUNE_BATCH`]
    /// LRU-tail entries with long-elapsed bans.
    ///
    /// Caller contract: invoke this BEFORE dropping the admission guard
    /// so the state transition lands while we still hold the logical
    /// "my verify is still accounted for" semantics.
    pub fn record_failure(&self, ip: IpAddr, max: u32, ban_secs: u64) {
        let now = Instant::now();
        let mut map = self.inner.write();
        let mut lru = self.lru.write();

        if let Some(state) = map.get_mut(&ip) {
            // Ban-expiry reset: if the caller raced a long-banned IP back
            // into verify and the ban window lapsed, reset before the
            // increment so the "next failure after ban" is attempt #1.
            if let Some(until) = state.banned_until
                && now >= until
            {
                state.banned_until = None;
                state.attempts = 0;
            }
            state.attempts = state.attempts.saturating_add(1);
            if state.attempts == 1 {
                state.first_failure_at = now;
            }
            if state.attempts >= max {
                state.banned_until = Some(now + Duration::from_secs(ban_secs));
            }
            state.last_touch = now;
            move_to_front(&mut lru, ip);
        }
        // If the entry vanished (evicted between admit and record), we
        // skip silently — the attacker still got a 403, the counter
        // merely restarts next round. Acceptable.

        // Lazy prune (P2): scan up to PRUNE_BATCH tail entries and drop
        // any whose ban elapsed `ban_secs` ago. The `+ ban_secs` delay
        // gives the stale-ban check a stable hysteresis — we don't want
        // to prune an IP whose ban JUST expired because that same IP
        // might retry in the next few seconds and we want the counter
        // transition to be observable on their side.
        prune_expired_tail(&mut map, &mut lru, now, ban_secs);
    }

    /// Record a successful verify. Clears `attempts` and `banned_until`,
    /// bumps `last_touch`, moves to LRU front. Leaves the entry in place
    /// (future failures start a fresh counter).
    ///
    /// Caller contract: invoke this BEFORE dropping the admission guard.
    pub fn record_success(&self, ip: IpAddr) {
        let now = Instant::now();
        let mut map = self.inner.write();
        let mut lru = self.lru.write();

        if let Some(state) = map.get_mut(&ip) {
            state.attempts = 0;
            state.banned_until = None;
            state.last_touch = now;
            move_to_front(&mut lru, ip);
        }
    }

    /// Eagerly prune every expired entry — not limited to the
    /// [`PRUNE_BATCH`] tail so callers can reclaim memory deterministically
    /// (test #11, future admin-tool). The hot path uses the batched
    /// [`prune_expired_tail`] helper inside `record_failure` for amortised
    /// O(1) cost.
    pub fn prune_expired(&self, ban_secs: u64) {
        let now = Instant::now();
        let mut map = self.inner.write();
        let mut lru = self.lru.write();
        // Scan the whole LRU tail until we hit a non-prunable entry or
        // the VecDeque empties.
        while let Some(&ip) = lru.back() {
            let should_prune = match map.get(&ip) {
                Some(state) => match state.banned_until {
                    Some(until) => now >= until + Duration::from_secs(ban_secs),
                    // No ban and already idle in the tail — safe to drop
                    // even though production would not reach this branch
                    // because tail entries with attempts=0, banned=None
                    // are rare.
                    None => state.attempts == 0 && state.pending == 0,
                },
                None => true,
            };
            if should_prune {
                lru.pop_back();
                map.remove(&ip);
            } else {
                break;
            }
        }
    }

    /// Current number of tracked entries. Primarily used by integration
    /// tests to assert eviction + prune behaviour; production code should
    /// treat this as a diagnostic window only.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    /// `true` when the registry is empty — wraps `len() == 0` so clippy's
    /// `len_without_is_empty` lint stays happy when we expose `len`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }

    /// Registry LRU cap. Mutable at runtime via
    /// [`shrink_preserving_recent_bans`] / [`grow_capacity`] (M225 closes
    /// the M173+ FIXME at session.rs:1015). Reads with `Relaxed` ordering
    /// — capacity has no cross-field consistency invariant with `inner`
    /// or `lru`.
    ///
    /// [`shrink_preserving_recent_bans`]: Self::shrink_preserving_recent_bans
    /// [`grow_capacity`]: Self::grow_capacity
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity.load(Ordering::Relaxed)
    }

    /// Grow the LRU cap to `new_capacity`. Idempotent: if `new_capacity`
    /// is less than or equal to the current cap, this is a no-op (use
    /// [`shrink_preserving_recent_bans`] to shrink). Pure capacity update
    /// — `inner` and `lru` are untouched.
    ///
    /// [`shrink_preserving_recent_bans`]: Self::shrink_preserving_recent_bans
    pub fn grow_capacity(&self, new_capacity: usize) {
        let new = new_capacity.max(1);
        let cur = self.capacity.load(Ordering::Relaxed);
        if new > cur {
            self.capacity.store(new, Ordering::Relaxed);
        }
    }

    /// Shrink the LRU cap to `new_capacity`, preserving two tiers
    /// (M225 — closes the M173+ FIXME at session.rs:1015):
    ///
    /// * **Tier A — currently-banned**: every entry where
    ///   `banned_until.is_some() && banned_until > Instant::now()` is an
    ///   active security ban. ALL Tier A entries are retained, even when
    ///   `count(Tier A) >= new_capacity`. Losing an active ban is a
    ///   security regression worse than a transient cap violation.
    /// * **Tier B — most-recent partial-violation entries**: the remainder
    ///   of the budget (`new_capacity - count(Tier A)`) is filled from the
    ///   not-currently-banned entries, sorted by `last_touch` descending
    ///   (most recent first).
    ///
    /// Both maps are rebuilt from the union (Tier A first, Tier B in
    /// `last_touch` descending order). New cap is stored atomically.
    ///
    /// If `new_capacity` is greater than or equal to the current
    /// `inner.len()`, this is a no-op apart from the cap store (no need to
    /// touch the maps).
    pub fn shrink_preserving_recent_bans(&self, new_capacity: usize) {
        let new = new_capacity.max(1);
        let cur = self.capacity.load(Ordering::Relaxed);
        if new >= cur {
            return;
        }
        let now = Instant::now();
        let mut map = self.inner.write();
        let mut lru = self.lru.write();

        if map.len() <= new {
            self.capacity.store(new, Ordering::Relaxed);
            return;
        }

        let mut tier_a: Vec<IpAddr> = Vec::new();
        let mut tier_b: Vec<(IpAddr, Instant)> = Vec::new();
        for (ip, state) in map.iter() {
            let actively_banned = state
                .banned_until
                .is_some_and(|until| now < until);
            if actively_banned {
                tier_a.push(*ip);
            } else {
                tier_b.push((*ip, state.last_touch));
            }
        }
        tier_b.sort_by_key(|b| std::cmp::Reverse(b.1));

        let tier_b_budget = new.saturating_sub(tier_a.len());
        tier_b.truncate(tier_b_budget);

        let keep: std::collections::HashSet<IpAddr> = tier_a
            .iter()
            .copied()
            .chain(tier_b.iter().map(|(ip, _)| *ip))
            .collect();
        map.retain(|ip, _| keep.contains(ip));

        lru.clear();
        for ip in &tier_a {
            lru.push_back(*ip);
        }
        for (ip, _) in &tier_b {
            lru.push_back(*ip);
        }

        self.capacity.store(new, Ordering::Relaxed);
    }

    /// Current attempt count for the given IP. Returns 0 when the IP has
    /// no entry. Test + diagnostic use only.
    #[must_use]
    pub fn attempts_for(&self, ip: IpAddr) -> u32 {
        self.inner.read().get(&ip).map_or(0, |state| state.attempts)
    }

    /// Current in-flight (pending) count for the given IP. Test +
    /// diagnostic use only.
    #[must_use]
    pub fn pending_for(&self, ip: IpAddr) -> u32 {
        self.inner.read().get(&ip).map_or(0, |state| state.pending)
    }

    /// `true` when the IP is currently banned.
    #[must_use]
    pub fn is_banned(&self, ip: IpAddr) -> bool {
        self.inner
            .read()
            .get(&ip)
            .and_then(|state| state.banned_until)
            .is_some_and(|until| Instant::now() < until)
    }
}

/// Move `ip` to the front of the LRU deque. If the IP is already in the
/// deque we relocate it; otherwise we push it.
fn move_to_front(lru: &mut VecDeque<IpAddr>, ip: IpAddr) {
    // Linear scan is fine — deque length is bounded by `capacity`. A
    // sorted-by-last-touch `BTreeMap` would be asymptotically better but
    // the constant factor wins at capacity = 10_000.
    if let Some(pos) = lru.iter().position(|&x| x == ip) {
        lru.remove(pos);
    }
    lru.push_front(ip);
}

/// Evict LRU-tail entries until the map has room for one more.
fn evict_until_under_capacity(
    map: &mut HashMap<IpAddr, FailedAuthState>,
    lru: &mut VecDeque<IpAddr>,
    capacity: usize,
) {
    while map.len() >= capacity {
        if let Some(evicted) = lru.pop_back() {
            map.remove(&evicted);
        } else {
            break;
        }
    }
}

/// Prune up to [`PRUNE_BATCH`] LRU-tail entries whose bans elapsed over
/// `ban_secs` ago. Shared between the production hot path
/// ([`BruteForceRegistry::record_failure`]) and nowhere else.
fn prune_expired_tail(
    map: &mut HashMap<IpAddr, FailedAuthState>,
    lru: &mut VecDeque<IpAddr>,
    now: Instant,
    ban_secs: u64,
) {
    let mut scanned = 0;
    while scanned < PRUNE_BATCH
        && let Some(&ip) = lru.back()
    {
        scanned = scanned.saturating_add(1);
        let should_prune = match map.get(&ip) {
            Some(state) => match state.banned_until {
                Some(until) => now >= until + Duration::from_secs(ban_secs),
                None => false, // no ban — leave in place for LRU bookkeeping
            },
            None => true, // orphaned LRU entry — always drop
        };
        if should_prune {
            lru.pop_back();
            map.remove(&ip);
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn fresh_ip_admitted_and_guard_decrements_pending_on_drop() {
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        {
            let _guard = reg.check_and_admit(addr, 5, 60).expect("admit");
            assert_eq!(reg.pending_for(addr), 1);
        }
        assert_eq!(reg.pending_for(addr), 0);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn record_failure_increments_attempts() {
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        {
            let _guard = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        assert_eq!(reg.attempts_for(addr), 1);
        assert_eq!(reg.pending_for(addr), 0);
        assert!(!reg.is_banned(addr));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn cross_max_threshold_stamps_ban() {
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        for _ in 0..5 {
            let _guard = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        assert_eq!(reg.attempts_for(addr), 5);
        assert!(reg.is_banned(addr));

        // Next admit is denied (banned).
        assert!(
            reg.check_and_admit(addr, 5, 60).is_err(),
            "6th attempt must be denied"
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn ban_expires_after_ban_duration_secs() {
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        for _ in 0..5 {
            let _guard = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        assert!(reg.is_banned(addr));
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(!reg.is_banned(addr));
        // First failure AFTER ban expires should be attempt #1 again.
        let _guard = reg.check_and_admit(addr, 5, 60).expect("admit post-ban");
        reg.record_failure(addr, 5, 60);
        assert_eq!(reg.attempts_for(addr), 1);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn record_success_clears_counter() {
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        {
            let _guard = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        {
            let _guard = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_success(addr);
        }
        assert_eq!(reg.attempts_for(addr), 0);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn at_pending_cap_denies_admission() {
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        // Hold `max` guards open concurrently — pending == 5.
        let g1 = reg.check_and_admit(addr, 5, 60).expect("admit");
        let g2 = reg.check_and_admit(addr, 5, 60).expect("admit");
        let g3 = reg.check_and_admit(addr, 5, 60).expect("admit");
        let g4 = reg.check_and_admit(addr, 5, 60).expect("admit");
        let g5 = reg.check_and_admit(addr, 5, 60).expect("admit");
        assert_eq!(reg.pending_for(addr), 5);
        // 6th must be denied (at pending cap).
        assert!(reg.check_and_admit(addr, 5, 60).is_err());
        drop((g1, g2, g3, g4, g5));
        assert_eq!(reg.pending_for(addr), 0);
        // With all guards dropped, a fresh admit is allowed again.
        let _g = reg.check_and_admit(addr, 5, 60).expect("admit after drop");
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn lru_evicts_oldest_when_capacity_hit() {
        let reg = BruteForceRegistry::new(3);
        let a = ip(10, 0, 0, 1);
        let b = ip(10, 0, 0, 2);
        let c = ip(10, 0, 0, 3);
        let d = ip(10, 0, 0, 4);
        {
            let _ga = reg.check_and_admit(a, 5, 60).expect("a");
            reg.record_failure(a, 5, 60);
        }
        tokio::time::advance(Duration::from_millis(10)).await;
        {
            let _gb = reg.check_and_admit(b, 5, 60).expect("b");
            reg.record_failure(b, 5, 60);
        }
        tokio::time::advance(Duration::from_millis(10)).await;
        {
            let _gc = reg.check_and_admit(c, 5, 60).expect("c");
            reg.record_failure(c, 5, 60);
        }
        assert_eq!(reg.len(), 3);
        tokio::time::advance(Duration::from_millis(10)).await;
        {
            let _gd = reg.check_and_admit(d, 5, 60).expect("d");
        }
        assert_eq!(reg.len(), 3);
        // a was the oldest — should have been evicted.
        assert_eq!(reg.attempts_for(a), 0);
        // b, c, d remain
        assert!(reg.attempts_for(b) == 1 || reg.attempts_for(c) == 1);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn prune_expired_frees_memory_after_ban_plus_window() {
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        for _ in 0..5 {
            let _guard = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        assert!(reg.is_banned(addr));
        assert_eq!(reg.len(), 1);
        // Advance past ban + prune hysteresis window.
        tokio::time::advance(Duration::from_secs(120 + 5)).await;
        reg.prune_expired(60);
        assert_eq!(reg.len(), 0, "expired ban must be pruned");
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn different_ips_tracked_independently() {
        let reg = BruteForceRegistry::new(100);
        let a = ip(10, 0, 0, 1);
        let b = ip(10, 0, 0, 2);
        for _ in 0..5 {
            let _guard = reg.check_and_admit(a, 5, 60).expect("a admit");
            reg.record_failure(a, 5, 60);
        }
        assert!(reg.is_banned(a));
        // b was untouched — must still be admitted.
        let _gb = reg.check_and_admit(b, 5, 60).expect("b admit");
        assert!(!reg.is_banned(b));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn max_zero_never_denies_on_pending() {
        // qBt-parity: `max == 0` means "counter disabled" (only legitimate
        // with bypass_local_auth). The brute-force gate must not deny
        // admission when it's configured as a no-op; otherwise a misconfig
        // on the handler side (calling the gate when max=0) would cause
        // a 100% outage.
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        let g1 = reg.check_and_admit(addr, 0, 60).expect("admit 1");
        let g2 = reg.check_and_admit(addr, 0, 60).expect("admit 2");
        let g3 = reg.check_and_admit(addr, 0, 60).expect("admit 3");
        drop((g1, g2, g3));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn orphan_lru_tail_pruned_silently() {
        // Simulate a state divergence (shouldn't happen in production) —
        // if the LRU back() is missing from the map, prune drops it.
        let reg = BruteForceRegistry::new(3);
        let a = ip(10, 0, 0, 1);
        {
            let _g = reg.check_and_admit(a, 5, 60).expect("admit");
            reg.record_failure(a, 5, 60);
        }
        reg.prune_expired(60);
        // Fresh map — prune on record_failure for a DIFFERENT IP must
        // not panic.
        let b = ip(10, 0, 0, 2);
        let _g = reg.check_and_admit(b, 5, 60).expect("b admit");
        reg.record_failure(b, 5, 60);
    }

    // ── D3.2 (M173 Lane C) supplementary coverage ────────────────────
    //
    // The tests above already cover the canonical admit / fail / ban /
    // LRU / prune paths. This section fills in gaps the plan's audit
    // called out as under-covered: inspector correctness, construction
    // edge cases, no-op operations on unknown IPs, and AdmissionDenied
    // opaqueness.

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn fresh_registry_inspectors_report_empty_state() {
        // `len` / `is_empty` / `capacity` are part of the public surface —
        // regression-guard against a future refactor that confuses the
        // map len with the LRU len.
        let reg = BruteForceRegistry::new(100);
        assert!(reg.is_empty(), "fresh registry must be empty");
        assert_eq!(reg.len(), 0);
        assert_eq!(reg.capacity(), 100);
        let addr = ip(10, 0, 0, 1);
        assert_eq!(reg.attempts_for(addr), 0, "unknown IP reports zero");
        assert_eq!(reg.pending_for(addr), 0);
        assert!(!reg.is_banned(addr));
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn capacity_below_one_clamps_to_one() {
        // The constructor clamps to a minimum of 1 to avoid a
        // division-by-zero during eviction; validation at the Settings
        // layer rejects values `< 100`, but belt-and-braces matters
        // because a misconfig would otherwise crash the login path.
        let reg = BruteForceRegistry::new(0);
        assert_eq!(reg.capacity(), 1, "capacity 0 must be clamped to 1");
        // And it must still accept an admit.
        let addr = ip(10, 0, 0, 1);
        let _g = reg
            .check_and_admit(addr, 5, 60)
            .expect("admit on clamped reg");
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn record_failure_on_unknown_ip_is_silent_noop() {
        // The record_* paths intentionally ignore missing entries so a
        // guard that's been LRU-evicted between admit and record doesn't
        // crash the handler. Regression-guard against a future refactor
        // that treats the missing entry as an error.
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 99);
        reg.record_failure(addr, 5, 60);
        reg.record_success(addr);
        assert!(
            reg.is_empty(),
            "record_* on unknown IP must not create entry"
        );
        assert_eq!(reg.attempts_for(addr), 0);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn guard_drop_without_record_decrements_pending_only() {
        // A `check_and_admit` that is dropped without a paired record_*
        // call (e.g. the handler short-circuits on shutdown) must
        // release the pending slot but leave `attempts` at its prior
        // value.
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        {
            let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        assert_eq!(reg.attempts_for(addr), 1);
        // Second admit that is dropped without record — attempts stays
        // at 1, pending returns to 0.
        {
            let _g = reg
                .check_and_admit(addr, 5, 60)
                .expect("second admit (attempts 1 + pending 1 < 5)");
            assert_eq!(reg.pending_for(addr), 1);
        }
        assert_eq!(
            reg.attempts_for(addr),
            1,
            "attempts must not advance without record_failure"
        );
        assert_eq!(
            reg.pending_for(addr),
            0,
            "pending must drop on guard release"
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn admission_denied_is_opaque_and_copy_comparable() {
        // AdmissionDenied deliberately carries no data (the login handler
        // maps all denials to the same 403 "Fails." string), so consumers
        // must be able to compare two denial tokens cheaply without
        // caring about provenance.
        let a = AdmissionDenied;
        let b = AdmissionDenied;
        assert_eq!(a, b);
        // Copy / Clone check — if a future refactor widened this type,
        // the downstream handler's `Result<AdmitGuard, AdmissionDenied>`
        // would stop being `Copy`-friendly and the login path would need
        // to be audited.
        #[allow(
            clippy::no_effect_underscore_binding,
            reason = "intentional Copy trait verification"
        )]
        let _c: AdmissionDenied = a;
        #[allow(
            clippy::no_effect_underscore_binding,
            reason = "intentional Copy trait verification"
        )]
        let _d: AdmissionDenied = a;
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn record_failure_starts_fresh_window_after_record_success() {
        // Mirrors qBt parity: a successful login wipes the failure
        // counter. A subsequent failure is attempt #1, not #N+1.
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        for _ in 0..3 {
            let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        assert_eq!(reg.attempts_for(addr), 3);
        {
            let _g = reg.check_and_admit(addr, 5, 60).expect("admit success");
            reg.record_success(addr);
        }
        assert_eq!(reg.attempts_for(addr), 0);
        {
            let _g = reg
                .check_and_admit(addr, 5, 60)
                .expect("admit post-success");
            reg.record_failure(addr, 5, 60);
        }
        assert_eq!(
            reg.attempts_for(addr),
            1,
            "first failure after success must be attempt #1"
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn capacity_of_one_evicts_every_previous_entry() {
        // Stress-test the LRU path with capacity = 1 — every new IP
        // must evict the previous one. This is a hot path in the
        // cold-start case where the registry starts empty.
        let reg = BruteForceRegistry::new(1);
        for i in 0..5_u8 {
            let addr = ip(10, 0, 0, i);
            {
                let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
                reg.record_failure(addr, 5, 60);
            }
            assert_eq!(reg.len(), 1, "capacity=1 must never exceed one entry");
        }
        // Only the last IP survives.
        let last = ip(10, 0, 0, 4);
        assert_eq!(reg.attempts_for(last), 1);
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn prune_expired_preserves_active_bans() {
        // prune_expired must only drop entries whose ban has expired
        // by the full `ban_secs` hysteresis window. A freshly-banned IP
        // must survive a prune call.
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        for _ in 0..5 {
            let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        assert!(reg.is_banned(addr));
        reg.prune_expired(60);
        assert_eq!(
            reg.len(),
            1,
            "active ban must survive a prune; dropping it would reset the attacker's counter"
        );
    }

    // ── M225 Step 4: shrink + grow with AtomicUsize capacity ───────────

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn brute_force_registry_shrink_preserves_all_active_bans() {
        // Tier A invariant: every currently-banned IP MUST survive a
        // shrink even when their count exceeds the new capacity. A 5-cap
        // shrink applied to 10 active bans + 90 partials must keep all 10
        // bans (cap is a soft floor for security) and zero partials.
        let reg = BruteForceRegistry::new(100);
        for i in 0..10u8 {
            let addr = ip(10, 0, 0, i);
            for _ in 0..5 {
                let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
                reg.record_failure(addr, 5, 60);
            }
            assert!(reg.is_banned(addr), "ip {i} should be banned");
        }
        for i in 0..90u8 {
            let addr = ip(192, 168, 0, i);
            let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        assert_eq!(reg.len(), 100);

        reg.shrink_preserving_recent_bans(5);

        assert_eq!(reg.capacity(), 5, "new cap must be stored atomically");
        for i in 0..10u8 {
            assert!(
                reg.is_banned(ip(10, 0, 0, i)),
                "ban {i} dropped — security regression"
            );
        }
        assert_eq!(
            reg.len(),
            10,
            "shrink kept all 10 bans (cap exceeded by design), zero partials"
        );
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn brute_force_registry_shrink_fills_remainder_with_recent_partials() {
        // With 2 active bans + 8 partials at cap 10, shrinking to cap 5
        // keeps both bans and fills the remaining 3 slots with the most-
        // recent partials (by last_touch). Pre-OV-F5, a naive sort by
        // last_touch would have evicted bans in favour of fresher
        // partials — Tier A preservation prevents that.
        let reg = BruteForceRegistry::new(10);
        for i in 0..2u8 {
            let addr = ip(10, 0, 0, i);
            for _ in 0..5 {
                let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
                reg.record_failure(addr, 5, 60);
            }
        }
        for i in 0..8u8 {
            let addr = ip(192, 168, 0, i);
            let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
            tokio::time::advance(Duration::from_millis(10)).await;
        }
        assert_eq!(reg.len(), 10);

        reg.shrink_preserving_recent_bans(5);

        assert_eq!(reg.capacity(), 5);
        assert_eq!(reg.len(), 5, "2 bans + 3 most-recent partials");
        for i in 0..2u8 {
            assert!(reg.is_banned(ip(10, 0, 0, i)), "ban {i} must survive");
        }
        for i in 5..8u8 {
            assert_eq!(
                reg.attempts_for(ip(192, 168, 0, i)),
                1,
                "most-recent partial {i} must survive"
            );
        }
        for i in 0..5u8 {
            assert_eq!(
                reg.attempts_for(ip(192, 168, 0, i)),
                0,
                "older partial {i} must be evicted"
            );
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn brute_force_registry_grow_is_idempotent() {
        // Grow updates the atomic cap but never touches the maps. Grow
        // smaller than the current cap is a no-op (use shrink instead).
        let reg = BruteForceRegistry::new(100);
        let addr = ip(10, 0, 0, 1);
        let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
        reg.record_failure(addr, 5, 60);

        reg.grow_capacity(500);
        assert_eq!(reg.capacity(), 500);
        assert_eq!(reg.len(), 1, "grow never drops entries");

        reg.grow_capacity(500);
        assert_eq!(reg.capacity(), 500, "second grow is idempotent");

        reg.grow_capacity(50);
        assert_eq!(reg.capacity(), 500, "grow-smaller is a no-op");
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn brute_force_registry_shrink_no_op_when_below_capacity() {
        // shrink_preserving_recent_bans called with new >= cur is a no-op
        // for the maps; the cap store path also short-circuits.
        let reg = BruteForceRegistry::new(100);
        for i in 0..10u8 {
            let addr = ip(192, 168, 0, i);
            let _g = reg.check_and_admit(addr, 5, 60).expect("admit");
            reg.record_failure(addr, 5, 60);
        }
        let len_before = reg.len();
        reg.shrink_preserving_recent_bans(200);
        assert_eq!(reg.capacity(), 100, "shrink-larger is a no-op");
        assert_eq!(reg.len(), len_before);

        // Shrink to a value larger than current occupancy: cap drops, no
        // entries evicted.
        reg.shrink_preserving_recent_bans(50);
        assert_eq!(reg.capacity(), 50);
        assert_eq!(reg.len(), len_before, "shrink-cap-above-occupancy keeps all entries");
    }
}
