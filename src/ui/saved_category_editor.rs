//! Full-screen saved-category editor for renaming and deleting custom categories.

use crate::{app::App, models::AppState, ui::content::utils::split_cursor};
use ratatui::{
    Frame,
    layout::Rect,
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, ListItem, Paragraph},
};

use super::{border_set, render_scrollable_list};

/// Renders the full-screen saved-category editor for renaming and deleting custom categories.
pub(super) fn draw_saved_category_editor(f: &mut Frame, app: &mut App, area: Rect) {
    let rounded = app.user_data.border_rounded;
    let block = Block::default()
        .title(" Saved Category Editor  [r] rename  [d] delete  [n] new  [Esc] back ")
        .borders(Borders::ALL)
        .border_set(border_set(rounded))
        .border_style(Style::default().fg(app.theme.accent))
        .style(Style::default().bg(app.theme.bg));

    f.render_widget(block.clone(), area);
    let inner = block.inner(area);

    if app.user_data.saved_categories.is_empty() {
        let msg = Paragraph::new("  No categories yet. Save an article with [s] to create one.")
            .style(Style::default().fg(app.theme.muted));
        f.render_widget(msg, inner);
        return;
    }

    let items: Vec<ListItem> = app
        .user_data
        .saved_categories
        .iter()
        .enumerate()
        .map(|(i, cat)| {
            let count = app
                .user_data
                .saved_articles
                .iter()
                .filter(|s| s.category_id == cat.id)
                .count();

            let name_line = if app.state == AppState::SavedCategoryEditorRename
                && i == app.saved_cat_editor_scroll.cursor
            {
                let (before, cursor_ch, after) =
                    split_cursor(&app.feed_editor.input, app.feed_editor.input_cursor);
                Line::from(vec![
                    "  ".fg(app.theme.warning),
                    before.fg(app.theme.warning),
                    cursor_ch.fg(app.theme.accent).bold(),
                    after.fg(app.theme.warning),
                ])
            } else {
                let style = if i == app.saved_cat_editor_scroll.cursor {
                    Style::default().bg(app.theme.selection).fg(app.theme.accent)
                } else {
                    Style::default().fg(app.theme.text)
                };
                Line::from(Span::styled(format!("  {}", cat.name), style))
            };

            let count_span = Span::styled(
                format!("  [{count} article{}]", if count == 1 { "" } else { "s" }),
                Style::default().fg(app.theme.muted),
            );

            let mut spans = name_line.spans;
            spans.push(count_span);
            ListItem::new(Line::from(spans))
        })
        .collect();

    render_scrollable_list(
        f,
        items,
        inner,
        &mut app.saved_cat_editor_scroll.list_state,
        &app.theme,
    );

    // Render input row when creating a new category.
    if app.state == AppState::SavedCategoryEditorNew {
        let input_area = Rect {
            y: area.bottom().saturating_sub(2),
            height: 1,
            ..area
        };
        let (before, cursor_ch, after) =
            split_cursor(&app.feed_editor.input, app.feed_editor.input_cursor);
        let input_line = Line::from(vec![
            "  Category name: ".fg(app.theme.warning),
            before.fg(app.theme.warning),
            cursor_ch.fg(app.theme.accent).bold(),
            after.fg(app.theme.warning),
        ]);
        f.render_widget(Paragraph::new(input_line), input_area);
    }
}
