//! Rendering for the Changelog/About tab.
//!
//! Displays a static About block (app name, version, description) above a
//! scrollable list of collapsible changelog entries parsed from `changelog.json`.

use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{content_block, render_scrollbar};

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

    let block = content_block(
        " About ".fg(app.theme.header).bold(),
        false,
        app.user_data.border_rounded,
        &app.theme,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let name_line = Line::from(vec![
        Span::raw("  "),
        "Brochure".fg(app.theme.text).bold(),
        Span::raw("  "),
        format!("v{version}").fg(app.theme.accent).bold(),
    ]);
    let desc_line = Line::from(vec![
        "  A terminal RSS reader built with Ratatui".fg(app.theme.muted),
    ]);

    let blank_line = Line::raw("");
    let author_line = Line::from(vec![
        "  Author:      ".fg(app.theme.muted),
        "Sylviromi".fg(app.theme.text),
    ]);
    let license_line = Line::from(vec![
        "  License:     ".fg(app.theme.muted),
        "MIT".fg(app.theme.text),
    ]);
    let repo_line = Line::from(vec![
        "  Repository:  ".fg(app.theme.muted),
        "https://github.com/Sylviromi/brochure".fg(app.theme.link),
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
        .bg(app.theme.bg),
        inner,
    );
}

/// Renders the scrollable changelog entries block with collapse/expand and cursor highlight.
fn draw_changelog_block(f: &mut Frame, app: &mut App, area: Rect) {
    let block = content_block(
        " Changelog ".fg(app.theme.header).bold(),
        true,
        app.user_data.border_rounded,
        &app.theme,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let entries: Vec<ChangelogEntry> = match serde_json::from_str(CHANGELOG_JSON) {
        Ok(v) => v,
        Err(e) => {
            f.render_widget(
                Paragraph::new(format!("Error parsing changelog.json: {e}"))
                    .style(Style::default().fg(app.theme.text))
                    .bg(app.theme.bg),
                inner,
            );
            return;
        }
    };

    let viewport_height = inner.height as usize;
    let total = entries.len();
    let mut header_lines: Vec<usize> = Vec::with_capacity(total);
    let mut entry_ends: Vec<usize> = Vec::with_capacity(total);

    let mut full_lines: Vec<Line> = Vec::new();
    for (i, entry) in entries.iter().rev().enumerate() {
        let collapsed = app.changelog_collapsed.contains(&i);
        let is_last = i == total - 1;
        let is_cursor = i == app.changelog_cursor;
        let connector = if is_last {
            if app.user_data.border_rounded {
                " ╰─"
            } else {
                " └─"
            }
        } else {
            " ├─"
        };
        let prefix = if is_last { "    " } else { " │  " };
        let toggle_indicator = if collapsed { " ▶" } else { " ▼" };

        header_lines.push(full_lines.len());

        let cursor_highlight = if is_cursor {
            Style::default()
                .fg(app.theme.bg_dark)
                .bg(app.theme.accent)
                .bold()
        } else {
            Style::default().fg(app.theme.text)
        };

        let toggle_color = if is_cursor {
            app.theme.bg_dark
        } else {
            app.theme.muted
        };

        full_lines.push(
            Line::from(vec![
                connector
                    .to_string()
                    .fg(if is_cursor {
                        app.theme.bg_dark
                    } else {
                        app.theme.border
                    })
                    .bg(if is_cursor {
                        app.theme.accent
                    } else {
                        app.theme.bg
                    }),
                toggle_indicator.fg(toggle_color).bg(if is_cursor {
                    app.theme.accent
                } else {
                    app.theme.bg
                }),
                " ".fg(if is_cursor {
                    app.theme.bg_dark
                } else {
                    app.theme.border
                })
                .bg(if is_cursor {
                    app.theme.accent
                } else {
                    app.theme.bg
                }),
                format!("v{}", entry.version)
                    .fg(if is_cursor {
                        app.theme.bg_dark
                    } else {
                        app.theme.accent
                    })
                    .bg(if is_cursor {
                        app.theme.accent
                    } else {
                        app.theme.bg
                    })
                    .bold(),
                "  ·  "
                    .fg(if is_cursor {
                        app.theme.bg_dark
                    } else {
                        app.theme.border
                    })
                    .bg(if is_cursor {
                        app.theme.accent
                    } else {
                        app.theme.bg
                    }),
                entry
                    .date
                    .clone()
                    .fg(if is_cursor {
                        app.theme.bg_dark
                    } else {
                        app.theme.muted
                    })
                    .bg(if is_cursor {
                        app.theme.accent
                    } else {
                        app.theme.bg
                    }),
            ])
            .style(cursor_highlight),
        );

        if !collapsed {
            full_lines.push(Line::from(vec![
                prefix.fg(app.theme.border),
                entry.summary.clone().fg(app.theme.text),
            ]));
            for highlight in &entry.highlights {
                full_lines.push(Line::from(vec![
                    prefix.fg(app.theme.border),
                    format!("  • {highlight}").fg(app.theme.muted),
                ]));
            }
        }

        if !is_last {
            full_lines.push(Line::from(vec![prefix.fg(app.theme.border)]));
        }

        entry_ends.push(full_lines.len() - 1);
    }

    // Auto-scroll to keep the entire cursor entry visible.
    if let Some(&cursor_line) = header_lines.get(app.changelog_cursor)
        && viewport_height > 0
    {
        let entry_end = entry_ends
            .get(app.changelog_cursor)
            .copied()
            .unwrap_or(cursor_line);
        let entry_height = entry_end - cursor_line + 1;

        if cursor_line < app.changelog_scroll as usize {
            app.changelog_scroll = cursor_line as u16;
        }

        if entry_height <= viewport_height
            && entry_end >= app.changelog_scroll as usize + viewport_height
        {
            app.changelog_scroll = (entry_end - viewport_height + 1) as u16;
        } else if entry_height > viewport_height
            && cursor_line >= app.changelog_scroll as usize + viewport_height
        {
            app.changelog_scroll = cursor_line as u16;
        }
    }

    let total_lines = full_lines.len();
    let max_scroll = total_lines.saturating_sub(viewport_height) as u16;
    if app.changelog_scroll > max_scroll {
        app.changelog_scroll = max_scroll;
    }

    let has_scrollbar = total_lines > viewport_height;
    let text_area = if has_scrollbar {
        Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        }
    } else {
        inner
    };

    let visible: Vec<Line> = full_lines
        .into_iter()
        .skip(app.changelog_scroll as usize)
        .take(viewport_height)
        .collect();

    f.render_widget(Paragraph::new(visible).bg(app.theme.bg), text_area);

    if has_scrollbar {
        let bar_area = Rect {
            x: inner.x + inner.width.saturating_sub(1),
            width: 1,
            ..inner
        };
        render_scrollbar(
            f,
            bar_area,
            total_lines,
            viewport_height,
            app.changelog_scroll as usize,
            &app.theme,
        );
    }
}
