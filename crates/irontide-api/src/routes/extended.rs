//! Extended torrent and peer management endpoint handlers.
//!
//! Provides handlers for detailed torrent inspection (info, peers, trackers),
//! torrent operations (reannounce, file priority), and peer ban management.

use std::net::{IpAddr, SocketAddr};

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::error::{ApiError, ApiResult};

use super::AppState;

/// Get detailed metadata and state for a single torrent.
///
/// Returns a JSON [`TorrentInfo`](irontide::session::TorrentInfo) object
/// containing the torrent's name, files, piece count, and current state.
///
/// # Errors
///
/// Returns an API error if the hash is invalid or the torrent is not found.
pub async fn get_torrent_info(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let id = crate::extractors::parse_info_hash(&hash)?;
    let info = session.torrent_info(id).await?;
    Ok(Json(info))
}

/// Get the list of connected peers for a torrent.
///
/// Returns a JSON array of [`PeerInfo`](irontide::session::PeerInfo) objects,
/// one per connected peer, including address, client ID, and transfer stats.
///
/// # Errors
///
/// Returns an API error if the hash is invalid or the torrent is not found.
pub async fn get_peers(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let id = crate::extractors::parse_info_hash(&hash)?;
    let peers = session.get_peer_info(id).await?;
    Ok(Json(peers))
}

/// Get the tracker list for a torrent.
///
/// Returns a JSON array of [`TrackerInfo`](irontide::session::TrackerInfo)
/// objects with each tracker's URL, tier, and announce status.
///
/// # Errors
///
/// Returns an API error if the hash is invalid or the torrent is not found.
pub async fn get_trackers(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let id = crate::extractors::parse_info_hash(&hash)?;
    let trackers = session.tracker_list(id).await?;
    Ok(Json(trackers))
}

/// Force a re-announce to all trackers for a torrent.
///
/// Returns **204 No Content** on success.
///
/// # Errors
///
/// Returns an API error if the hash is invalid or the torrent is not found.
pub async fn reannounce(
    State(session): State<AppState>,
    Path(hash): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let id = crate::extractors::parse_info_hash(&hash)?;
    session.force_reannounce(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// JSON request body for the set-file-priority endpoint.
#[derive(serde::Deserialize)]
pub struct SetPriorityRequest {
    priority: irontide::core::FilePriority,
}

/// Set the download priority for a single file within a torrent.
///
/// The `hash` path segment identifies the torrent and `idx` identifies the
/// zero-based file index. The request body must contain a JSON object with
/// a `priority` field set to one of the [`FilePriority`](irontide::core::FilePriority)
/// variants (`"Skip"`, `"Low"`, `"Normal"`, `"High"`).
///
/// Returns **204 No Content** on success.
///
/// # Errors
///
/// Returns an API error if the hash is invalid, the file index is out of
/// range, or the torrent is not found.
pub async fn set_file_priority(
    State(session): State<AppState>,
    Path((hash, idx)): Path<(String, usize)>,
    Json(req): Json<SetPriorityRequest>,
) -> ApiResult<impl IntoResponse> {
    let id = crate::extractors::parse_info_hash(&hash)?;
    session.set_file_priority(id, idx, req.priority).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// JSON request body for the add-peers endpoint.
#[derive(serde::Deserialize)]
pub struct AddPeersRequest {
    peers: Vec<String>,
}

/// Add peer addresses to a torrent's candidate pool.
///
/// Accepts a JSON body with a `peers` array of `"ip:port"` strings.
/// Peers are injected with [`PeerSource::Api`] and deduplicated against
/// the torrent's existing known-peer set.
///
/// Returns **204 No Content** on success.
///
/// # Errors
///
/// Returns an API error if the hash is invalid, the peers array is empty,
/// or any peer address is malformed.
pub async fn add_peers(
    State(session): State<AppState>,
    Path(hash): Path<String>,
    Json(req): Json<AddPeersRequest>,
) -> ApiResult<impl IntoResponse> {
    let id = crate::extractors::parse_info_hash(&hash)?;
    let addrs: Vec<SocketAddr> = req
        .peers
        .iter()
        .map(|s| {
            s.parse::<SocketAddr>()
                .map_err(|_| ApiError::bad_request(format!("invalid peer address: {s}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if addrs.is_empty() {
        return Err(ApiError::bad_request("peers array must not be empty"));
    }
    session
        .add_peers(id, addrs, irontide::session::PeerSource::Api)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Get the list of currently banned peer IP addresses.
///
/// # Errors
///
/// Returns an API error if the session is unavailable.
pub async fn get_banned_peers(State(session): State<AppState>) -> ApiResult<impl IntoResponse> {
    let banned = session.banned_peers().await?;
    Ok(Json(banned))
}

/// JSON request body for the ban-peer endpoint.
#[derive(serde::Deserialize)]
pub struct BanPeerRequest {
    ip: IpAddr,
}

/// Ban a peer by IP address.
///
/// Accepts a JSON body with an `ip` field containing the address to ban.
///
/// Returns **204 No Content** on success.
///
/// # Errors
///
/// Returns an API error if the session is unavailable.
pub async fn ban_peer(
    State(session): State<AppState>,
    Json(req): Json<BanPeerRequest>,
) -> ApiResult<impl IntoResponse> {
    session.ban_peer(req.ip).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Remove a peer IP address from the ban list.
///
/// The `ip` path segment is parsed as an [`IpAddr`]. Both IPv4 and IPv6
/// addresses are accepted.
///
/// Returns **204 No Content** on success (even if the IP was not banned).
///
/// # Errors
///
/// Returns an API error if the IP address is malformed.
pub async fn unban_peer(
    State(session): State<AppState>,
    Path(ip_str): Path<String>,
) -> ApiResult<impl IntoResponse> {
    let ip: IpAddr = ip_str
        .parse()
        .map_err(|_| ApiError::bad_request(format!("invalid IP address: {ip_str}")))?;
    session.unban_peer(ip).await?;
    Ok(StatusCode::NO_CONTENT)
}
