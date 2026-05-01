//! Rendering for the Changelog/About tab.
//!
//! Displays a static About block (app name, version, description) above a
//! scrollable list of changelog entries parsed from the embedded `changelog.json`.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::App;

use super::{BASE, BLUE, MAUVE, SUBTEXT0, SURFACE0, TEXT, border_set};

const CHANGELOG_JSON: &str = include_str!("../../changelog.json");

#[derive(serde::Deserialize)]
struct ChangelogEntry {
    version: String,
    date: String,
    summary: String,
    highlights: Vec<String>,
}

/// Renders the Changelog tab: an About block at the top and a scrollable changelog list below.
pub(super) fn draw_changelog_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(0)])
        .split(area);

    draw_about_block(f, app, chunks[0]);
    draw_changelog_block(f, app, chunks[1]);
}

/// Renders the About block showing the app name, version, and a short description.
fn draw_about_block(f: &mut Frame, app: &App, area: Rect) {
    let version = env!("CARGO_PKG_VERSION");

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SURFACE0))
        .title(" About ")
        .bg(BASE);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let name_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Brochure",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("v{version}"),
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        ),
    ]);
    let desc_line = Line::from(vec![Span::styled(
        "  A terminal RSS reader built with Ratatui",
        Style::default().fg(SUBTEXT0),
    )]);

    let blank_line = Line::raw("");
    let author_line = Line::from(vec![
        Span::styled("  Author:      ", Style::default().fg(SUBTEXT0)),
        Span::styled("Sylviromi", Style::default().fg(TEXT)),
    ]);
    let license_line = Line::from(vec![
        Span::styled("  License:     ", Style::default().fg(SUBTEXT0)),
        Span::styled("MIT", Style::default().fg(TEXT)),
    ]);
    let repo_line = Line::from(vec![
        Span::styled("  Repository:  ", Style::default().fg(SUBTEXT0)),
        Span::styled(
            "https://github.com/Sylviromi/brochure",
            Style::default().fg(BLUE),
        ),
    ]);

    f.render_widget(
        Paragraph::new(vec![
            name_line,
            desc_line,
            blank_line,
            author_line,
            license_line,
            repo_line,
        ])
        .bg(BASE),
        inner,
    );
}

/// Renders the scrollable changelog entries block.
fn draw_changelog_block(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SURFACE0))
        .title(" Changelog ")
        .bg(BASE);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let entries: Vec<ChangelogEntry> = match serde_json::from_str(CHANGELOG_JSON) {
        Ok(v) => v,
        Err(e) => {
            f.render_widget(
                Paragraph::new(format!("Error parsing changelog.json: {e}"))
                    .style(Style::default().fg(TEXT))
                    .bg(BASE),
                inner,
            );
            return;
        }
    };

    let mut lines: Vec<Line> = Vec::new();
    for (i, entry) in entries.iter().rev().enumerate() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("v{}", entry.version),
                Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ·  ", Style::default().fg(SURFACE0)),
            Span::styled(entry.date.clone(), Style::default().fg(SUBTEXT0)),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(entry.summary.clone(), Style::default().fg(TEXT)),
        ]));
        for highlight in &entry.highlights {
            lines.push(Line::from(vec![Span::styled(
                format!("    • {highlight}"),
                Style::default().fg(SUBTEXT0),
            )]));
        }
        if i + 1 < entries.len() {
            lines.push(Line::raw(""));
        }
    }

    let viewport_height = inner.height as usize;
    let max_scroll = lines.len().saturating_sub(viewport_height) as u16;
    if app.changelog_scroll > max_scroll {
        app.changelog_scroll = max_scroll;
    }

    let visible: Vec<Line> = lines
        .into_iter()
        .skip(app.changelog_scroll as usize)
        .take(viewport_height)
        .collect();

    f.render_widget(Paragraph::new(visible).bg(BASE), inner);
}
