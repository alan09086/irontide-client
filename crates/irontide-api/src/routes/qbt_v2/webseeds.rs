//! qBt v2 `GET /api/v2/torrents/webseeds?hash=X` (M171 Lane B + M178 Lane C).
//!
//! Returns an array of objects — one per configured web-seed URL. BEP 19
//! (`url-list`) and BEP 17 (`httpseeds`) URLs are merged into a single
//! list, with BEP 19 entries first (mirrors the wire order in the
//! .torrent file).
//!
//! M178 (Lane C): rows are extended with optional per-source stats
//! sourced from the actor's [`irontide::core::WebSeedStats`] map. Stats
//! fields use `skip_serializing_if = "Option::is_none"` so legacy
//! consumers (Radarr-style URL-only) still parse the response, while
//! newer clients can render downloaded bytes, last error, and rate.
//!
//! The endpoint returns an empty array when a torrent has no web seeds.
//! A 404 is returned when the hash is unknown; the handler is silent
//! about "metadata not yet resolved" because magnet-only torrents
//! naturally produce an empty list until metadata arrives — matching
//! qBt's behaviour.

use std::collections::HashMap;

use axum::extract::{Query, State};
use irontide::core::Id20;
use irontide::session::{WebSeedState, WebSeedStats};
use serde::Serialize;

use super::response::{QbtError, QbtResponse};
use super::state::QbtState;
use super::torrents::HashQuery;

/// Single row in the qBt v2 webseeds response.
///
/// `url` is required and present in M171's URL-only schema. All other
/// fields are M178-introduced and use `skip_serializing_if = "Option::is_none"`
/// so legacy Radarr-style consumers see only the URL when stats haven't
/// accumulated, and the response stays bit-identical to M177 for torrents
/// that have not seen any web-seed activity.
#[derive(Debug, Clone, Serialize)]
pub struct QbtWebSeed {
    /// Web-seed URL (BEP 19 url-list or BEP 17 httpseeds entry).
    pub url: String,
    /// Cumulative bytes downloaded from this URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloaded: Option<u64>,
    /// Most recent observed error, persisting through recovery.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Rate at the most recent emission, computed over the throttle window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_rate: Option<u64>,
    /// Coarse state — `"idle"`, `"active"`, or `"errored"`. Lower-case to
    /// match qBt's tracker `status` strings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<&'static str>,
    /// Consecutive failures in the most recent failure run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consecutive_failures: Option<u32>,
    /// Unix timestamp of the most recent emission.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_attempt_unix_secs: Option<u64>,
    /// M186 forward-compat (D-eng-9). Always `None` in M178.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_unix_secs: Option<u64>,
}

fn state_label(s: WebSeedState) -> &'static str {
    match s {
        WebSeedState::Idle => "idle",
        WebSeedState::Active => "active",
        WebSeedState::Errored => "errored",
    }
}

/// Merge a URL with its (possibly absent) stats into a `QbtWebSeed` row.
fn build_row(url: String, stats: Option<&WebSeedStats>) -> QbtWebSeed {
    match stats {
        Some(s) => QbtWebSeed {
            url,
            downloaded: Some(s.downloaded_bytes),
            last_error: s.last_error.clone(),
            last_rate: Some(s.last_rate_bps),
            state: Some(state_label(s.state)),
            consecutive_failures: Some(s.consecutive_failures),
            last_attempt_unix_secs: Some(s.last_attempt_unix_secs),
            next_retry_unix_secs: s.next_retry_unix_secs,
        },
        None => QbtWebSeed {
            url,
            downloaded: None,
            last_error: None,
            last_rate: None,
            state: None,
            consecutive_failures: None,
            last_attempt_unix_secs: None,
            next_retry_unix_secs: None,
        },
    }
}

/// `GET /api/v2/torrents/webseeds?hash=X`.
///
/// # Errors
/// - `QbtError::BadRequest` if `hash` is not a 40-char hex string.
/// - `QbtError::NotFound` if the hash is unknown.
/// - `QbtError::Internal` on session/serialisation failures.
pub async fn list(
    State(state): State<QbtState>,
    Query(q): Query<HashQuery>,
) -> Result<QbtResponse, QbtError> {
    let id =
        Id20::from_hex(&q.hash).map_err(|e| QbtError::BadRequest(format!("invalid hash: {e}")))?;

    let urls = state
        .session
        .get_web_seeds(id)
        .await
        .map_err(|_| QbtError::NotFound)?;

    // Best-effort fetch of stats: if it fails, surface URL-only rows.
    let stats_by_url: HashMap<String, WebSeedStats> = state
        .session
        .web_seed_stats(id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|s| (s.url.clone(), s))
        .collect();

    let rows: Vec<QbtWebSeed> = urls
        .into_iter()
        .map(|u| {
            let stats = stats_by_url.get(&u);
            build_row(u, stats)
        })
        .collect();

    Ok(QbtResponse::Json(serde_json::to_value(&rows).map_err(
        |e| QbtError::Internal(format!("serialise: {e}")),
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated_stats() -> WebSeedStats {
        WebSeedStats {
            url: "http://seed.example/file".into(),
            state: WebSeedState::Active,
            downloaded_bytes: 4096,
            last_rate_bps: 256,
            last_error: Some("temporary 503".into()),
            consecutive_failures: 0,
            last_attempt_unix_secs: 1_700_000_000,
            next_retry_unix_secs: None,
        }
    }

    #[test]
    fn row_with_stats_serialises_all_fields() {
        let stats = populated_stats();
        let row = build_row(stats.url.clone(), Some(&stats));
        let v = serde_json::to_value(&row).expect("serialise");
        assert_eq!(v["url"], "http://seed.example/file");
        assert_eq!(v["downloaded"], 4096);
        assert_eq!(v["last_rate"], 256);
        assert_eq!(v["state"], "active");
        assert_eq!(v["last_error"], "temporary 503");
        assert_eq!(v["consecutive_failures"], 0);
        assert!(v.get("next_retry_unix_secs").is_none());
    }

    #[test]
    fn row_without_stats_is_url_only() {
        let row = build_row("http://noseed.example/file".into(), None);
        let v = serde_json::to_value(&row).expect("serialise");
        assert_eq!(v["url"], "http://noseed.example/file");
        assert_eq!(v.as_object().expect("object").len(), 1);
    }

    #[test]
    fn legacy_url_only_consumer_sees_url_field_unchanged() {
        // Radarr-style consumers parse `[{"url": "..."}]`. Even with stats
        // present, the `url` key is identical, so legacy parsing works.
        let stats = populated_stats();
        let row = build_row(stats.url.clone(), Some(&stats));
        let v = serde_json::to_value(&row).expect("serialise");
        let url = v.get("url").and_then(|x| x.as_str()).expect("url");
        assert_eq!(url, "http://seed.example/file");
    }

    #[test]
    fn errored_state_label_propagates() {
        let mut stats = populated_stats();
        stats.state = WebSeedState::Errored;
        stats.consecutive_failures = 5;
        let row = build_row(stats.url.clone(), Some(&stats));
        let v = serde_json::to_value(&row).expect("serialise");
        assert_eq!(v["state"], "errored");
        assert_eq!(v["consecutive_failures"], 5);
    }

    #[test]
    fn last_error_persists_visibly_through_active_state() {
        // D-eng-8: last_error PERSISTS through Errored→Active recovery.
        // The wire JSON must reflect that.
        let mut stats = populated_stats();
        stats.state = WebSeedState::Active;
        stats.last_error = Some("flaked recently".into());
        let row = build_row(stats.url.clone(), Some(&stats));
        let v = serde_json::to_value(&row).expect("serialise");
        assert_eq!(v["state"], "active");
        assert_eq!(v["last_error"], "flaked recently");
    }
}
