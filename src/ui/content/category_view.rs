//! Category article list preview rendering (flat date-sorted view when a category header is selected).

use crate::app::App;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    widgets::{List, ListItem, Paragraph},
};

use super::super::{content_block, render_scrollbar};
use super::{article_list::build_article_list_item, footer::draw_article_footer};

/// Render the article list panel as a flat date-sorted preview when the sidebar
/// cursor rests on a category header (FeedList state, no navigation cursor).
pub(super) fn draw_category_article_list(f: &mut Frame, app: &mut App, area: Rect) {
    let cat_id = match app.selected_sidebar_category {
        Some(id) => id,
        None => return,
    };
    let cat_name = app
        .categories
        .iter()
        .find(|c| c.id == cat_id)
        .map(|c| c.name.as_str())
        .unwrap_or("Category");

    // Split area into content (list) and footer (1 row).
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    let content_area = rows[0];
    let footer_area = rows[1];

    let block = content_block(
        format!(" {} ", cat_name).fg(app.theme.link).bold(),
        false,
        app.user_data.border_rounded,
        &app.theme,
    );
    let inner = block.inner(content_area);
    f.render_widget(block, content_area);

    if app.category_view_articles.is_empty() {
        f.render_widget(
            Paragraph::new(" No articles in this category.")
                .style(Style::default().fg(app.theme.muted_text)),
            inner,
        );
        draw_article_footer(f, app, footer_area, false);
        return;
    }

    let total = app.category_view_articles.len();
    let has_scrollbar = total > inner.height as usize;
    let list_render_area = if has_scrollbar {
        Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        }
    } else {
        inner
    };

    let items: Vec<ListItem> = app
        .category_view_articles
        .iter()
        .map(|&(fi, ai)| {
            let Some(article) = app.feeds.get(fi).and_then(|f| f.articles.get(ai)) else {
                return ListItem::new("");
            };
            build_article_list_item(
                article,
                false,
                false,
                list_render_area.width,
                app.tick,
                app.article_title_start_tick,
                &app.theme,
            )
        })
        .collect();
    app.article_list_state.select(None);
    f.render_stateful_widget(
        List::new(items),
        list_render_area,
        &mut app.article_list_state,
    );
    if has_scrollbar {
        let bar_area = Rect {
            x: inner.x + inner.width.saturating_sub(1),
            width: 1,
            ..inner
        };
        render_scrollbar(f, bar_area, total, inner.height as usize, 0, &app.theme);
    }

    draw_article_footer(f, app, footer_area, false);
}
