//! Feed/category tree sidebar rendering.

use crate::{
    app::{App, sidebar_tree_items},
    models::{AppState, FeedTreeItem},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::ListItem,
};

use super::super::{
    SPINNER_FRAMES, content_block, render_scrollable_list, tree_connector, tree_indent,
};
use super::utils::{scroll_title, truncate_title};

/// Renders the feeds sidebar showing categories and feeds in a tree with unread badges and fetch status.
pub(super) fn draw_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let tree = sidebar_tree_items(&app.categories, &app.feeds, &app.sidebar_collapsed);
    let is_navigating = app.state == AppState::FeedList;
    let cursor = app.sidebar_cursor;

    let block = content_block(
        " Feeds ".fg(app.theme.link).bold(),
        is_navigating,
        app.user_data.border_rounded,
        &app.theme,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split inner area to place progress bar at the bottom when fetching.
    let (list_area, maybe_progress) = if app.feeds_pending > 0 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(inner);
        (split[0], Some(split[1]))
    } else {
        (inner, None)
    };

    let items: Vec<ListItem> = tree
        .iter()
        .enumerate()
        .map(|(render_idx, item)| {
            let selected = cursor == render_idx;
            match item {
                FeedTreeItem::AllFeeds => {
                    let total_unread: usize = app
                        .feeds
                        .iter()
                        .filter(|f| f.url != crate::models::FAVORITES_URL)
                        .map(|f| f.unread_count)
                        .sum();
                    let style = if selected {
                        Style::default()
                            .fg(app.theme.warning)
                            .bg(app.theme.selection)
                            .bold()
                    } else {
                        Style::default().fg(app.theme.text)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled("🞴 All Feeds ", style),
                        format!("[{total_unread}]").fg(app.theme.muted),
                    ]))
                }
                FeedTreeItem::Category {
                    id,
                    depth,
                    collapsed,
                } => {
                    let cat_colors = app.theme.category_colors();
                    let color = cat_colors[(id % cat_colors.len() as u64) as usize];
                    let arrow = if *collapsed { " ▶" } else { " ▼" };
                    let cat_name = app
                        .categories
                        .iter()
                        .find(|c| c.id == *id)
                        .map(|c| c.name.as_str())
                        .unwrap_or("?");
                    let indent = tree_indent(&tree, render_idx, *depth);
                    let connector =
                        tree_connector(&tree, render_idx, *depth, app.user_data.border_rounded, "");
                    let style = if selected {
                        Style::default()
                            .fg(app.theme.contrast_color(color))
                            .bg(color)
                            .bold()
                    } else {
                        Style::default().fg(color).bold()
                    };
                    let connector_style = if selected {
                        Style::default().fg(color).bg(app.theme.border)
                    } else {
                        Style::default().fg(app.theme.border)
                    };
                    ListItem::new(Line::from(vec![
                        indent.fg(app.theme.border),
                        Span::styled(connector, connector_style),
                        Span::styled(cat_name, style),
                        Span::styled(arrow, style),
                    ]))
                }
                FeedTreeItem::Feed { feeds_idx, depth } => {
                    let feed = &app.feeds[*feeds_idx];
                    let indent = tree_indent(&tree, render_idx, *depth);
                    let connector =
                        tree_connector(&tree, render_idx, *depth, app.user_data.border_rounded, "");
                    let count_str = feed.unread_badge();
                    let style = if selected {
                        Style::default()
                            .fg(app.theme.accent)
                            .bg(app.theme.selection)
                            .bold()
                    } else {
                        Style::default().fg(app.theme.text)
                    };
                    let connector_style = if selected {
                        Style::default()
                            .fg(app.theme.accent)
                            .bg(app.theme.selection)
                    } else {
                        Style::default().fg(app.theme.border)
                    };
                    // For the selected feed, scroll the title if it overflows.
                    let title_available = (list_area.width as usize).saturating_sub(
                        indent.chars().count()
                            + connector.chars().count()
                            + count_str.chars().count()
                            + 2,
                    );
                    let displayed_title = if selected {
                        let elapsed = app.tick.saturating_sub(app.sidebar_title_start_tick);
                        scroll_title(&feed.title, title_available, elapsed)
                    } else {
                        truncate_title(&feed.title, title_available)
                    };
                    let mut spans = vec![
                        indent.fg(app.theme.border),
                        Span::styled(connector, connector_style),
                        Span::styled(displayed_title, style),
                        count_str.fg(app.theme.warning).bold(),
                    ];
                    if !feed.fetched
                        && feed.fetch_error.is_none()
                        && app.state != AppState::ArticleDetail
                    {
                        let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
                        spans.push(format!(" {spinner}").fg(app.theme.warning));
                    } else if feed.fetch_error.is_some() {
                        // ⚠ (red) when feed is empty — broken; ! (yellow) when stale cached data exists.
                        if feed.articles.is_empty() {
                            spans.push(" ⚠".fg(app.theme.error));
                        } else {
                            spans.push(" !".fg(app.theme.warning));
                        }
                    }
                    ListItem::new(Line::from(spans))
                }
            }
        })
        .collect();

    app.sidebar_list_state.select(Some(cursor));
    render_scrollable_list(f, items, list_area, &mut app.sidebar_list_state, &app.theme);

    if let Some(pb) = maybe_progress {
        super::super::chrome::draw_progress_bar(f, app, pb);
    }
}
