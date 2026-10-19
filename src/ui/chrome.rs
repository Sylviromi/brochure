//! Shared chrome rendering: the tab bar (tabs, `?` hint, status message or totals)
//! and the fetch progress bar.

use crate::{
    app::App,
    models::{FAVORITES_URL, Tab},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::content_block;

/// Renders the top tab bar showing the currently selected tab, feed/article counts, and help text.
pub(super) fn draw_tab_bar(f: &mut Frame, app: &mut App, area: Rect) {
    let tabs = [
        (" Feeds ", Tab::Feeds),
        (" Saved ", Tab::Saved),
        (" Settings ", Tab::Settings),
        (" Changelog ", Tab::Changelog),
    ];

    let mut tab_spans: Vec<Span> = vec![
        " Brochure "
            .fg(app.theme.contrast_color(app.theme.accent))
            .bg(app.theme.accent)
            .bold(),
        Span::raw("  "),
    ];
    for (label, tab) in &tabs {
        if app.selected_tab == *tab {
            tab_spans.push(
                (*label)
                    .fg(app.theme.contrast_color(app.theme.accent))
                    .bg(app.theme.accent)
                    .bold(),
            );
        } else {
            tab_spans.push((*label).fg(app.theme.muted));
        }
        tab_spans.push(Span::raw("  "));
    }
    tab_spans.push(HELP_HINT.fg(app.theme.border));
    let tabs_width = Line::from(tab_spans.clone()).width() as u16;

    let block = content_block("", false, app.user_data.border_rounded, &app.theme);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Right side: the latest status message while it's fresh, otherwise totals.
    let right = if status_is_fresh(app) {
        let width = inner.width.saturating_sub(tabs_width + 2) as usize;
        status_line(app, width)
    } else {
        stats_line(app)
    };
    let right_width = right.width() as u16;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(right_width)])
        .split(inner);

    // Record each tab label's cells for mouse clicks (spans are laid out left to right).
    let mut x = cols[0].x;
    for span in &tab_spans {
        let width = span.width() as u16;
        let rect = Rect::new(x, cols[0].y, width, 1).intersection(cols[0]);
        if let Some(&(_, tab)) = tabs.iter().find(|(label, _)| *label == span.content) {
            app.hit.tabs.push((rect, tab));
        } else if span.content == HELP_HINT {
            app.hit.help_hint = rect;
        }
        x = x.saturating_add(width);
    }
    f.render_widget(
        Paragraph::new(Line::from(tab_spans)).bg(app.theme.bg),
        cols[0],
    );
    f.render_widget(Paragraph::new(right).bg(app.theme.bg), cols[1]);
}

/// Tab-bar hint that opens the keybindings popup (also clickable).
const HELP_HINT: &str = "[?] keys";

/// How long (in ~250 ms ticks) a status message stays in the tab bar.
const STATUS_TICKS: usize = 40;

fn status_is_fresh(app: &App) -> bool {
    !app.status_msg.is_empty() && app.tick.saturating_sub(app.status_msg_start_tick) < STATUS_TICKS
}

/// `Feeds: N  Total: N  Unread: N` totals.
fn stats_line(app: &App) -> Line<'static> {
    let theme = &app.theme;
    let feed_count = app.feeds.iter().filter(|f| f.url != FAVORITES_URL).count();
    let total_articles: usize = app.feeds.iter().map(|f| f.articles.len()).sum();
    let total_unread: usize = app.feeds.iter().map(|f| f.unread_count).sum();
    Line::from(vec![
        "Feeds: ".fg(theme.muted),
        feed_count.to_string().fg(theme.warning),
        "  Total: ".fg(theme.muted),
        total_articles.to_string().fg(theme.warning),
        "  Unread: ".fg(theme.muted),
        total_unread.to_string().fg(theme.warning),
        Span::raw(" "),
    ])
}

/// The status message fitted to `width` columns. Text after the first `": "` scrolls
/// one character per tick when too long; the prefix (e.g. a feed name) stays put.
fn status_line(app: &App, width: usize) -> Line<'static> {
    let msg = &app.status_msg;
    let (prefix, body) = match msg.find(": ") {
        Some(pos) => msg.split_at(pos + 2),
        None => ("", msg.as_str()),
    };
    let color = if msg.starts_with('⚠') {
        app.theme.warning
    } else {
        app.theme.success
    };
    let prefix: String = prefix.chars().take(width.saturating_sub(4)).collect();
    let viewport = width.saturating_sub(prefix.chars().count() + 1);
    let body_chars: Vec<char> = body.chars().collect();
    let max_offset = body_chars.len().saturating_sub(viewport);
    let start = app
        .tick
        .saturating_sub(app.status_msg_start_tick)
        .min(max_offset);
    let visible: String = body_chars[start..].iter().take(viewport).collect();
    Line::from(format!("{prefix}{visible} ").fg(color))
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
            "━".repeat(filled).fg(app.theme.warning),
            "─".repeat(unfilled).fg(app.theme.border),
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
