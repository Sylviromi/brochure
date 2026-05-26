//! Shared chrome rendering: tab bar, progress bar, and footer hints.
//!
//! This module renders the persistent chrome elements that appear on every frame:
//! the top tab bar showing the current tab, feed stats, and progress during fetches;
//! and the footer showing context-sensitive key hints and scrolling status messages.

use crate::{
    app::App,
    models::{AppState, FAVORITES_URL, Tab},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

use super::content_block;

/// Renders the top tab bar showing the currently selected tab, feed/article counts, and help text.
pub(super) fn draw_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let tabs = [
        (" Feeds ", Tab::Feeds),
        (" Saved ", Tab::Saved),
        (" Settings ", Tab::Settings),
        (" Changelog ", Tab::Changelog),
    ];

    let mut tab_spans: Vec<Span> = vec![
        Span::styled(
            " Brochure ",
            Style::default()
                .fg(app.theme.contrast_color(app.theme.accent))
                .bg(app.theme.accent)
                .bold(),
        ),
        Span::raw("  "),
    ];
    for (label, tab) in &tabs {
        if app.selected_tab == *tab {
            tab_spans.push(Span::styled(
                *label,
                Style::default()
                    .fg(app.theme.contrast_color(app.theme.accent))
                    .bg(app.theme.accent)
                    .bold(),
            ));
        } else {
            tab_spans.push(Span::styled(
                *label,
                Style::default().fg(app.theme.muted),
            ));
        }
        tab_spans.push(Span::raw("  "));
    }
    tab_spans.push(Span::styled(
        "  [Tab] switch tab",
        Style::default().fg(app.theme.border),
    ));

    let feed_count = app.feeds.iter().filter(|f| f.url != FAVORITES_URL).count();
    let total_articles: usize = app.feeds.iter().map(|f| f.articles.len()).sum();
    let total_unread: usize = app.feeds.iter().map(|f| f.unread_count).sum();
    let stats = ListItem::new(Line::from(vec![
        Span::styled("Feeds: ", Style::default().fg(app.theme.muted)),
        Span::styled(
            feed_count.to_string(),
            Style::default().fg(app.theme.warning),
        ),
        Span::styled("  Total: ", Style::default().fg(app.theme.muted)),
        Span::styled(
            total_articles.to_string(),
            Style::default().fg(app.theme.warning),
        ),
        Span::styled("  Unread: ", Style::default().fg(app.theme.muted)),
        Span::styled(
            total_unread.to_string(),
            Style::default().fg(app.theme.warning),
        ),
        Span::raw(" "),
    ]));
    let stats_width = stats.width() as u16;

    let block = content_block("", false, app.user_data.border_rounded, &app.theme);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(stats_width)])
        .split(inner);

    f.render_widget(
        Paragraph::new(Line::from(tab_spans)).bg(app.theme.bg),
        cols[0],
    );
    f.render_widget(List::new([stats]).bg(app.theme.bg), cols[1]);
}

/// Renders a progress bar showing feed fetch completion (done/total).
pub(super) fn draw_progress_bar(f: &mut Frame, app: &App, area: Rect) {
    let done = app.feeds_total.saturating_sub(app.feeds_pending);
    let counter = format!(" {}/{} ", done, app.feeds_total);
    let counter_width = counter.len() as u16;

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(counter_width)])
        .split(area);

    let bar_width = cols[0].width as usize;
    let filled = (bar_width * done)
        .checked_div(app.feeds_total)
        .unwrap_or(0)
        .min(bar_width);
    let unfilled = bar_width.saturating_sub(filled);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("━".repeat(filled), Style::default().fg(app.theme.warning)),
            Span::styled("─".repeat(unfilled), Style::default().fg(app.theme.border)),
        ]))
        .bg(app.theme.bg),
        cols[0],
    );
    f.render_widget(
        Paragraph::new(counter)
            .style(Style::default().fg(app.theme.muted))
            .bg(app.theme.bg),
        cols[1],
    );
}

/// Renders the bottom footer showing context-sensitive key hints and a scrolling status message.
pub(super) fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let hints = match app.state {
        AppState::ArticleDetail => {
            " [↑/↓] Scroll   [m] Read   [s] Save   [C] Copy   [Esc] Back   [q] Quit "
        }
        AppState::ArticleList => {
            " [↑/↓] Navigate   [Enter] Open   [m] Read   [s] Save   [O] Open   [C] Copy   [Esc] Back   [q] Quit "
        }
        AppState::SettingsList => {
            " [↑/↓] Navigate   [Enter] Select   [Tab/Shift+Tab] Switch Tab   [Esc] Back   [q] Quit "
        }
        AppState::AddFeed | AppState::OPMLExportPath | AppState::OPMLImportPath => {
            " [Enter] Confirm   [Esc] Cancel "
        }
        AppState::ClearData | AppState::ClearArticleCache => " [Enter] Confirm   [Esc] Cancel ",
        AppState::SavedCategoryList => {
            " [↑/↓] Navigate   [Enter] Open   [Tab/Shift+Tab] Switch Tab   [q] Quit "
        }
        AppState::FeedEditor => {
            " [↑/↓] Navigate   [Tab] Switch Panel   [Enter] Toggle   [Space] Move   [a] Add Feed   [n] New Category   [r] Rename   [u] URL   [d] Delete   [Esc] Back "
        }
        AppState::FeedEditorRename => " [Enter] Confirm   [Esc] Cancel ",
        AppState::CategoryPicker => " [↑/↓] Navigate   [Enter] Select   [Esc] Cancel ",
        AppState::SavedCategoryEditor => " [↑/↓] Navigate   [r] Rename   [d] Delete   [Esc] Back ",
        AppState::SavedCategoryEditorRename | AppState::SavedCategoryEditorNew => {
            " [Enter] Confirm   [Esc] Cancel "
        }
        AppState::SavedCategoryEditorDeleteConfirm => " [Enter] Confirm   [Esc] Cancel ",
        AppState::FeedList => {
            " [↑/↓] Navigate   [Space] Expand   [Enter] Open   [r] Refresh   [R] Fetch All   [e] Edit   [Tab/Shift+Tab] Switch Tab   [q] Quit "
        }
        AppState::Changelog => " [↑/↓] Scroll   [Tab/Shift+Tab] Switch Tab   [q] Quit ",
        AppState::ThemeEditor => {
            " [↑/↓] Navigate   [Enter] Select   [n] New   [e] Edit   [r] Rename   [d] Delete   [x] Export   [i] Import   [q] Back "
        }
        AppState::ThemeEditorNew => " [↑/↓] Navigate   [Enter] Clone from   [Esc] Cancel ",
        AppState::ThemeEditorColorEdit => " [↑/↓] Navigate   [Enter] Edit slot   [s/Esc] Back ",
        AppState::ThemeEditorHexInput
        | AppState::ThemeEditorRename
        | AppState::ThemeEditorExport
        | AppState::ThemeEditorImport => " [Enter] Confirm   [Esc] Cancel ",
    };

    let block = content_block("", false, app.user_data.border_rounded, &app.theme);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let hints_width = hints.len() as u16;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(hints_width)])
        .split(inner);

    // Scrolling status: split on first ": " → static prefix + scrolling body.
    let status = if !app.status_msg.is_empty() {
        let status_width = cols[0].width as usize;
        let (prefix, body) = if let Some(pos) = app.status_msg.find(": ") {
            let (p, b) = app.status_msg.split_at(pos + 2);
            (format!(" {p}"), b.to_string())
        } else {
            (" ".to_string(), app.status_msg.clone())
        };
        let prefix_len = prefix.chars().count();
        let body_chars: Vec<char> = body.chars().collect();
        let body_len = body_chars.len();
        // Reserve 1 char on each side for padding.
        let viewport = status_width.saturating_sub(prefix_len + 1);

        if body_len <= viewport {
            Span::styled(
                format!("{prefix}{body} "),
                Style::default().fg(app.theme.success),
            )
        } else {
            // Scroll 1 char per tick (~250 ms), stop at end.
            let max_offset = body_len.saturating_sub(viewport);
            let elapsed = app.tick.saturating_sub(app.status_msg_start_tick);
            let start = elapsed.min(max_offset);
            let visible: String = body_chars[start..].iter().take(viewport).collect();
            Span::styled(
                format!("{prefix}{visible} "),
                Style::default().fg(app.theme.success),
            )
        }
    } else {
        Span::raw("")
    };

    f.render_widget(
        Paragraph::new(Line::from(vec![status])).bg(app.theme.bg),
        cols[0],
    );

    f.render_widget(
        Paragraph::new(hints)
            .style(Style::default().fg(app.theme.muted))
            .alignment(Alignment::Right)
            .bg(app.theme.bg),
        cols[1],
    );
}
