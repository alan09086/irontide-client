//! Typed error surface for the CLI control plane.
//!
//! `CliError` is the single error type returned from `client::ApiClient`,
//! the `commands` dispatch helpers, and (eventually) the top-level mode
//! runners. It lives in its own module so that every CLI subsystem can
//! depend on a single `thiserror` enum without importing client/progress.
//!
//! Exit code mapping (wired up in T5, not here):
//!
//! | variant               | exit code |
//! |-----------------------|-----------|
//! | (ok)                  | 0         |
//! | `HttpStatus`,         | 1         |
//! | `ParseFailed`,        | 1         |
//! | `WebSocket`,          | 1         |
//! | `NotFound`,           | 1         |
//! | `InvalidInput`,       | 1         |
//! | `Other`               | 1         |
//! | (user abort from T5) | 2         |
//! | `DaemonUnreachable`   | 3         |

/// Errors produced by the CLI control plane.
///
/// Every variant is constructed with enough context for the CLI top-level
/// formatter to produce an actionable message (which URL, which hash,
/// which operation) without needing to inspect a source-chain.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CliError {
    /// The daemon could not be reached at `url`.
    ///
    /// Produced by `ApiClient::ping` and by every HTTP method that fails
    /// before the daemon sends a response (DNS failure, connection
    /// refused, connect timeout, request timeout). Maps to exit code 3.
    #[error("daemon unreachable at {url}: {source}")]
    DaemonUnreachable {
        /// The base URL we attempted to contact.
        url: String,
        /// Underlying reqwest error.
        #[source]
        source: reqwest::Error,
    },

    /// The daemon returned a non-success HTTP status.
    ///
    /// `body` contains the raw response body (possibly empty, possibly
    /// a JSON error envelope) — the caller decides how to display it.
    #[error("HTTP {status}: {body}")]
    HttpStatus {
        /// HTTP status code.
        status: u16,
        /// Raw response body as UTF-8 (lossy).
        body: String,
    },

    /// The response body could not be deserialized.
    #[error("failed to parse response: {0}")]
    ParseFailed(#[from] serde_json::Error),

    /// A WebSocket subscription error occurred.
    #[error("websocket error: {0}")]
    WebSocket(String),

    /// The referenced torrent (full hash or prefix) could not be found.
    #[error("torrent not found: {0}")]
    NotFound(String),

    /// The user supplied invalid input (bad hash prefix, unknown filter, etc.).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Fallback for anything that doesn't fit the above variants.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl CliError {
    /// Suggested process exit code for this error.
    ///
    /// Kept here (not in `main.rs`) so that every caller agrees on the
    /// mapping and future modes can reuse it without re-implementing the
    /// table.
    #[allow(dead_code)] // used by T5 (batch mode)
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            Self::DaemonUnreachable { .. } => 3,
            _ => 1,
        }
    }
}
