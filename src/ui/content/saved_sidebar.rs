//! Saved-categories sidebar rendering.

use crate::{app::App, models::AppState};
use ratatui::{
    Frame,
    layout::Rect,
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem},
};

use super::super::content_block;

/// Renders the saved-categories sidebar with "All Saved" entry, categories, and article counts.
pub(super) fn draw_saved_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let is_navigating = app.state == AppState::SavedCategoryList;

    let total_saved = app.user_data.saved_articles.len();

    let mut items: Vec<ListItem> = Vec::new();

    // "All Saved" entry (cursor 0)
    let all_style = if app.saved_sidebar_cursor == 0 && is_navigating {
        Style::default().bg(app.theme.selection).fg(app.theme.warning)
    } else {
        Style::default().fg(app.theme.text)
    };
    items.push(ListItem::new(Line::from(vec![
        Span::styled("🞴 All Saved ", all_style),
        format!("[{total_saved}]").fg(app.theme.muted),
    ])));

    // Separator
    items.push(ListItem::new(Line::from(
        "──────────────".fg(app.theme.border),
    )));

    // Category entries (cursor 1+)
    for (i, cat) in app.user_data.saved_categories.iter().enumerate() {
        let cursor_pos = i + 1; // +1 for "All Saved"
        let count = app
            .user_data
            .saved_articles
            .iter()
            .filter(|s| s.category_id == cat.id)
            .count();
        let style = if app.saved_sidebar_cursor == cursor_pos && is_navigating {
            Style::default().bg(app.theme.selection).fg(app.theme.accent)
        } else {
            Style::default().fg(app.theme.text)
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("  {} ", cat.name), style),
            format!("[{count}]").fg(app.theme.muted),
        ])));
    }

    // Empty state
    if app.user_data.saved_categories.is_empty() && app.user_data.saved_articles.is_empty() {
        items.push(ListItem::new(Line::from(
            "  No saved articles".fg(app.theme.muted),
        )));
    }

    let block = content_block(
        " Saved ",
        is_navigating,
        app.user_data.border_rounded,
        &app.theme,
    );
    let list = List::new(items).block(block);
    app.saved_sidebar_list_state
        .select(Some(app.saved_sidebar_cursor));
    f.render_stateful_widget(list, area, &mut app.saved_sidebar_list_state);
}
