use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use crate::{
    app::App,
    models::{AppState, FAVORITES_URL, Tab},
};

use ratatui::prelude::Stylize;

use super::{BASE, GREEN, MANTLE, MAUVE, SUBTEXT0, SURFACE0, YELLOW, border_set};

pub(super) fn draw_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let tabs = [
        (" Feeds ", Tab::Feeds),
        (" Saved ", Tab::Saved),
        (" Settings ", Tab::Settings),
    ];

    let mut tab_spans: Vec<Span> = vec![
        Span::styled(
            " Brochure ",
            Style::default()
                .fg(MANTLE)
                .bg(MAUVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
    ];
    for (label, tab) in &tabs {
        if app.selected_tab == *tab {
            tab_spans.push(Span::styled(
                *label,
                Style::default()
                    .fg(MANTLE)
                    .bg(MAUVE)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            tab_spans.push(Span::styled(*label, Style::default().fg(SUBTEXT0)));
        }
        tab_spans.push(Span::raw("  "));
    }
    tab_spans.push(Span::styled(
        "  [Tab] switch tab",
        Style::default().fg(SURFACE0),
    ));

    let feed_count = app.feeds.iter().filter(|f| f.url != FAVORITES_URL).count();
    let total_articles: usize = app.feeds.iter().map(|f| f.articles.len()).sum();
    let total_unread: usize = app.feeds.iter().map(|f| f.unread_count).sum();
    let stats = ListItem::new(Line::from(vec![
        Span::raw("Feeds: "),
        Span::styled(feed_count.to_string(), Style::default().fg(YELLOW)),
        Span::raw("  Total: "),
        Span::styled(total_articles.to_string(), Style::default().fg(YELLOW)),
        Span::raw("  Unread: "),
        Span::styled(total_unread.to_string(), Style::default().fg(YELLOW)),
        Span::raw(" "),
    ]));
    let stats_width = stats.width() as u16;

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SURFACE0))
        .bg(BASE);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(stats_width)])
        .split(inner);

    f.render_widget(Paragraph::new(Line::from(tab_spans)).bg(BASE), cols[0]);
    f.render_widget(List::new([stats]).bg(BASE), cols[1]);
}

pub(super) fn draw_progress_bar(f: &mut Frame, app: &App, area: Rect) {
    let done = app.feeds_total.saturating_sub(app.feeds_pending);
    let counter = format!(" {}/{} ", done, app.feeds_total);
    let counter_width = counter.len() as u16;

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(counter_width)])
        .split(area);

    let bar_width = cols[0].width as usize;
    let filled = if app.feeds_total > 0 {
        (bar_width * done / app.feeds_total).min(bar_width)
    } else {
        0
    };
    let unfilled = bar_width.saturating_sub(filled);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("━".repeat(filled), Style::default().fg(YELLOW)),
            Span::styled("─".repeat(unfilled), Style::default().fg(SURFACE0)),
        ]))
        .bg(BASE),
        cols[0],
    );
    f.render_widget(
        Paragraph::new(counter)
            .style(Style::default().fg(SUBTEXT0))
            .bg(BASE),
        cols[1],
    );
}

pub(super) fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let hints = match app.state {
        AppState::ArticleDetail => " [↑/↓] Scroll   [m] Read   [s] Save   [Esc] Back   [q] Quit ",
        AppState::ArticleList => {
            " [↑/↓] Navigate   [Enter] Open   [m] Read   [s] Save   [Esc] Back   [q] Quit "
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
            " [↑/↓] Navigate   [Tab] Switch Panel   [Enter] Toggle   [Space] Move   [a] Add Feed   [n] New Category   [r] Rename   [d] Delete   [Esc] Back "
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
    };

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SURFACE0))
        .bg(BASE);
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
            Span::styled(format!("{prefix}{body} "), Style::default().fg(GREEN))
        } else {
            // Scroll 1 char per tick (~250 ms), stop at end.
            let max_offset = body_len.saturating_sub(viewport);
            let elapsed = app.tick.saturating_sub(app.status_msg_start_tick);
            let start = elapsed.min(max_offset);
            let visible: String = body_chars[start..].iter().take(viewport).collect();
            Span::styled(format!("{prefix}{visible} "), Style::default().fg(GREEN))
        }
    } else {
        Span::raw("")
    };

    f.render_widget(Paragraph::new(Line::from(vec![status])).bg(BASE), cols[0]);

    f.render_widget(
        Paragraph::new(hints)
            .style(Style::default().fg(SUBTEXT0))
            .alignment(Alignment::Right)
            .bg(BASE),
        cols[1],
    );
}
