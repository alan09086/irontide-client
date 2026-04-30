//! In-memory session cookie store for the qBt v2 compatibility layer.
//!
//! Generates cryptographically secure 24-byte session tokens (base64 URL-safe,
//! 32 chars output) via `aws_lc_rs::rand::SystemRandom`. Enforces a 24-hour
//! TTL (configurable) with lazy expiry on lookup, and a 1024-session LRU cap
//! (configurable) so a login storm cannot exhaust memory.
//!
//! xorshift64 is insufficient here — its 64-bit seed gives only 64 bits of
//! real entropy regardless of token length. Session cookies gate authenticated
//! API access, so we need the full 192 bits.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use aws_lc_rs::rand::{SecureRandom, SystemRandom};
use base64::Engine;
use parking_lot::RwLock;

/// Length (bytes) of the raw token before base64 encoding.
const SID_RAW_LEN: usize = 24;

/// Error returned when the crypto-random source fails.
///
/// In practice `aws_lc_rs::rand::SystemRandom::fill` only fails if the OS's
/// `getrandom(2)` / `CryptGenRandom` returns an error — i.e. the kernel is
/// in an unusable state. Propagating it lets callers surface the failure
/// rather than panic silently inside request handling.
#[derive(Debug)]
pub struct RandomSourceError;

impl std::fmt::Display for RandomSourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OS random source failed")
    }
}

impl std::error::Error for RandomSourceError {}

/// Per-session data kept in the store.
#[derive(Clone, Debug)]
pub struct SessionData {
    pub username: String,
    pub created_at: Instant,
    pub last_used: Instant,
}

/// Thread-safe in-memory session store.
///
/// Tokens are stored in a `HashMap` for O(1) lookup, with a `VecDeque` shadow
/// index tracking insertion order so LRU eviction is also O(1). Both are
/// guarded by a single `parking_lot::RwLock` — cheap and `Send + Sync`.
pub struct SessionStore {
    inner: RwLock<Inner>,
    ttl: Duration,
    max_sessions: usize,
    rng: SystemRandom,
}

struct Inner {
    sessions: HashMap<String, SessionData>,
    /// Insertion order. When at `max_sessions`, we pop from the front.
    order: VecDeque<String>,
}

impl SessionStore {
    #[must_use] 
    pub fn new(ttl: Duration, max_sessions: usize) -> Self {
        let cap = max_sessions.max(1);
        Self {
            inner: RwLock::new(Inner {
                sessions: HashMap::with_capacity(cap.min(1024)),
                order: VecDeque::with_capacity(cap.min(1024)),
            }),
            ttl,
            max_sessions: cap,
            rng: SystemRandom::new(),
        }
    }

    /// Generate a cryptographically secure token (24 bytes → URL-safe base64).
    fn generate_sid(&self) -> Result<String, RandomSourceError> {
        let mut buf = [0u8; SID_RAW_LEN];
        self.rng.fill(&mut buf).map_err(|_| RandomSourceError)?;
        Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf))
    }

    /// Create a new session for the given username, returning the cookie token.
    ///
    /// Evicts the oldest session if the store is at capacity.
    ///
    /// # Errors
    ///
    /// Returns an error if the random source fails to generate a session ID.
    pub fn create(&self, username: impl Into<String>) -> Result<String, RandomSourceError> {
        let sid = self.generate_sid()?;
        let now = Instant::now();
        let data = SessionData {
            username: username.into(),
            created_at: now,
            last_used: now,
        };

        let mut inner = self.inner.write();
        while inner.order.len() >= self.max_sessions {
            if let Some(oldest) = inner.order.pop_front() {
                inner.sessions.remove(&oldest);
            } else {
                break;
            }
        }
        inner.sessions.insert(sid.clone(), data);
        inner.order.push_back(sid.clone());
        Ok(sid)
    }

    /// Look up a session by token. Returns `Some` iff the token exists AND is
    /// not expired; lazy-expires the entry on access.
    ///
    /// Also refreshes `last_used` so "active" sessions don't timeout at TTL
    /// boundaries (matches real qBt behaviour).
    pub fn validate(&self, sid: &str) -> Option<SessionData> {
        let now = Instant::now();
        let mut inner = self.inner.write();
        // Check existence first to avoid mutable borrow complexity on expiry.
        let expired = match inner.sessions.get(sid) {
            Some(d) => now.duration_since(d.created_at) > self.ttl,
            None => return None,
        };
        if expired {
            inner.sessions.remove(sid);
            inner.order.retain(|s| s != sid);
            return None;
        }
        if let Some(d) = inner.sessions.get_mut(sid) {
            d.last_used = now;
            return Some(d.clone());
        }
        None
    }

    /// Invalidate a session — called by `auth/logout`. Idempotent: returns
    /// silently if the token is unknown.
    pub fn invalidate(&self, sid: &str) {
        let mut inner = self.inner.write();
        inner.sessions.remove(sid);
        inner.order.retain(|s| s != sid);
    }

    /// Current session count (test/debug only).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.inner.read().sessions.len()
    }

    /// Whether the store has any active sessions (test/debug only).
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.inner.read().sessions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(max: usize) -> SessionStore {
        SessionStore::new(Duration::from_hours(1), max)
    }

    #[test]
    fn session_store_create_and_validate() {
        let s = store(16);
        let sid = s.create("alan").unwrap();
        let data = s.validate(&sid).expect("new session must validate");
        assert_eq!(data.username, "alan");
    }

    #[test]
    fn session_store_logout_invalidates() {
        let s = store(16);
        let sid = s.create("alan").unwrap();
        s.invalidate(&sid);
        assert!(s.validate(&sid).is_none());
    }

    #[test]
    fn session_store_expired_token_rejected() {
        // 0-second TTL so the session is immediately expired; we wait 1ms to
        // push `now` past `created_at` on even the fastest machines.
        let s = SessionStore::new(Duration::from_nanos(1), 16);
        let sid = s.create("alan").unwrap();
        std::thread::sleep(Duration::from_millis(1));
        assert!(s.validate(&sid).is_none());
    }

    #[test]
    fn session_store_lazy_expiry_updates_last_used() {
        let s = store(16);
        let sid = s.create("alan").unwrap();
        let first = s.validate(&sid).unwrap().last_used;
        std::thread::sleep(Duration::from_millis(2));
        let second = s.validate(&sid).unwrap().last_used;
        assert!(second > first, "last_used must advance on access");
    }

    #[test]
    #[allow(clippy::many_single_char_names, reason = "concise session IDs in test")]
    fn session_store_lru_evicts_oldest_when_full() {
        let s = store(3);
        let a = s.create("a").unwrap();
        let b = s.create("b").unwrap();
        let c = s.create("c").unwrap();
        assert_eq!(s.len(), 3);
        // 4th session must push out the oldest (a)
        let d = s.create("d").unwrap();
        assert_eq!(s.len(), 3);
        assert!(s.validate(&a).is_none(), "oldest must be evicted");
        assert!(s.validate(&b).is_some());
        assert!(s.validate(&c).is_some());
        assert!(s.validate(&d).is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn session_store_concurrent_create_and_validate() {
        use std::sync::Arc;

        let s = Arc::new(store(1024));
        let mut handles = Vec::new();
        for i in 0..32 {
            let s = Arc::clone(&s);
            handles.push(tokio::spawn(async move {
                let sid = s.create(format!("user-{i}")).unwrap();
                let data = s.validate(&sid).unwrap();
                assert_eq!(data.username, format!("user-{i}"));
                sid
            }));
        }
        let mut sids = Vec::new();
        for h in handles {
            sids.push(h.await.unwrap());
        }
        assert_eq!(sids.len(), 32);
        // All 32 tokens must be unique (no collisions from parallel generation).
        let mut uniq = std::collections::HashSet::new();
        for sid in &sids {
            assert!(uniq.insert(sid.clone()), "duplicate token generated");
        }
    }

    #[test]
    fn session_store_tokens_are_unpredictable_32_bytes() {
        let s = store(1024);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..128 {
            let sid = s.create("u").unwrap();
            // URL-safe base64 of 24 bytes without padding = 32 chars.
            assert_eq!(sid.len(), 32, "got: {sid}");
            // All chars must be URL-safe base64 alphabet (no +, /, or =).
            assert!(
                sid.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "non-base64-url char: {sid}"
            );
            assert!(seen.insert(sid.clone()), "duplicate token: {sid}");
        }
    }
}
