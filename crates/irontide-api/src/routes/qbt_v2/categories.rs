//! qBt v2 category CRUD (M170 Lane C).
//!
//! Implements the four endpoints *arr clients use to organise torrents:
//!
//! - `GET  /api/v2/torrents/categories`    → list all categories
//! - `POST /api/v2/torrents/createCategory` → add a new category
//! - `POST /api/v2/torrents/editCategory`  → change an existing category's `save_path`
//! - `POST /api/v2/torrents/removeCategories` → remove categories (newline-delimited)
//!
//! The underlying registry lives on the session actor's category
//! manager. This module is pure DTO +
//! error-shape mapping; the only logic here is form-body parsing and
//! translating session errors to qBt-shaped HTTP status codes.
//!
//! # Wire format notes
//! qBt serialises category save paths with the JSON key `savePath` (camelCase),
//! even though the internal Rust field is `save_path`. The `QbtCategory` DTO
//! below handles the rename via `#[serde(rename)]` so domain types stay in
//! `snake_case` while the wire stays byte-compatible with qBt.
//!
//! `removeCategories` accepts `categories=A%0AB%0AC` where `%0A` decodes to a
//! bare `\n`. Real qBt clients (including Radarr's built-in test) send `%0A`;
//! we're also lenient about `\r\n` so hand-crafted curl requests work.

use std::collections::HashMap;
use std::path::PathBuf;

use axum::extract::State;
use irontide::session::{CategoryError, CategoryMetadata};
use serde::{Deserialize, Serialize};

use super::response::{QbtError, QbtResponse};
use super::state::QbtState;

/// Wire DTO for a single category entry.
///
/// Matches qBt's outer-map value exactly: `{ "name": ..., "savePath": ... }`.
/// Conversion from [`CategoryMetadata`] renders `save_path` via
/// `Path::to_string_lossy` — non-UTF-8 paths end up with `U+FFFD`
/// replacements, which is what qBt's own JSON encoder does.
#[derive(Debug, Clone, Serialize)]
pub struct QbtCategory {
    /// Category name; matches the outer-map key.
    pub name: String,
    /// Default save path for torrents tagged with this category.
    #[serde(rename = "savePath")]
    pub save_path: String,
}

impl From<&CategoryMetadata> for QbtCategory {
    fn from(meta: &CategoryMetadata) -> Self {
        Self {
            name: meta.name.clone(),
            save_path: meta.save_path.to_string_lossy().into_owned(),
        }
    }
}

/// Form body accepted by `createCategory` and `editCategory`.
///
/// qBt's canonical key is `savePath` (camelCase); Radarr and Sonarr send it
/// that way. We also accept `savepath` because `IronTide`'s own
/// `/torrents/add` handler uses lowercase `savepath` on the add path and it
/// would be a footgun to diverge. The `category` field carries the name.
#[derive(Debug, Default, Deserialize)]
struct CategoryForm {
    #[serde(default)]
    category: Option<String>,
    #[serde(default, alias = "savepath")]
    #[serde(rename = "savePath")]
    save_path: Option<String>,
}

/// Form body accepted by `removeCategories`.
///
/// qBt encodes the list as `categories=A%0AB%0AC` — a single form field
/// whose value is a newline-delimited blob. Empty lines and whitespace-only
/// lines are dropped by the splitter below.
#[derive(Debug, Default, Deserialize)]
struct RemoveForm {
    #[serde(default)]
    categories: Option<String>,
}

/// `GET /api/v2/torrents/categories`.
///
/// Returns a JSON object keyed by category name. Empty object when no
/// categories exist — matches qBt's behaviour on a fresh install and keeps
/// the *arr "Test Connection" probe happy.
///
/// # Errors
/// - `QbtError::Internal` on serialisation failure (shouldn't happen given
///   the schema is all `String`).
pub async fn list(State(state): State<QbtState>) -> Result<QbtResponse, QbtError> {
    let categories = state.session.list_categories().await;
    let wire: HashMap<String, QbtCategory> = categories
        .iter()
        .map(|meta| (meta.name.clone(), QbtCategory::from(meta)))
        .collect();
    serde_json::to_value(&wire)
        .map(QbtResponse::Json)
        .map_err(|e| QbtError::Internal(format!("serialise: {e}")))
}

/// `POST /api/v2/torrents/createCategory`.
///
/// Form body: `category=<name>&savePath=<path>`.
///
/// # Errors
/// - `QbtError::BadRequest` if the form is malformed or required fields
///   are missing, or the category name fails validation.
/// - `QbtError::Conflict` if a category with the same name already exists.
/// - `QbtError::Internal` on persistence failure.
pub async fn create(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let form = parse_category_form(req).await?;
    let (name, save_path) = require_category_fields(form)?;

    state
        .session
        .create_category(name, save_path)
        .await
        .map_err(map_create_error)?;
    Ok(QbtResponse::ok())
}

/// `POST /api/v2/torrents/editCategory`.
///
/// Form body: `category=<name>&savePath=<new-path>`.
///
/// # Errors
/// - `QbtError::BadRequest` on a malformed form, missing fields, or an
///   invalid category name.
/// - `QbtError::NotFound` if no category with that name exists.
/// - `QbtError::Internal` on persistence failure.
pub async fn edit(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let form = parse_category_form(req).await?;
    let (name, save_path) = require_category_fields(form)?;

    state
        .session
        .edit_category(name, save_path)
        .await
        .map_err(map_edit_error)?;
    Ok(QbtResponse::ok())
}

/// `POST /api/v2/torrents/removeCategories`.
///
/// Form body: `categories=A%0AB%0AC` — URL-encoded newlines between names.
/// Silent about unknown names; the session also clears the `category`
/// label on any torrents assigned to a removed name (see
/// `SessionHandle::remove_categories`).
///
/// # Errors
/// - `QbtError::BadRequest` if the form body cannot be parsed.
/// - Never returns 404/409 — qBt is lenient here and so are we.
pub async fn remove(
    State(state): State<QbtState>,
    req: axum::extract::Request,
) -> Result<QbtResponse, QbtError> {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
        .await
        .map_err(|e| QbtError::BadRequest(format!("read body: {e}")))?;
    let form: RemoveForm = serde_urlencoded::from_bytes(&bytes)
        .map_err(|e| QbtError::BadRequest(format!("parse urlencoded: {e}")))?;

    let names = form
        .categories
        .unwrap_or_default()
        .split('\n')
        .map(|s| s.trim_end_matches('\r').trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();

    // Always 200 — qBt swallows per-name errors on bulk remove and so do we.
    let _removed = state.session.remove_categories(names).await;
    Ok(QbtResponse::ok())
}

/// Shared form parser for create + edit.
async fn parse_category_form(req: axum::extract::Request) -> Result<CategoryForm, QbtError> {
    let bytes = axum::body::to_bytes(req.into_body(), 64 * 1024)
        .await
        .map_err(|e| QbtError::BadRequest(format!("read body: {e}")))?;
    serde_urlencoded::from_bytes(&bytes)
        .map_err(|e| QbtError::BadRequest(format!("parse urlencoded: {e}")))
}

/// Extract the non-empty `category` and `savePath` fields, returning 400
/// when either is missing. Empty strings count as missing — qBt rejects
/// them too (the form literally sends nothing for an unset field).
fn require_category_fields(form: CategoryForm) -> Result<(String, PathBuf), QbtError> {
    let name = form
        .category
        .filter(|s| !s.is_empty())
        .ok_or_else(|| QbtError::BadRequest("category field is required".into()))?;
    let save_path = form
        .save_path
        .filter(|s| !s.is_empty())
        .ok_or_else(|| QbtError::BadRequest("savePath field is required".into()))?;
    Ok((name, PathBuf::from(save_path)))
}

/// Map a `create_category` error onto the qBt response taxonomy.
/// `NotFound` should never surface here (create never emits it), but we
/// collapse it to 500 instead of silently remapping to something wrong.
fn map_create_error(e: CategoryError) -> QbtError {
    match e {
        CategoryError::InvalidName(msg) => QbtError::BadRequest(msg),
        CategoryError::AlreadyExists(msg) => QbtError::Conflict(msg),
        CategoryError::NotFound(_)
        | CategoryError::Persistence(_)
        | CategoryError::Serialise(_) => QbtError::Internal(e.to_string()),
    }
}

/// Map an `edit_category` error onto the qBt response taxonomy.
/// `AlreadyExists` should not surface here; treating it as Internal keeps
/// any future upstream change from silently returning a 200.
fn map_edit_error(e: CategoryError) -> QbtError {
    match e {
        CategoryError::InvalidName(msg) => QbtError::BadRequest(msg),
        CategoryError::NotFound(_) => QbtError::NotFound,
        CategoryError::AlreadyExists(_)
        | CategoryError::Persistence(_)
        | CategoryError::Serialise(_) => QbtError::Internal(e.to_string()),
    }
}
