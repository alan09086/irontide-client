//! Shared state for the qBt v2 sub-router.
//!
//! Bundles the upstream `SessionHandle` with the in-memory `SessionStore`
//! that tracks authenticated cookies. This is the only place where the
//! qBt v2 surface intersects the main engine state.

use std::sync::Arc;

use irontide::session::SessionHandle;

use super::session_store::SessionStore;

/// Cheap-to-clone state for every qBt v2 handler and middleware.
#[derive(Clone)]
pub struct QbtState {
    pub session: Arc<SessionHandle>,
    pub store: Arc<SessionStore>,
}

impl QbtState {
    pub fn new(session: Arc<SessionHandle>, store: Arc<SessionStore>) -> Self {
        Self { session, store }
    }
}
