//! Shared command dispatch for every CLI mode.
//!
//! Each `cmd_*` function takes an `&ApiClient`, command-specific args,
//! and a mutable `Output` sink. The top-level binary (T5), the REPL
//! (T6), and the TUI (T7) all dispatch through these so that output,
//! error handling, and JSON/human switching are consistent.
//!
//! ## Conventions
//!
//! - No panics. Every failure is a `CliError`.
//! - `Output::Json` produces exactly one JSON value per command, so
//!   batch mode can pipe output into `jq` without a second pass.
//! - Hash arguments accept a prefix (minimum 2 chars); prefixes are
//!   expanded via `resolve_hash` before the daemon call.

pub(crate) mod config;

use crate::client::{ApiClient, TorrentSummaryDto};
use crate::error::CliError;
use crate::format::{format_rate, format_size};
use crate::progress;

/// Output sink wrapper: either human-readable text or JSON.
pub(crate) enum Output<'a> {
    /// Plain-text output (stdout, stderr, or a test buffer).
    Human(&'a mut dyn std::io::Write),
    /// JSON output.
    Json(&'a mut dyn std::io::Write),
}

impl Output<'_> {
    /// Whether this sink expects JSON output.
    fn is_json(&self) -> bool {
        matches!(self, Self::Json(_))
    }

    /// Mutable borrow of the underlying writer.
    fn writer(&mut self) -> &mut dyn std::io::Write {
        match self {
            Self::Human(w) | Self::Json(w) => *w,
        }
    }

    /// Write a line, appending `\n`. Swallows I/O errors by mapping to
    /// `CliError::Other` so callers never have to handle `io::Error`.
    fn writeln(&mut self, s: &str) -> Result<(), CliError> {
        writeln!(self.writer(), "{s}")
            .map_err(|e| CliError::Other(anyhow::anyhow!("write failed: {e}")))
    }

    /// Serialize `value` as pretty JSON and write it with a trailing
    /// newline. Only use on `Output::Json`.
    fn write_json(&mut self, value: &serde_json::Value) -> Result<(), CliError> {
        let text = serde_json::to_string_pretty(value)
            .map_err(|e| CliError::Other(anyhow::anyhow!("serialize failed: {e}")))?;
        writeln!(self.writer(), "{text}")
            .map_err(|e| CliError::Other(anyhow::anyhow!("write failed: {e}")))
    }
}

/// Arguments for `cmd_list`.
#[derive(Debug, Clone, Default)]
pub(crate) struct ListArgs {
    /// Optional filter: `"downloading"`, `"seeding"`, or `"paused"`.
    /// A trailing `"all"` disables filtering.
    pub(crate) filter: Option<String>,
}

/// Minimum hash prefix length accepted by `resolve_hash`.
const MIN_PREFIX_LEN: usize = 2;

/// Expand a hash prefix into the unique full info hash.
///
/// Returns `CliError::NotFound` when no torrent matches, or
/// `CliError::InvalidInput` when the prefix is ambiguous or too short.
pub(crate) async fn resolve_hash(client: &ApiClient, prefix: &str) -> Result<String, CliError> {
    // 40 chars → already a full hash; just validate format upstream.
    if prefix.len() == 40 && prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(prefix.to_ascii_lowercase());
    }
    if prefix.len() < MIN_PREFIX_LEN {
        return Err(CliError::InvalidInput(format!(
            "hash prefix must be at least {MIN_PREFIX_LEN} characters"
        )));
    }
    if !prefix.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CliError::InvalidInput(format!(
            "hash prefix contains non-hex characters: {prefix}"
        )));
    }

    let needle = prefix.to_ascii_lowercase();
    let list = client.list_torrents().await?;
    let matches: Vec<&TorrentSummaryDto> = list
        .iter()
        .filter(|t| t.info_hash.starts_with(&needle))
        .collect();

    match matches.len() {
        0 => Err(CliError::NotFound(prefix.to_owned())),
        1 => Ok(matches[0].info_hash.clone()),
        n => {
            let sample: Vec<String> = matches
                .iter()
                .take(5)
                .map(|t| format!("{}: {}", short_hash(&t.info_hash), t.name))
                .collect();
            Err(CliError::InvalidInput(format!(
                "prefix \"{prefix}\" matches {n} torrents; disambiguate: {}",
                sample.join(", "),
            )))
        }
    }
}

/// Truncate a hash for error messages (`"aaf4c61d…"`).
fn short_hash(hash: &str) -> String {
    if hash.len() <= 8 {
        return hash.to_owned();
    }
    format!("{}…", &hash[..8])
}

/// `list` command — print a table of all active torrents.
pub(crate) async fn cmd_list(
    client: &ApiClient,
    args: &ListArgs,
    out: &mut Output<'_>,
) -> Result<(), CliError> {
    let mut torrents = client.list_torrents().await?;

    if let Some(filter) = &args.filter {
        let filter_lc = filter.to_ascii_lowercase();
        if filter_lc != "all" {
            let matcher = match filter_lc.as_str() {
                "downloading" => "Downloading",
                "seeding" => "Seeding",
                "paused" => "Paused",
                other => {
                    return Err(CliError::InvalidInput(format!(
                        "unknown list filter: {other}"
                    )));
                }
            };
            torrents.retain(|t| t.state == matcher);
        }
    }

    if out.is_json() {
        let value = serde_json::to_value(
            torrents
                .iter()
                .map(torrent_summary_to_json)
                .collect::<Vec<_>>(),
        )
        .map_err(|e| CliError::Other(anyhow::anyhow!("serialize failed: {e}")))?;
        out.write_json(&value)?;
        return Ok(());
    }

    if torrents.is_empty() {
        out.writeln("(no torrents)")?;
        return Ok(());
    }

    out.writeln(&format!(
        "{:>3}  {:<12}  {:>8}  {:>11}  {:>6}  {}",
        "#", "State", "Progress", "Rate", "Peers", "Name"
    ))?;
    out.writeln("---  ------------  --------  -----------  ------  --------------------")?;
    for (idx, t) in torrents.iter().enumerate() {
        let progress_col = format!("{:>6.1}%", t.progress * 100.0);
        let rate_col = format!("↓{}", format_rate(t.download_rate));
        out.writeln(&format!(
            "{:>3}  {:<12}  {:>8}  {:>11}  {:>6}  {}",
            idx + 1,
            truncate_for_col(&t.state, 12),
            progress_col,
            rate_col,
            t.num_peers,
            t.name,
        ))?;
    }

    Ok(())
}

/// Serialize a `TorrentSummaryDto` as a JSON object (fields the CLI
/// displays, with bytes-per-second rates rather than formatted strings).
fn torrent_summary_to_json(t: &TorrentSummaryDto) -> serde_json::Value {
    serde_json::json!({
        "info_hash": t.info_hash,
        "name": t.name,
        "state": t.state,
        "progress": t.progress,
        "download_rate": t.download_rate,
        "upload_rate": t.upload_rate,
        "total_size": t.total_size,
        "num_peers": t.num_peers,
        "added_time": t.added_time,
    })
}

/// Truncate `s` with an ellipsis for a narrow column.
fn truncate_for_col(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len <= width {
        return s.to_owned();
    }
    if width <= 1 {
        return s.chars().take(width).collect();
    }
    let mut out: String = s.chars().take(width - 1).collect();
    out.push('…');
    out
}

/// `add` command — accepts either a magnet URI or a `.torrent` file path.
pub(crate) async fn cmd_add(
    client: &ApiClient,
    source: &str,
    out: &mut Output<'_>,
) -> Result<(), CliError> {
    let hashes = if source.starts_with("magnet:") {
        client.add_magnet(source).await?
    } else {
        let bytes = std::fs::read(source)
            .map_err(|e| CliError::Other(anyhow::anyhow!("failed to read {source}: {e}")))?;
        client.add_torrent_bytes(&bytes).await?
    };

    let hex = hashes.v1_hex().unwrap_or_default();
    if out.is_json() {
        let value = serde_json::json!({
            "info_hash": hex,
            "name": serde_json::Value::Null,
            "added": true,
        });
        out.write_json(&value)?;
    } else {
        out.writeln(&format!("added torrent {hex} (pending metadata)"))?;
    }
    Ok(())
}

/// `remove` command.
pub(crate) async fn cmd_remove(
    client: &ApiClient,
    hash: &str,
    out: &mut Output<'_>,
) -> Result<(), CliError> {
    let resolved = resolve_hash(client, hash).await?;
    client.remove_torrent(&resolved).await?;
    emit_simple(out, "removed", &resolved)
}

/// `pause` command.
pub(crate) async fn cmd_pause(
    client: &ApiClient,
    hash: &str,
    out: &mut Output<'_>,
) -> Result<(), CliError> {
    let resolved = resolve_hash(client, hash).await?;
    client.pause(&resolved).await?;
    emit_simple(out, "paused", &resolved)
}

/// `resume` command.
pub(crate) async fn cmd_resume(
    client: &ApiClient,
    hash: &str,
    out: &mut Output<'_>,
) -> Result<(), CliError> {
    let resolved = resolve_hash(client, hash).await?;
    client.resume(&resolved).await?;
    emit_simple(out, "resumed", &resolved)
}

/// `seed` / `unseed` command. Callers pick `enabled` based on the
/// subcommand variant.
pub(crate) async fn cmd_seed(
    client: &ApiClient,
    hash: &str,
    enabled: bool,
    out: &mut Output<'_>,
) -> Result<(), CliError> {
    let resolved = resolve_hash(client, hash).await?;
    client.set_seed_mode(&resolved, enabled).await?;
    let verb = if enabled {
        "seed-mode enabled"
    } else {
        "seed-mode disabled"
    };
    emit_simple(out, verb, &resolved)
}

/// `info` command — fetch stats (+ optional info / peers) and render
/// via the `progress` module.
pub(crate) async fn cmd_info(
    client: &ApiClient,
    hash: &str,
    show_files: bool,
    show_peers: bool,
    out: &mut Output<'_>,
) -> Result<(), CliError> {
    let resolved = resolve_hash(client, hash).await?;
    let stats = client.get_torrent(&resolved).await?;
    let info = if show_files {
        Some(client.torrent_info(&resolved).await?)
    } else {
        None
    };
    let peers = if show_peers {
        Some(client.torrent_peers(&resolved).await?)
    } else {
        None
    };

    if out.is_json() {
        let mut value = progress::render_json(&stats, info.as_ref(), None);
        if let Some(peers) = peers
            && let serde_json::Value::Object(obj) = &mut value
        {
            obj.insert("peers".to_owned(), serde_json::to_value(&peers)?);
        }
        out.write_json(&value)?;
        return Ok(());
    }

    let lines =
        progress::render_human(&stats, info.as_ref(), None, progress::RenderOpts::default());
    for line in lines {
        out.writeln(&line)?;
    }
    // Extra summary line mirroring rqbit: session byte totals.
    out.writeln(&format!(
        "  downloaded {}   uploaded {}",
        format_size(stats.downloaded),
        format_size(stats.uploaded),
    ))?;
    Ok(())
}

/// Shared helper for simple action commands (remove/pause/resume/seed):
/// emits either a text line or a JSON envelope.
fn emit_simple(out: &mut Output<'_>, verb: &str, hash: &str) -> Result<(), CliError> {
    if out.is_json() {
        let value = serde_json::json!({
            "action": verb,
            "info_hash": hash,
            "ok": true,
        });
        out.write_json(&value)?;
    } else {
        out.writeln(&format!("{verb}: {hash}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_hash_truncates() {
        assert_eq!(short_hash("aaf4c61ddcc5e8a2"), "aaf4c61d…");
    }

    #[test]
    fn short_hash_passthrough() {
        assert_eq!(short_hash("aabb"), "aabb");
    }

    #[test]
    fn truncate_for_col_pads_and_cuts() {
        assert_eq!(truncate_for_col("ok", 6), "ok");
        assert_eq!(truncate_for_col("abcdefghij", 6), "abcde…");
    }
}
