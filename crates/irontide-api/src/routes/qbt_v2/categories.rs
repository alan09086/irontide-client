//! qBt v2 torrents categories shim (M168).
//!
//! Returns an empty JSON object — real category CRUD lands in M170.
//! *arr clients are happy with an empty map; they only use categories
//! optionally for organising torrents within qBt.
//!
//! FIXME(M170): real category manager (createCategory, editCategory,
//! removeCategories, torrents/addCategory/removeCategory).

use super::response::QbtResponse;

/// `GET /api/v2/torrents/categories`.
pub async fn list() -> QbtResponse {
    QbtResponse::Json(serde_json::json!({}))
}
