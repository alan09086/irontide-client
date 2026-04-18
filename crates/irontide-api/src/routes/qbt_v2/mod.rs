//! qBittorrent WebUI v2 compatibility layer (M168).
//!
//! Implements a subset of the qBt WebUI v2 HTTP API so that `*arr` clients
//! (Radarr, Sonarr, Prowlarr, Lidarr) can talk to IronTide as if it were
//! qBittorrent. Same pattern as CockroachDB speaking PostgreSQL wire protocol.
//!
//! # Surface
//! - `auth/login`, `auth/logout` — session cookie lifecycle
//! - `app/version`, `app/webapiVersion`, `app/buildInfo`, `app/preferences`
//! - `torrents/info`, `torrents/properties`, `torrents/add`
//! - `torrents/pause`, `resume`, `delete`, `recheck`, `reannounce`
//! - `torrents/categories` (`{}` shim)
//! - `transferInfo`
//!
//! # Opt-in
//! Disabled by default (`settings.qbt_compat.enabled = false`). When disabled,
//! every `/api/v2/*` route returns 404 via the `qbt_gate` middleware — the
//! route must appear non-existent, not 403, to minimise attack surface.
//!
//! # Deferred
//! Category CRUD, tag management, detail endpoints (`files`, `trackers`,
//! `webseeds`, `pieceStates`, etc.), `setPreferences`, and `shutdown` land
//! in M170. argon2 hashing + CSRF land in M171.

pub mod response;
pub mod session_store;

pub use response::{QbtError, QbtResponse};
pub use session_store::SessionStore;
