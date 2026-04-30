//! M171: qBt v2 tags CRUD + per-torrent assignment.
//!
//! Routes:
//! * `GET  /api/v2/torrents/tags`       - list all known tags (JSON string array, sorted)
//! * `POST /api/v2/torrents/createTags` - form body `tags=a,b,c` or newline-separated, idempotent
//! * `POST /api/v2/torrents/deleteTags` - form body `tags=a,b`, idempotent
//! * `POST /api/v2/torrents/addTags`    - form body `hashes=X|Y&tags=a,b` (pipe-separated hashes)
//! * `POST /api/v2/torrents/removeTags` - form body `hashes=X|Y&tags=a,b`
//!
//! # qBt parity: lenient createTags
//! Real qBt does NOT return 409 or 400 for duplicate or invalid names - it
//! always returns 200. We mirror that at the HTTP layer by swallowing
//! `TagError::AlreadyExists` AND `TagError::InvalidName` (with a WARN log
//! for invalid). The domain API (`TagRegistry::create`) remains strict;
//! only the wire layer is lenient.
//!
//! # Hash batch parsing
//! `addTags` / `removeTags` accept pipe-separated info hashes (qBt's bulk
//! shape, mirrored in the M168/M170 pause / resume / delete endpoints).
//! Malformed hashes are WARN-logged and skipped individually so one bad
//! hash doesn't nuke the whole batch.

use axum::extract::State;
use irontide::core::Id20;
use irontide::session::TagError;
use serde::Deserialize;

use super::response::{QbtError, QbtResponse};
use super::state::QbtState;

/// Split a `tags=...` blob into trimmed, non-empty names.
///
/// qBt accepts either comma- or newline-delimited blobs; the *arr clients
/// mostly use commas, the `WebUI` uses newlines. We accept both (same as the
/// categories `removeCategories` splitter).
fn parse_tag_list(raw: &str) -> Vec<String> {
    raw.split([',', '\n'])
        .map(|s| s.trim_end_matches('\r').trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Split a `hashes=X|Y|Z` blob into trimmed, non-empty hex strings.
///
/// Does NOT validate hex shape here - individual hashes are parsed via
/// `Id20::from_hex` at dispatch time so we can log per-hash errors.
fn parse_hash_list(raw: &str) -> Vec<String> {
    raw.split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Form body shape for `createTags` / `deleteTags`.
///
/// Both endpoints take a single `tags=` field whose value is a comma- or
/// newline-separated list. `#[serde(default)]` keeps us lenient about
/// missing fields; an absent `tags` param becomes a no-op 200.
#[derive(Debug, Default, Deserialize)]
struct TagsForm {
    #[serde(default)]
    tags: Option<String>,
}

/// Form body shape for `addTags` / `removeTags`.
///
/// Both endpoints take `hashes=X|Y|Z` and `tags=a,b,c`. Missing either
/// field is a 200 no-op (matches qBt's permissive behaviour).
#[derive(Debug, Default, Deserialize)]
struct AddRemoveForm {
    #[serde(default)]
    hashes: Option<String>,
    #[serde(default)]
    tags: Option<String>,
}

/// `GET /api/v2/torrents/tags`.
///
/// Returns a JSON array of tag names, sorted alphabetically. Empty array
/// when no tags exist (matches qBt's fresh-install behaviour).
///
/// # Errors
/// - `QbtError::Internal` on serialisation failure (shouldn't happen
///   given the schema is `Vec<String>`).
pub async fn list(State(state): State<QbtState>) -> Result<QbtResponse, QbtError> {
    let names = state.session.list_tags().await;
    serde_json::to_value(&names)
        .map(QbtResponse::Json)
        .map_err(|e| QbtError::Internal(format!("serialise: {e}")))
}

/// `POST /api/v2/torrents/createTags`.
///
/// Form body: `tags=a,b,c` or `tags=a%0Ab%0Ac` (newline-delimited).
/// Idempotent on duplicate names (qBt parity). Invalid names (spaces,
/// path traversal) are silently skipped with a WARN log.
///
/// # Errors
/// - `QbtError::BadRequest` if the form body cannot be parsed as
///   `application/x-www-form-urlencoded`.
/// - Never returns 409 / 400 for per-tag problems; they're swallowed.
pub async fn create(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let form = parse_tags_form(req).await?;
    let names = parse_tag_list(form.tags.as_deref().unwrap_or(""));
    if names.is_empty() {
        return Ok(QbtResponse::ok());
    }

    let results = state.session.create_tags(names.clone()).await;
    // qBt parity: swallow AlreadyExists and InvalidName, always 200. Log
    // invalid names so operators can debug typos in their *arr configs.
    for (name, r) in names.iter().zip(results.iter()) {
        match r {
            Ok(()) | Err(TagError::AlreadyExists(_)) => {}
            Err(TagError::InvalidName(msg)) => {
                tracing::warn!(tag = %name, reason = %msg, "createTags skipped invalid name");
            }
            Err(e) => {
                tracing::warn!(tag = %name, error = %e, "createTags skipped on persistence error");
            }
        }
    }
    Ok(QbtResponse::ok())
}

/// `POST /api/v2/torrents/deleteTags`.
///
/// Form body: `tags=a,b,c`. Idempotent - unknown names are silently
/// dropped. Deleting a tag also strips it from every torrent currently
/// carrying it (see `SessionHandle::delete_tags`).
///
/// # Errors
/// - `QbtError::BadRequest` if the form body cannot be parsed.
pub async fn delete(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let form = parse_tags_form(req).await?;
    let names = parse_tag_list(form.tags.as_deref().unwrap_or(""));
    if names.is_empty() {
        return Ok(QbtResponse::ok());
    }
    // `delete_tags` returns the subset actually removed; we don't surface
    // that back (qBt doesn't) but a debug log helps during *arr triage.
    let removed = state.session.delete_tags(names).await;
    tracing::debug!(removed = ?removed, "deleteTags applied");
    Ok(QbtResponse::ok())
}

/// `POST /api/v2/torrents/addTags`.
///
/// Form body: `hashes=X|Y&tags=a,b`. Tags must already exist in the
/// registry (or will simply no-op at the session layer). Malformed
/// hashes are logged and skipped individually.
///
/// # Errors
/// - `QbtError::BadRequest` if the form body cannot be parsed.
/// - `QbtError::Internal` if the session command channel has closed.
pub async fn add_to_torrents(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let form = parse_add_remove_form(req).await?;
    let hash_strs = parse_hash_list(form.hashes.as_deref().unwrap_or(""));
    let tags = parse_tag_list(form.tags.as_deref().unwrap_or(""));
    if hash_strs.is_empty() || tags.is_empty() {
        return Ok(QbtResponse::ok());
    }

    // Parse each hash; silently skip malformed ones (qBt behaviour - don't
    // 400 the whole batch over one bad hash). We still log so operators
    // see what was skipped.
    let mut hashes = Vec::with_capacity(hash_strs.len());
    for h in &hash_strs {
        match Id20::from_hex(h) {
            Ok(id) => hashes.push(id),
            Err(e) => {
                tracing::warn!(hash = %h, error = %e, "addTags skipped malformed hash");
            }
        }
    }
    if !hashes.is_empty()
        && let Err(e) = state.session.add_tags_to_torrents(hashes, tags).await
    {
        return Err(QbtError::Internal(format!("add_tags: {e}")));
    }
    Ok(QbtResponse::ok())
}

/// `POST /api/v2/torrents/removeTags`.
///
/// Form body: `hashes=X|Y&tags=a,b`. Mirror of [`add_to_torrents`] -
/// idempotent, skips malformed hashes with a WARN log.
///
/// # Errors
/// - `QbtError::BadRequest` if the form body cannot be parsed.
/// - `QbtError::Internal` if the session command channel has closed.
pub async fn remove_from_torrents(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let form = parse_add_remove_form(req).await?;
    let hash_strs = parse_hash_list(form.hashes.as_deref().unwrap_or(""));
    let tags = parse_tag_list(form.tags.as_deref().unwrap_or(""));
    if hash_strs.is_empty() || tags.is_empty() {
        return Ok(QbtResponse::ok());
    }

    let mut hashes = Vec::with_capacity(hash_strs.len());
    for h in &hash_strs {
        match Id20::from_hex(h) {
            Ok(id) => hashes.push(id),
            Err(e) => {
                tracing::warn!(hash = %h, error = %e, "removeTags skipped malformed hash");
            }
        }
    }
    if !hashes.is_empty()
        && let Err(e) = state.session.remove_tags_from_torrents(hashes, tags).await
    {
        return Err(QbtError::Internal(format!("remove_tags: {e}")));
    }
    Ok(QbtResponse::ok())
}

/// Shared form parser for `createTags` / `deleteTags`.
///
/// Reads the request body (capped at 64 KiB - the tag list is a blob of
/// short names, not binary data) and decodes `application/x-www-form-urlencoded`.
async fn parse_tags_form(req: axum::extract::Request) -> Result<TagsForm, QbtError> {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
        .await
        .map_err(|e| QbtError::BadRequest(format!("read body: {e}")))?;
    serde_urlencoded::from_bytes(&bytes)
        .map_err(|e| QbtError::BadRequest(format!("parse urlencoded: {e}")))
}

/// Shared form parser for `addTags` / `removeTags`.
///
/// Same body limit as [`parse_tags_form`]; the hashes field can grow with
/// the torrent count but 64 KiB covers ~1500 torrents at 40 hex chars
/// each, well beyond any sane bulk operation.
async fn parse_add_remove_form(req: axum::extract::Request) -> Result<AddRemoveForm, QbtError> {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
        .await
        .map_err(|e| QbtError::BadRequest(format!("read body: {e}")))?;
    serde_urlencoded::from_bytes(&bytes)
        .map_err(|e| QbtError::BadRequest(format!("parse urlencoded: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_list_trims_whitespace_and_drops_empties() {
        let out = parse_tag_list("  sonarr , radarr,  ");
        assert_eq!(out, vec!["sonarr".to_string(), "radarr".to_string()]);
    }

    #[test]
    fn parse_tag_list_handles_newlines() {
        let out = parse_tag_list("sonarr\nradarr\nkids");
        assert_eq!(
            out,
            vec![
                "sonarr".to_string(),
                "radarr".to_string(),
                "kids".to_string()
            ]
        );
    }

    #[test]
    fn parse_tag_list_handles_crlf() {
        let out = parse_tag_list("sonarr\r\nradarr\r\n");
        assert_eq!(out, vec!["sonarr".to_string(), "radarr".to_string()]);
    }

    #[test]
    fn parse_tag_list_mixed_separators() {
        let out = parse_tag_list("sonarr,radarr\nkids");
        assert_eq!(
            out,
            vec![
                "sonarr".to_string(),
                "radarr".to_string(),
                "kids".to_string()
            ]
        );
    }

    #[test]
    fn parse_tag_list_empty_yields_empty_vec() {
        assert!(parse_tag_list("").is_empty());
        assert!(parse_tag_list("   ").is_empty());
        assert!(parse_tag_list(",,,").is_empty());
    }

    #[test]
    fn parse_hash_list_pipes() {
        let out = parse_hash_list("aaaa|bbbb|cccc");
        assert_eq!(
            out,
            vec!["aaaa".to_string(), "bbbb".to_string(), "cccc".to_string()]
        );
    }

    #[test]
    fn parse_hash_list_trims_and_drops_empties() {
        let out = parse_hash_list("  aaaa | | bbbb  ");
        assert_eq!(out, vec!["aaaa".to_string(), "bbbb".to_string()]);
    }
}
