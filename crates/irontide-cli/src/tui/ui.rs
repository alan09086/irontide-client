//! Ratatui widget rendering for the `irontide tui` dashboard.
//!
//! A single entry point — [`draw`] — partitions the frame into
//! top-bar / main / bottom-bar regions and delegates to helper
//! functions for each. Modal dialogs are rendered on top via a
//! centered rect with [`Clear`] as the first widget so the
//! underlying dashboard is masked while the modal is open.
//!
//! None of these functions allocate per-render unless unavoidable;
//! ratatui's widget constructors take borrowed strings where we
//! can, so the hot path is mostly stack-based formatting.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};

use super::state::{AppState, Modal};
use crate::format::{format_rate, format_size};

/// Render the current app state into the given frame.
///
/// The layout is fixed:
/// - row 0: top bar (1 line)
/// - rows 1..N-2: main dashboard (flexible)
/// - row N-1: bottom bar (1 line)
///
/// If a modal is active, it's drawn as a centered rectangle on top
/// of the main area after clearing that rect.
pub(crate) fn draw(f: &mut Frame<'_>, state: &AppState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top bar
            Constraint::Min(1),    // main area
            Constraint::Length(1), // bottom bar
        ])
        .split(area);

    draw_top_bar(f, chunks[0], state);
    draw_main(f, chunks[1], state);
    draw_bottom_bar(f, chunks[2]);

    if let Some(modal) = &state.modal {
        draw_modal(f, chunks[1], modal);
    }
}

/// Render the session summary on the top bar.
fn draw_top_bar(f: &mut Frame<'_>, area: Rect, state: &AppState) {
    let hostname = std::env::var("HOSTNAME").unwrap_or_default();
    let mut segments = vec![
        Span::styled(
            "IronTide",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  {} torrents  \u{2193}{}  \u{2191}{}",
            state.torrents.len(),
            format_rate(state.agg_down),
            format_rate(state.agg_up),
        )),
    ];
    if !hostname.is_empty() {
        segments.push(Span::raw(format!("  ({hostname})")));
    }
    if let Some(err) = state.last_error.as_deref() {
        segments.push(Span::raw("  "));
        segments.push(Span::styled(
            format!("[{err}]"),
            Style::default().fg(Color::Red),
        ));
    }
    let para = Paragraph::new(Line::from(segments)).alignment(Alignment::Left);
    f.render_widget(para, area);
}

/// Render the bottom-bar keybind hints.
fn draw_bottom_bar(f: &mut Frame<'_>, area: Rect) {
    let reverse = Style::default().add_modifier(Modifier::REVERSED);
    let spans = vec![
        Span::styled(" \u{2191}\u{2193} ", reverse),
        Span::raw(" nav  "),
        Span::styled(" Enter ", reverse),
        Span::raw(" expand  "),
        Span::styled(" s ", reverse),
        Span::raw(" seed  "),
        Span::styled(" p ", reverse),
        Span::raw(" pause  "),
        Span::styled(" r ", reverse),
        Span::raw(" resume  "),
        Span::styled(" a ", reverse),
        Span::raw(" add  "),
        Span::styled(" d ", reverse),
        Span::raw(" delete  "),
        Span::styled(" ? ", reverse),
        Span::raw(" help  "),
        Span::styled(" q ", reverse),
        Span::raw(" quit"),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the main dashboard — torrent list or empty placeholder.
fn draw_main(f: &mut Frame<'_>, area: Rect, state: &AppState) {
    if state.torrents.is_empty() {
        let para = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No torrents in the daemon.",
                Style::default().add_modifier(Modifier::DIM),
            )),
            Line::from(""),
            Line::from(Span::raw("Press `a` to add a magnet URI.")),
        ])
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("torrents"));
        f.render_widget(para, area);
        return;
    }

    // Compose the top-level torrent table. If the selected row is
    // expanded, we reserve half the area below for the detail view.
    let expanded_selected = state
        .selected_hash()
        .is_some_and(|h| state.expanded.contains(h));

    let (list_area, detail_area) = if expanded_selected {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);
        (split[0], Some(split[1]))
    } else {
        (area, None)
    };

    draw_torrent_table(f, list_area, state);

    if let Some(detail) = detail_area
        && let Some(hash) = state.selected_hash()
    {
        draw_detail_panel(f, detail, state, hash);
    }
}

/// Render the main torrent-list table.
fn draw_torrent_table(f: &mut Frame<'_>, area: Rect, state: &AppState) {
    let header = Row::new(vec![
        Cell::from("name"),
        Cell::from("state"),
        Cell::from("progress"),
        Cell::from("\u{2193} rate"),
        Cell::from("\u{2191} rate"),
        Cell::from("peers"),
        Cell::from("size"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let rows: Vec<Row<'_>> = state
        .torrents
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let pct = format!("{:>5.1}%", (t.progress * 100.0).clamp(0.0, 100.0));
            let bar = crate::format::progress_bar(t.progress, 10);
            let name = truncate_for_column(&t.name, 30);
            let state_label = short_state(&t.state);
            let mut row = Row::new(vec![
                Cell::from(name),
                Cell::from(state_label),
                Cell::from(format!("{bar} {pct}")),
                Cell::from(format_rate(t.download_rate)),
                Cell::from(format_rate(t.upload_rate)),
                Cell::from(t.num_peers.to_string()),
                Cell::from(format_size(t.total_size)),
            ]);
            if i == state.selected {
                row = row.style(
                    Style::default()
                        .add_modifier(Modifier::REVERSED)
                        .add_modifier(Modifier::BOLD),
                );
            }
            row
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(20),
            Constraint::Length(12),
            Constraint::Length(18),
            Constraint::Length(11),
            Constraint::Length(11),
            Constraint::Length(6),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} torrents ", state.torrents.len())),
    )
    .column_spacing(1);

    f.render_widget(table, area);
}

/// Render the detail panel below the torrent list for the expanded row.
fn draw_detail_panel(f: &mut Frame<'_>, area: Rect, state: &AppState, hash: &str) {
    let Some(cached) = state.detail_cache.get(hash) else {
        let para = Paragraph::new(Line::from(Span::styled(
            "loading detail…",
            Style::default().add_modifier(Modifier::DIM),
        )))
        .block(Block::default().borders(Borders::ALL).title(" detail "));
        f.render_widget(para, area);
        return;
    };

    let total = cached.stats.total.max(1);
    let pct_line = Line::from(format!(
        "pieces {}/{}  \u{2193} {}  \u{2191} {}  peers {}  seed_mode={}",
        cached.stats.pieces_have,
        cached.stats.pieces_total,
        format_rate(cached.stats.download_rate),
        format_rate(cached.stats.upload_rate),
        cached.stats.peers_connected,
        cached.stats.user_seed_mode,
    ));
    let progress_line = Line::from(format!(
        "{}/{} ({:.1}%)",
        format_size(cached.stats.downloaded),
        format_size(total),
        cached.stats.progress * 100.0,
    ));

    // File table (path + size — no per-file progress available today).
    let file_rows: Vec<Row<'_>> = cached
        .info
        .files
        .iter()
        .take(16)
        .map(|f| {
            Row::new(vec![
                Cell::from(truncate_for_column(&f.path, 50)),
                Cell::from(format_size(f.length)),
            ])
        })
        .collect();

    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let summary = Paragraph::new(vec![pct_line, progress_line])
        .block(Block::default().borders(Borders::ALL).title(" detail "));
    f.render_widget(summary, body[0]);

    let more = cached.info.files.len().saturating_sub(file_rows.len());
    let title = if more == 0 {
        format!(" files ({}) ", cached.info.files.len())
    } else {
        format!(
            " files ({} shown of {}) ",
            file_rows.len(),
            cached.info.files.len()
        )
    };
    let file_table = Table::new(file_rows, [Constraint::Min(20), Constraint::Length(12)])
        .header(Row::new(vec![Cell::from("path"), Cell::from("size")]).bold())
        .block(Block::default().borders(Borders::ALL).title(title))
        .column_spacing(1);
    f.render_widget(file_table, body[1]);
}

/// Render a modal dialog on top of the main area.
fn draw_modal(f: &mut Frame<'_>, area: Rect, modal: &Modal) {
    let modal_area = centered_rect(60, 30, area);
    // Clear the underlying widgets so the modal body is readable.
    f.render_widget(Clear, modal_area);

    match modal {
        Modal::AddMagnet { input } => {
            let body = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::raw(format!("{input}\u{2588}"))),
                Line::from(""),
                Line::from(Span::styled(
                    "[Enter] add   [Esc] cancel",
                    Style::default().add_modifier(Modifier::DIM),
                )),
            ])
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Add magnet URL "),
            );
            f.render_widget(body, modal_area);
        }
        Modal::ConfirmDelete { hash, name } => {
            let short_hash = if hash.len() > 12 { &hash[..12] } else { hash };
            let body = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::raw(format!("Delete \"{name}\"?"))),
                Line::from(Span::styled(
                    format!("({short_hash}\u{2026})"),
                    Style::default().add_modifier(Modifier::DIM),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "[y] yes   [n] no",
                    Style::default().add_modifier(Modifier::DIM),
                )),
            ])
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Confirm delete "),
            );
            f.render_widget(body, modal_area);
        }
        Modal::Help => {
            let body = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::raw("\u{2191} / k       move selection up")),
                Line::from(Span::raw("\u{2193} / j       move selection down")),
                Line::from(Span::raw("Enter           toggle expanded detail")),
                Line::from(Span::raw("s               toggle seed-only mode")),
                Line::from(Span::raw("p               pause selected")),
                Line::from(Span::raw("r               resume selected")),
                Line::from(Span::raw("d / Delete      delete selected (with confirm)")),
                Line::from(Span::raw("a               add magnet URI")),
                Line::from(Span::raw("F5              force refresh")),
                Line::from(Span::raw("?               open this help overlay")),
                Line::from(Span::raw("q / Esc / Ctrl-C  quit")),
                Line::from(""),
                Line::from(Span::styled(
                    "[any key] close",
                    Style::default().add_modifier(Modifier::DIM),
                )),
            ])
            .alignment(Alignment::Left)
            .block(Block::default().borders(Borders::ALL).title(" Help "));
            f.render_widget(body, modal_area);
        }
    }
}

/// Compute a centered rectangle of `percent_x` × `percent_y` within `r`.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// Truncate a string to at most `max` display characters, appending an
/// ellipsis if it was truncated. Operates on `char` boundaries so we
/// never cut a multi-byte codepoint in half.
fn truncate_for_column(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('\u{2026}');
    out
}

/// Shorten a state enum name to a fixed-width label.
fn short_state(state: &str) -> String {
    match state {
        "Downloading" => "downloading".to_owned(),
        "Seeding" => "seeding".to_owned(),
        "Paused" => "paused".to_owned(),
        "CheckingResume" | "CheckingFiles" => "checking".to_owned(),
        "Initializing" => "init".to_owned(),
        "Finished" => "finished".to_owned(),
        other => other.to_ascii_lowercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_shorter_than_max_unchanged() {
        assert_eq!(truncate_for_column("abc", 10), "abc");
    }

    #[test]
    fn test_truncate_exact_length_unchanged() {
        assert_eq!(truncate_for_column("abcdefghij", 10), "abcdefghij");
    }

    #[test]
    fn test_truncate_longer_adds_ellipsis() {
        let out = truncate_for_column("abcdefghijk", 10);
        assert_eq!(out.chars().count(), 10);
        assert!(out.ends_with('\u{2026}'));
    }

    #[test]
    fn test_truncate_respects_char_boundaries() {
        // Multi-byte codepoints must not be split mid-byte.
        let input = "日本語テスト";
        let out = truncate_for_column(input, 4);
        assert_eq!(out.chars().count(), 4);
        assert!(out.ends_with('\u{2026}'));
    }

    #[test]
    fn test_short_state_known_variants() {
        assert_eq!(short_state("Downloading"), "downloading");
        assert_eq!(short_state("Seeding"), "seeding");
        assert_eq!(short_state("Paused"), "paused");
        assert_eq!(short_state("CheckingResume"), "checking");
        assert_eq!(short_state("Initializing"), "init");
    }

    #[test]
    fn test_short_state_unknown_passthrough() {
        assert_eq!(short_state("MysterySeed"), "mysteryseed");
    }

    #[test]
    fn test_centered_rect_math() {
        let r = Rect::new(0, 0, 100, 100);
        let c = centered_rect(60, 30, r);
        assert_eq!(c.width, 60);
        assert_eq!(c.height, 30);
        assert_eq!(c.x, 20);
        assert_eq!(c.y, 35);
    }
}
