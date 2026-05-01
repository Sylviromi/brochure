//! Main content area rendering: three-panel layout (sidebar, article list, article detail) shared by multiple tabs.
//!
//! This module renders the central content area with the feed/category sidebar, article list,
//! article detail view, and footer with article metadata or feed stats.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
        Wrap,
    },
};

use ratatui::prelude::Stylize;

use crate::{
    app::{App, visible_tree_items},
    models::{AppState, Article, FeedTreeItem, Tab},
};

use super::{
    BASE, BLUE, CATEGORY_COLORS, GREEN, MANTLE, MAUVE, RED, SPINNER_FRAMES, SUBTEXT0, SURFACE0,
    TEXT, YELLOW, border_set, editor::draw_feed_editor, tree_connector, tree_indent,
};

/// Formats a Unix timestamp as a human-readable age string (e.g., "42m ago", "2d ago").
fn format_age(secs: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(secs);
    let diff = (now - secs).max(0) as u64;
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

/// Truncate `text` to `max` chars, appending `…` if truncated.
fn truncate_title(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max || max == 0 {
        text.to_string()
    } else {
        let end = max.saturating_sub(1);
        chars[..end].iter().collect::<String>() + "…"
    }
}

/// Scroll `text` to fit within `available` chars, using `elapsed` ticks.
/// Pauses 8 ticks (~2s) before scrolling, then advances 1 char/tick, stops at end.
fn scroll_title(text: &str, available: usize, elapsed: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if len <= available || available == 0 {
        return text.to_string();
    }
    let max_offset = len - available;
    let start = elapsed.saturating_sub(3).min(max_offset);
    chars[start..start + available].iter().collect()
}

/// Returns a color for a timestamp age: green for recent (< 1h), yellow for today, dimmed for older.
fn age_color(secs: i64) -> ratatui::style::Color {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(secs);
    let diff = (now - secs).max(0) as u64;
    if diff < 3600 {
        GREEN
    } else if diff < 86400 {
        YELLOW
    } else {
        SUBTEXT0
    }
}

/// Splits articles into current and archived groups, returning indices.
/// Returns (current_indices, archived_indices, has_archived).
fn split_articles(articles: &[Article]) -> (Vec<usize>, Vec<usize>, bool) {
    let mut current = Vec::new();
    let mut archived = Vec::new();
    for (i, article) in articles.iter().enumerate() {
        if article.is_archived {
            archived.push(i);
        } else {
            current.push(i);
        }
    }
    let has_archived = !archived.is_empty();
    (current, archived, has_archived)
}

/// Renders a three-panel layout: article list (left) and article detail (right).
///
/// A single shared footer bar is carved from the bottom of `right_area` and rendered
/// after both panels, spanning the full combined width.
fn draw_three_panel(f: &mut Frame, app: &mut App, right_area: Rect, is_preview: bool) {
    // Carve shared footer from the bottom before splitting into panels.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(right_area);
    let panels_area = rows[0];
    let footer_area = rows[1];

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(33), Constraint::Percentage(67)])
        .split(panels_area);

    draw_article_list(f, app, panels[0], false);
    draw_article_detail(f, app, panels[1], is_preview, false);

    // Shared footer: article info when viewing detail, feed stats otherwise.
    draw_article_footer(f, app, footer_area, !is_preview);
}

/// Renders the Feeds tab with sidebar (categories/feeds) and content area (list or detail).
pub(super) fn draw_feeds_tab(f: &mut Frame, app: &mut App, area: Rect) {
    if matches!(app.state, AppState::FeedEditor | AppState::FeedEditorRename)
        || (app.state == AppState::AddFeed && app.add_feed_return_state == AppState::FeedEditor)
    {
        draw_feed_editor(f, app, area);
        return;
    }

    // Outer split: sidebar (25%) | right area (75%)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(area);

    draw_sidebar(f, app, cols[0]);

    match app.state {
        AppState::FeedList | AppState::AddFeed => {
            if app.selected_sidebar_category.is_some() {
                draw_category_article_list(f, app, cols[1]);
            } else {
                draw_article_list(f, app, cols[1], true);
            }
        }
        AppState::ArticleList => {
            draw_three_panel(f, app, cols[1], true);
        }
        AppState::ArticleDetail => {
            draw_three_panel(f, app, cols[1], false);
        }
        AppState::CategoryPicker => {
            let is_preview = app.category_picker_return_state != AppState::ArticleDetail;
            draw_three_panel(f, app, cols[1], is_preview);
        }
        _ => {}
    }
}

/// Renders the Saved tab with saved-categories sidebar and content area (list or detail).
pub(super) fn draw_saved_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(area);

    draw_saved_sidebar(f, app, cols[0]);

    match app.state {
        AppState::SavedCategoryList => draw_article_list(f, app, cols[1], true),
        AppState::ArticleList => {
            draw_three_panel(f, app, cols[1], true);
        }
        AppState::ArticleDetail => {
            draw_three_panel(f, app, cols[1], false);
        }
        AppState::CategoryPicker => {
            let is_preview = app.category_picker_return_state != AppState::ArticleDetail;
            draw_three_panel(f, app, cols[1], is_preview);
        }
        _ => draw_article_list(f, app, cols[1], true),
    }
}

/// Renders the saved-categories sidebar with "All Saved" entry, categories, and article counts.
fn draw_saved_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let is_navigating = app.state == AppState::SavedCategoryList;
    let rounded = app.user_data.border_rounded;

    let total_saved = app.user_data.saved_articles.len();

    let mut items: Vec<ListItem> = Vec::new();

    // "All Saved" entry (cursor 0)
    let all_style = if app.saved_sidebar_cursor == 0 && is_navigating {
        Style::default().bg(SURFACE0).fg(YELLOW)
    } else {
        Style::default().fg(TEXT)
    };
    items.push(ListItem::new(Line::from(vec![
        Span::styled("★ All Saved ", all_style),
        Span::styled(format!("[{total_saved}]"), Style::default().fg(SUBTEXT0)),
    ])));

    // Separator
    items.push(ListItem::new(Line::from(Span::styled(
        "──────────────",
        Style::default().fg(SURFACE0),
    ))));

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
            Style::default().bg(SURFACE0).fg(MAUVE)
        } else {
            Style::default().fg(TEXT)
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(format!("  {} ", cat.name), style),
            Span::styled(format!("[{count}]"), Style::default().fg(SUBTEXT0)),
        ])));
    }

    // Empty state
    if app.user_data.saved_categories.is_empty() && app.user_data.saved_articles.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "  No saved articles",
            Style::default().fg(SUBTEXT0),
        ))));
    }

    let block = Block::default()
        .title(" Saved ")
        .borders(Borders::ALL)
        .border_set(border_set(rounded))
        .border_style(if is_navigating {
            Style::default().fg(MAUVE)
        } else {
            Style::default().fg(SURFACE0)
        })
        .bg(BASE);

    let list = List::new(items).block(block);
    app.saved_sidebar_list_state
        .select(Some(app.saved_sidebar_cursor));
    f.render_stateful_widget(list, area, &mut app.saved_sidebar_list_state);
}

/// Renders the feeds sidebar showing categories and feeds in a tree with unread badges and fetch status.
pub(super) fn draw_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    let tree = visible_tree_items(&app.categories, &app.feeds, &app.sidebar_collapsed);
    let is_navigating = app.state == AppState::FeedList;
    let cursor = app.sidebar_cursor;

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if is_navigating { MAUVE } else { SURFACE0 }))
        .bg(BASE)
        .title(Span::styled(
            " Feeds ",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ));

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
                FeedTreeItem::Category {
                    id,
                    depth,
                    collapsed,
                } => {
                    let color = CATEGORY_COLORS[(id % CATEGORY_COLORS.len() as u64) as usize];
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
                            .fg(MANTLE)
                            .bg(color)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(color).add_modifier(Modifier::BOLD)
                    };
                    let connector_style = if selected {
                        Style::default().fg(color).bg(SURFACE0)
                    } else {
                        Style::default().fg(SURFACE0)
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(indent, Style::default().fg(SURFACE0)),
                        Span::styled(connector, connector_style),
                        Span::styled(cat_name, style),
                        Span::styled(arrow, style),
                    ]))
                }
                FeedTreeItem::Feed { feeds_idx, depth } => {
                    let feed = &app.feeds[*feeds_idx];
                    let indent = tree_indent(&tree, render_idx, *depth);
                    let connector = tree_connector(
                        &tree,
                        render_idx,
                        *depth,
                        app.user_data.border_rounded,
                        "   ",
                    );
                    let count_str = feed.unread_badge();
                    let style = if selected {
                        Style::default()
                            .fg(MAUVE)
                            .bg(SURFACE0)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(TEXT)
                    };
                    let connector_style = if selected {
                        Style::default().fg(MAUVE).bg(SURFACE0)
                    } else {
                        Style::default().fg(SURFACE0)
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
                        Span::styled(indent, Style::default().fg(SURFACE0)),
                        Span::styled(connector, connector_style),
                        Span::styled(displayed_title, style),
                        Span::styled(
                            count_str,
                            Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
                        ),
                    ];
                    if !feed.fetched
                        && feed.fetch_error.is_none()
                        && app.state != AppState::ArticleDetail
                    {
                        let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
                        spans.push(Span::styled(
                            format!(" {spinner}"),
                            Style::default().fg(YELLOW),
                        ));
                    } else if feed.fetch_error.is_some() {
                        // ⚠ (red) when feed is empty — broken; ! (yellow) when stale cached data exists.
                        if feed.articles.is_empty() {
                            spans.push(Span::styled(" ⚠", Style::default().fg(RED)));
                        } else {
                            spans.push(Span::styled(" !", Style::default().fg(YELLOW)));
                        }
                    }
                    ListItem::new(Line::from(spans))
                }
            }
        })
        .collect();

    app.sidebar_list_state.select(Some(cursor));
    let total = tree.len();
    let has_scrollbar = total > list_area.height as usize;
    let list_render_area = if has_scrollbar {
        Rect {
            width: list_area.width.saturating_sub(1),
            ..list_area
        }
    } else {
        list_area
    };
    f.render_stateful_widget(
        List::new(items),
        list_render_area,
        &mut app.sidebar_list_state,
    );

    if has_scrollbar {
        let mut scrollbar_state = ScrollbarState::new(total).position(cursor);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(SURFACE0)),
            list_area,
            &mut scrollbar_state,
        );
    }

    if let Some(pb) = maybe_progress {
        super::chrome::draw_progress_bar(f, app, pb);
    }
}

/// Render the article list panel as a flat date-sorted preview when the sidebar
/// cursor rests on a category header (FeedList state, no navigation cursor).
fn draw_category_article_list(f: &mut Frame, app: &mut App, area: Rect) {
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

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SURFACE0))
        .bg(BASE)
        .title(Span::styled(
            format!(" {} ", cat_name),
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(content_area);
    f.render_widget(block, content_area);

    if app.category_view_articles.is_empty() {
        f.render_widget(
            Paragraph::new(" No articles in this category.").style(Style::default().fg(SUBTEXT0)),
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
            let article = &app.feeds[fi].articles[ai];
            let style = if article.is_read {
                Style::default().fg(SUBTEXT0)
            } else {
                Style::default().fg(TEXT)
            };

            // Compute short age string (e.g. " 42m", " 2h", " 3d") for display.
            let age_str: Option<String> = article.published_secs.map(|secs| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(secs);
                let diff = (now - secs).max(0) as u64;
                if diff < 60 {
                    " now".to_string()
                } else if diff < 3600 {
                    format!(" {}m", diff / 60)
                } else if diff < 86400 {
                    format!(" {}h", diff / 3600)
                } else {
                    format!(" {}d", diff / 86400)
                }
            });
            let age_width = age_str.as_ref().map(|s| s.chars().count()).unwrap_or(0);

            // Subtract icon width (2) and age width from available title space.
            let title_available = list_render_area
                .width
                .saturating_sub(2)
                .saturating_sub(age_width as u16) as usize;

            let mut spans = vec![
                Span::styled(article.get_icon(), article.get_icon_style()),
                Span::raw(truncate_title(&article.title, title_available)),
            ];
            if let Some(ref age) = age_str {
                spans.push(Span::styled(
                    age.clone(),
                    Style::default()
                        .fg(age_color(article.published_secs.unwrap()))
                        .add_modifier(Modifier::DIM),
                ));
            }

            ListItem::new(Line::from(spans)).style(style)
        })
        .collect();
    app.article_list_state.select(None);
    f.render_stateful_widget(
        List::new(items),
        list_render_area,
        &mut app.article_list_state,
    );
    if has_scrollbar {
        let mut sb_state = ScrollbarState::new(total).position(0);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(SURFACE0)),
            inner,
            &mut sb_state,
        );
    }

    draw_article_footer(f, app, footer_area, false);
}

/// Renders the article list for the currently selected feed or category.
///
/// When `show_footer` is `false` (called from three-panel mode), the per-panel footer is
/// suppressed; `draw_three_panel` renders a single shared footer instead.
pub(super) fn draw_article_list(f: &mut Frame, app: &mut App, area: Rect, show_footer: bool) {
    let (area, footer_area) = if show_footer {
        let cf = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);
        (cf[0], cf[1])
    } else {
        (area, Rect::default())
    };

    // ── Category context: flat date-sorted list from all feeds in selected category ──
    if app.in_category_context {
        // Derive the category name from the sidebar selection for the panel title.
        let cat_name: String = app
            .selected_sidebar_category
            .and_then(|id| app.categories.iter().find(|c| c.id == id))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "Category".to_string());

        let is_navigating = app.state == AppState::ArticleList;
        let block = Block::default()
            .border_set(border_set(app.user_data.border_rounded))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if is_navigating { MAUVE } else { SURFACE0 }))
            .bg(BASE)
            .title(Span::styled(
                format!(" {} ", cat_name),
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if app.category_view_articles.is_empty() {
            f.render_widget(
                Paragraph::new(" No articles in this category.")
                    .style(Style::default().fg(SUBTEXT0)),
                inner,
            );
            if show_footer {
                draw_article_footer(f, app, footer_area, false);
            }
            return;
        }

        let items: Vec<ListItem> = app
            .category_view_articles
            .iter()
            .enumerate()
            .map(|(i, &(fi, ai))| {
                let article = &app.feeds[fi].articles[ai];
                let is_selected = app.selected_article == i && app.state == AppState::ArticleList;
                let style = if is_selected {
                    Style::default()
                        .fg(MAUVE)
                        .bg(SURFACE0)
                        .add_modifier(Modifier::BOLD)
                } else if article.is_read {
                    Style::default().fg(SUBTEXT0)
                } else {
                    Style::default().fg(TEXT)
                };
                let title_available = (inner.width as usize).saturating_sub(2);
                let displayed_title = if is_selected {
                    let elapsed = app.tick.saturating_sub(app.article_title_start_tick);
                    scroll_title(&article.title, title_available, elapsed)
                } else {
                    article.title.clone()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(article.get_icon(), article.get_icon_style()),
                    Span::raw(displayed_title),
                ]))
                .style(style)
            })
            .collect();

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
        app.article_list_state.select(Some(app.selected_article));
        f.render_stateful_widget(
            List::new(items),
            list_render_area,
            &mut app.article_list_state,
        );
        if has_scrollbar {
            let mut sb_state = ScrollbarState::new(total).position(app.selected_article);
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .style(Style::default().fg(SURFACE0)),
                inner,
                &mut sb_state,
            );
        }
        if show_footer {
            draw_article_footer(f, app, footer_area, false);
        }
        return;
    }

    // In the Saved tab but no category is selected (cursor on a category or nothing).
    if app.selected_tab == Tab::Saved && !app.in_saved_context {
        let block = Block::default()
            .border_set(border_set(app.user_data.border_rounded))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SURFACE0))
            .bg(BASE)
            .title(Span::styled(
                " ★ Saved ",
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(" Select a category to view saved articles.")
                .style(Style::default().fg(SUBTEXT0)),
            inner,
        );
        if show_footer {
            draw_article_footer(f, app, footer_area, false);
        }
        return;
    }

    let (feed_title, articles): (String, &[crate::models::Article]) = if app.in_saved_context {
        let title = if let Some(cat_id) = app.selected_saved_category {
            app.user_data
                .saved_categories
                .iter()
                .find(|c| c.id == cat_id)
                .map(|c| format!(" {} ", c.name))
                .unwrap_or_else(|| " Saved ".to_string())
        } else {
            " ★ All Saved ".to_string()
        };
        (title, app.saved_view_articles.as_slice())
    } else {
        let feed = app.feeds.get(app.selected_feed);
        let title = feed
            .map(|f| format!(" Articles: {} ", f.title))
            .unwrap_or_else(|| " Articles ".to_string());
        let arts = feed.map(|f| f.articles.as_slice()).unwrap_or(&[]);
        (title, arts)
    };

    let is_navigating = app.state == AppState::ArticleList;
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if is_navigating { MAUVE } else { SURFACE0 }))
        .bg(BASE)
        .title(Span::styled(
            feed_title,
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let list_area = inner;

    if articles.is_empty() {
        if !app.in_saved_context
            && let Some(feed) = app.feeds.get(app.selected_feed)
            && let Some(err) = &feed.fetch_error
        {
            let text = Line::from(vec![
                Span::styled(" ⚠ ", Style::default().fg(RED)),
                Span::styled(err.clone(), Style::default().fg(TEXT)),
            ]);
            f.render_widget(
                Paragraph::new(vec![text]).wrap(Wrap { trim: false }),
                list_area,
            );
            if show_footer {
                draw_article_footer(f, app, footer_area, false);
            }
            return;
        }
        f.render_widget(
            Paragraph::new(" No articles found or fetching...")
                .style(Style::default().fg(SUBTEXT0)),
            list_area,
        );
        if show_footer {
            draw_article_footer(f, app, footer_area, false);
        }
        return;
    }

    let (current_indices, archived_indices, has_archived) = split_articles(articles);

    // Build list items: current articles + optional separator + archived articles
    let mut items: Vec<ListItem> = Vec::new();

    // Add current articles
    for &i in &current_indices {
        let article = &articles[i];
        let is_selected = app.selected_article == i
            && (app.state == AppState::ArticleList || app.in_saved_context);
        let style = if is_selected {
            Style::default()
                .fg(MAUVE)
                .bg(SURFACE0)
                .add_modifier(Modifier::BOLD)
        } else if is_navigating && app.selected_article == i {
            Style::default().fg(MAUVE)
        } else if article.is_read {
            Style::default().fg(SUBTEXT0)
        } else {
            Style::default().fg(TEXT)
        };

        // Compute short age string (e.g. " 42m", " 2h", " 3d") for display next to title.
        let age_str: Option<String> = article.published_secs.map(|secs| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(secs);
            let diff = (now - secs).max(0) as u64;
            if diff < 60 {
                " now".to_string()
            } else if diff < 3600 {
                format!(" {}m", diff / 60)
            } else if diff < 86400 {
                format!(" {}h", diff / 3600)
            } else {
                format!(" {}d", diff / 86400)
            }
        });
        let age_width = age_str.as_ref().map(|s| s.chars().count()).unwrap_or(0);

        // Build title with warning icon if no timestamp
        let mut title_spans = Vec::new();
        if article.published_secs.is_none() {
            title_spans.push(Span::styled("⚠ ", Style::default().fg(YELLOW)));
        }
        let title_available = (list_area.width as usize).saturating_sub(
            2 + age_width
                + if article.published_secs.is_none() {
                    2
                } else {
                    0
                },
        );
        let displayed_title = if is_selected {
            let elapsed = app.tick.saturating_sub(app.article_title_start_tick);
            scroll_title(&article.title, title_available, elapsed)
        } else {
            article.title.clone()
        };
        title_spans.push(Span::raw(displayed_title));
        if let Some(ref age) = age_str {
            title_spans.push(Span::styled(
                age.clone(),
                Style::default()
                    .fg(age_color(article.published_secs.unwrap()))
                    .add_modifier(Modifier::DIM),
            ));
        }

        items.push(
            ListItem::new(Line::from(
                vec![Span::styled(article.get_icon(), article.get_icon_style())]
                    .into_iter()
                    .chain(title_spans)
                    .collect::<Vec<_>>(),
            ))
            .style(style),
        );
    }

    // Add separator if there are archived articles
    if has_archived {
        items.push(ListItem::new(Line::from(Span::styled(
            " ── Archived ──",
            Style::default().fg(SUBTEXT0),
        ))));
    }

    // Add archived articles
    for &i in &archived_indices {
        let article = &articles[i];
        let is_selected = app.selected_article == i
            && (app.state == AppState::ArticleList || app.in_saved_context);
        let style = if is_selected {
            Style::default()
                .fg(MAUVE)
                .bg(SURFACE0)
                .add_modifier(Modifier::BOLD)
        } else if is_navigating && app.selected_article == i {
            Style::default().fg(MAUVE)
        } else if article.is_read {
            Style::default().fg(SUBTEXT0)
        } else {
            Style::default().fg(TEXT)
        };

        // Compute short age string for display next to title.
        let age_str: Option<String> = article.published_secs.map(|secs| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(secs);
            let diff = (now - secs).max(0) as u64;
            if diff < 60 {
                " now".to_string()
            } else if diff < 3600 {
                format!(" {}m", diff / 60)
            } else if diff < 86400 {
                format!(" {}h", diff / 3600)
            } else {
                format!(" {}d", diff / 86400)
            }
        });
        let age_width = age_str.as_ref().map(|s| s.chars().count()).unwrap_or(0);

        // Build title with warning icon if no timestamp
        let mut title_spans = Vec::new();
        if article.published_secs.is_none() {
            title_spans.push(Span::styled("⚠ ", Style::default().fg(YELLOW)));
        }
        let title_available = (list_area.width as usize).saturating_sub(
            2 + age_width
                + if article.published_secs.is_none() {
                    2
                } else {
                    0
                },
        );
        let displayed_title = if is_selected {
            let elapsed = app.tick.saturating_sub(app.article_title_start_tick);
            scroll_title(&article.title, title_available, elapsed)
        } else {
            article.title.clone()
        };
        title_spans.push(Span::raw(displayed_title));
        if let Some(ref age) = age_str {
            title_spans.push(Span::styled(
                age.clone(),
                Style::default()
                    .fg(age_color(article.published_secs.unwrap()))
                    .add_modifier(Modifier::DIM),
            ));
        }

        items.push(
            ListItem::new(Line::from(
                vec![Span::styled(article.get_icon(), article.get_icon_style())]
                    .into_iter()
                    .chain(title_spans)
                    .collect::<Vec<_>>(),
            ))
            .style(style),
        );
    }

    // Calculate the visual selection position, accounting for the separator
    let visual_selected =
        if has_archived && app.selected_article >= archived_indices.first().copied().unwrap_or(0) {
            // Article is in the archived section; add 1 for separator
            current_indices.len()
                + 1
                + archived_indices
                    .iter()
                    .position(|&i| i == app.selected_article)
                    .unwrap_or(0)
        } else {
            // Article is in the current section
            current_indices
                .iter()
                .position(|&i| i == app.selected_article)
                .unwrap_or(app.selected_article)
        };

    app.article_list_state.select(Some(visual_selected));
    let total = items.len();
    let has_scrollbar = total > list_area.height as usize;
    let list_render_area = if has_scrollbar {
        Rect {
            width: list_area.width.saturating_sub(1),
            ..list_area
        }
    } else {
        list_area
    };
    f.render_stateful_widget(
        List::new(items),
        list_render_area,
        &mut app.article_list_state,
    );

    if has_scrollbar {
        let mut scrollbar_state = ScrollbarState::new(total).position(visual_selected);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(SURFACE0)),
            list_area,
            &mut scrollbar_state,
        );
    }

    if show_footer {
        draw_article_footer(f, app, footer_area, false);
    }
}

/// Unified footer renderer used by both `draw_article_list` and `draw_article_detail`.
///
/// When `is_article_view` is `true`, renders article info (link, publish date, scroll %).
/// When `is_article_view` is `false`, renders feed stats (counts, fetch age) on a single row.
/// `area` must be exactly 1 row tall.
fn draw_article_footer(f: &mut Frame, app: &App, area: Rect, is_article_view: bool) {
    if is_article_view {
        // ── Article detail footer: link, publish date, scroll % ──
        let article = if app.in_category_context {
            app.category_view_articles
                .get(app.selected_article)
                .and_then(|&(fi, ai)| app.feeds.get(fi).and_then(|f| f.articles.get(ai)))
                .cloned()
        } else if app.in_saved_context {
            app.saved_view_articles.get(app.selected_article).cloned()
        } else {
            app.feeds
                .get(app.selected_feed)
                .and_then(|f| f.articles.get(app.selected_article))
                .cloned()
        };

        let Some(article) = article else { return };

        let mut link_spans = vec![
            Span::raw(" "),
            Span::styled(article.link.clone(), Style::default().fg(SUBTEXT0)),
        ];
        if let Some(secs) = article.published_secs {
            let age = format_age(secs);
            let color = age_color(secs);
            link_spans.push(Span::styled("  •  ", Style::default().fg(SUBTEXT0)));
            if let Some(number_part) = age.strip_suffix(" ago") {
                link_spans.push(Span::styled(
                    number_part.to_string(),
                    Style::default().fg(color),
                ));
                link_spans.push(Span::styled(" ago", Style::default().fg(SUBTEXT0)));
            } else {
                link_spans.push(Span::styled(age, Style::default().fg(color)));
            }
        }

        let scroll_offset = app.article_scroll.get(&article.link);
        let line_count = app.content_line_count.max(1);
        let content_height = app.content_area_height;
        let max_scroll = line_count.saturating_sub(content_height as usize);
        let pct = if max_scroll == 0 {
            100
        } else {
            (scroll_offset as usize * 100 / max_scroll).min(100)
        };
        let pct_str = format!(" {pct}% ");
        let pct_width = pct_str.len() as u16;
        let bar_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(pct_width)])
            .split(area);
        f.render_widget(
            Paragraph::new(Line::from(link_spans)).bg(BASE),
            bar_chunks[0],
        );
        f.render_widget(
            Paragraph::new(pct_str)
                .style(Style::default().fg(YELLOW).add_modifier(Modifier::BOLD))
                .bg(BASE),
            bar_chunks[1],
        );
    } else {
        // ── Feed stats footer: article count, unread, fetch age on a single row ──
        if app.selected_tab == Tab::Saved && !app.in_saved_context {
            f.render_widget(Paragraph::new("").bg(BASE), area);
            return;
        }

        if app.in_category_context || app.selected_sidebar_category.is_some() {
            let article_count = app.category_view_articles.len();
            let unread_count = app
                .category_view_articles
                .iter()
                .filter(|&&(fi, ai)| {
                    app.feeds
                        .get(fi)
                        .and_then(|f| f.articles.get(ai))
                        .is_some_and(|a| !a.is_read)
                })
                .count();
            let unread_color = if unread_count > 0 { YELLOW } else { GREEN };
            let stat_spans = vec![
                Span::styled(" ", Style::default().fg(SUBTEXT0)),
                Span::styled(article_count.to_string(), Style::default().fg(BLUE)),
                Span::styled(" articles  •  ", Style::default().fg(SUBTEXT0)),
                Span::styled(unread_count.to_string(), Style::default().fg(unread_color)),
                Span::styled(" unread", Style::default().fg(SUBTEXT0)),
            ];
            f.render_widget(Paragraph::new(Line::from(stat_spans)).bg(BASE), area);
            return;
        }

        let (articles, last_fetched_secs): (&[crate::models::Article], Option<i64>) =
            if app.in_saved_context {
                (app.saved_view_articles.as_slice(), None)
            } else {
                let feed = app.feeds.get(app.selected_feed);
                let fetched = feed.and_then(|f| f.last_fetched_secs);
                let arts = feed.map(|f| f.articles.as_slice()).unwrap_or(&[]);
                (arts, fetched)
            };

        let article_count = articles.len();
        let unread_count = articles.iter().filter(|a| !a.is_read).count();
        let unread_color = if unread_count > 0 { YELLOW } else { GREEN };
        let mut stat_spans = vec![
            Span::styled(" ", Style::default().fg(SUBTEXT0)),
            Span::styled(article_count.to_string(), Style::default().fg(BLUE)),
            Span::styled(" articles  •  ", Style::default().fg(SUBTEXT0)),
            Span::styled(unread_count.to_string(), Style::default().fg(unread_color)),
            Span::styled(" unread", Style::default().fg(SUBTEXT0)),
        ];
        if let Some(secs) = last_fetched_secs {
            let age = format_age(secs);
            let color = age_color(secs);
            stat_spans.push(Span::styled("  •  fetched ", Style::default().fg(SUBTEXT0)));
            if let Some(number_part) = age.strip_suffix(" ago") {
                stat_spans.push(Span::styled(
                    number_part.to_string(),
                    Style::default().fg(color),
                ));
                stat_spans.push(Span::styled(" ago", Style::default().fg(SUBTEXT0)));
            } else {
                stat_spans.push(Span::styled(age, Style::default().fg(color)));
            }
        }
        f.render_widget(Paragraph::new(Line::from(stat_spans)).bg(BASE), area);
    }
}

/// Renders the article detail view with markdown content, scrolling, and article metadata footer.
///
/// When `show_footer` is `false` (called from three-panel mode), the per-panel footer is
/// suppressed; `draw_three_panel` renders a single shared footer instead.
pub(super) fn draw_article_detail(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    is_preview: bool,
    show_footer: bool,
) {
    let article = if app.in_category_context {
        app.category_view_articles
            .get(app.selected_article)
            .and_then(|&(fi, ai)| app.feeds.get(fi).and_then(|f| f.articles.get(ai)))
            .cloned()
    } else if app.in_saved_context {
        app.saved_view_articles.get(app.selected_article).cloned()
    } else {
        app.feeds
            .get(app.selected_feed)
            .and_then(|f| f.articles.get(app.selected_article))
            .cloned()
    };
    if article.is_none() && is_preview {
        let block = Block::default()
            .border_set(border_set(app.user_data.border_rounded))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SURFACE0))
            .bg(BASE);
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new("Select an article to preview.").style(Style::default().fg(SUBTEXT0)),
            inner,
        );
        return;
    }
    let Some(article) = article else { return };

    // Show spinner only when the article's own feed is actively refreshing.
    let feed_refreshing = !app.in_saved_context
        && !app.in_category_context
        && app.feeds.get(app.selected_feed).is_some_and(|f| !f.fetched);
    let age_suffix: String = if let Some(secs) = article.published_secs {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(secs);
        let diff = (now - secs).max(0) as u64;
        let age = if diff < 60 {
            "now".to_string()
        } else if diff < 3600 {
            format!("{}m", diff / 60)
        } else if diff < 86400 {
            format!("{}h", diff / 3600)
        } else {
            format!("{}d", diff / 86400)
        };
        format!("  • {age}")
    } else {
        String::new()
    };
    let detail_title = if feed_refreshing || app.article_fetching {
        let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
        format!(" {spinner} {}{age_suffix} ", article.title)
    } else {
        format!(" {}{age_suffix} ", article.title)
    };
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if is_preview { SURFACE0 } else { MAUVE }))
        .bg(BASE)
        .title(Span::styled(
            detail_title,
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        ));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let (content_area, bar_area) = if show_footer {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(inner_area);
        (layout[0], layout[1])
    } else {
        (inner_area, Rect::default())
    };

    // Convert images to inline text (![alt](url) → 🖼 alt), then strip hyperlinks.
    let no_images = regex::Regex::new(r"!\[([^\]]*)\]\([^\)]+\)")
        .unwrap()
        .replace_all(&article.content, |caps: &regex::Captures| {
            let alt = caps[1].trim();
            if alt.is_empty() {
                "🖼".to_string()
            } else {
                format!("🖼 {alt}")
            }
        })
        .to_string();
    let stripped = regex::Regex::new(r"\[([^]]+)]\([^)]+\)")
        .unwrap()
        .replace_all(&no_images, "$1")
        .to_string();

    let scroll_offset = app.article_scroll.get(&article.link);

    if !is_preview {
        app.content_area_height = content_area.height;
    }

    // Build the paragraph first so we can call line_count(width) for the true rendered
    // line count (accounts for word-wrap), not just the logical markdown line count.
    let paragraph = Paragraph::new(tui_markdown::from_str(&stripped))
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset, 0));

    let line_count = paragraph.line_count(content_area.width).max(1);
    if !is_preview {
        app.content_line_count = line_count;
    }

    if show_footer {
        draw_article_footer(f, app, bar_area, true);
    }

    let has_scrollbar = line_count > content_area.height as usize;
    let para_render_area = if has_scrollbar {
        Rect {
            width: content_area.width.saturating_sub(1),
            ..content_area
        }
    } else {
        content_area
    };
    f.render_widget(paragraph, para_render_area);

    if has_scrollbar {
        let mut scrollbar_state = ScrollbarState::new(line_count).position(scroll_offset as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(SURFACE0)),
            content_area,
            &mut scrollbar_state,
        );
    }
}
