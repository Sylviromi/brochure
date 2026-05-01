use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use ratatui::prelude::Stylize;

use crate::{
    app::{visible_favorites_tree_items, visible_tree_items, App},
    models::{AppState, FeedTreeItem, Tab},
};

use super::{
    border_set, editor::draw_feed_editor, BASE, BLUE, CATEGORY_COLORS, GREEN, MANTLE, MAUVE, RED, SPINNER_FRAMES,
    SUBTEXT0, SURFACE0, TEXT, YELLOW,
};

/// Format a Unix timestamp (seconds) as a relative age string.
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

/// Compute the leading indent string for a tree item at `depth` positioned at `render_idx`.
/// For each ancestor level (1 to depth-1), emits "│  " if that level still has siblings
/// after the current item, or "   " if it was the last child.
fn tree_indent(tree: &[FeedTreeItem], render_idx: usize, depth: u8) -> String {
    if depth <= 1 {
        return String::new();
    }
    let mut s = String::new();
    for level in 1..depth {
        let next_at_level = tree[render_idx + 1..]
            .iter()
            .find(|n| {
                let d = match n {
                    FeedTreeItem::Feed { depth, .. } | FeedTreeItem::Category { depth, .. } => *depth,
                };
                d <= level
            })
            .map(|n| match n {
                FeedTreeItem::Feed { depth, .. } | FeedTreeItem::Category { depth, .. } => *depth,
            });
        if next_at_level == Some(level) {
            s.push_str("│  ");
        } else {
            s.push_str("   ");
        }
    }
    s
}

/// Color for a Unix timestamp age: green = fresh, yellow = today, dimmed = old.
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

pub(super) fn draw_feeds_tab(f: &mut Frame, app: &mut App, area: Rect) {
    if matches!(app.state, AppState::FeedEditor | AppState::FeedEditorRename) {
        draw_feed_editor(f, app, area);
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(area);

    draw_sidebar(f, app, cols[0], false);

    match app.state {
        AppState::FeedList | AppState::ArticleList | AppState::AddFeed => {
            draw_article_list(f, app, cols[1]);
        }
        AppState::ArticleDetail => draw_article_detail(f, app, cols[1]),
        _ => {}
    }
}

pub(super) fn draw_favorites_tab(f: &mut Frame, app: &mut App, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
        .split(area);

    draw_sidebar(f, app, cols[0], true);

    match app.state {
        AppState::FavoriteFeedList | AppState::ArticleList => draw_article_list(f, app, cols[1]),
        AppState::ArticleDetail => draw_article_detail(f, app, cols[1]),
        _ => draw_article_list(f, app, cols[1]),
    }
}

pub(super) fn draw_sidebar(f: &mut Frame, app: &mut App, area: Rect, is_favorites: bool) {
    let (is_navigating, tree, cursor, title) = if is_favorites {
        let items = visible_favorites_tree_items(
            &app.categories,
            &app.feeds,
            &app.sidebar_collapsed,
            &app.user_data.starred_articles,
        );
        (
            app.state == AppState::FavoriteFeedList,
            items,
            app.favorites_sidebar_cursor,
            " ⭐ Favorites ",
        )
    } else {
        let items = visible_tree_items(&app.categories, &app.feeds, &app.sidebar_collapsed);
        (
            app.state == AppState::FeedList,
            items,
            app.sidebar_cursor,
            " Feeds ",
        )
    };

    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if is_navigating { MAUVE } else { SURFACE0 }))
        .bg(BASE)
        .title(Span::styled(
            title,
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split inner area to place progress bar at the bottom when fetching.
    let (list_area, maybe_progress) = if !is_favorites && app.feeds_pending > 0 {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(inner);
        (split[0], Some(split[1]))
    } else {
        (inner, None)
    };

    if tree.is_empty() && is_favorites {
        f.render_widget(
            Paragraph::new(" No starred articles yet. Press [s] on an article to star it.")
                .style(Style::default().fg(SUBTEXT0)),
            list_area,
        );
        if let Some(pb) = maybe_progress {
            super::chrome::draw_progress_bar(f, app, pb);
        }
        return;
    }

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
                    let style = if selected {
                        Style::default()
                            .fg(MANTLE)
                            .bg(color)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(color).add_modifier(Modifier::BOLD)
                    };
                    // Sub-categories (depth > 0) get tree connectors like feeds.
                    let (indent, connector) = if *depth > 0 {
                        let next_depth = tree
                            .get(render_idx + 1)
                            .map(|n| match n {
                                FeedTreeItem::Feed { depth, .. }
                                | FeedTreeItem::Category { depth, .. } => *depth,
                            })
                            .unwrap_or(0);
                        let conn = if next_depth < *depth {
                            if app.user_data.border_rounded { "╰─ " } else { "└─ " }
                        } else {
                            "├─ "
                        };
                        (tree_indent(&tree, render_idx, *depth), conn)
                    } else {
                        (String::new(), "")
                    };
                    let connector_style = if selected {
                        Style::default().fg(color).bg(SURFACE0)
                    } else {
                        Style::default().fg(SURFACE0)
                    };
                    ListItem::new(Line::from(vec![
                        Span::raw(indent),
                        Span::styled(connector, connector_style),
                        Span::styled(cat_name, style),
                        Span::styled(arrow, style),
                    ]))
                }
                FeedTreeItem::Feed { feeds_idx, depth } => {
                    let feed = &app.feeds[*feeds_idx];
                    let indent = tree_indent(&tree, render_idx, *depth);
                    // Tree connector: ╰─ for last child, ├─ for others; plain indent for depth 0.
                    let connector = if *depth > 0 {
                        let next_depth = tree
                            .get(render_idx + 1)
                            .map(|n| match n {
                                FeedTreeItem::Feed { depth, .. }
                                | FeedTreeItem::Category { depth, .. } => *depth,
                            })
                            .unwrap_or(0);
                        if next_depth < *depth {
                            if app.user_data.border_rounded { "╰─ " } else { "└─ " }
                        } else {
                            "├─ "
                        }
                    } else {
                        "   "
                    };
                    let count_str = if is_favorites {
                        let n = app
                            .user_data
                            .starred_articles
                            .iter()
                            .filter(|a| a.source_feed == feed.title)
                            .count();
                        if n > 0 {
                            format!(" [★{n}]")
                        } else {
                            String::new()
                        }
                    } else {
                        feed.unread_badge()
                    };
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
                    let mut spans = vec![
                        Span::raw(indent),
                        Span::styled(connector, connector_style),
                        Span::styled(feed.title.clone(), style),
                        Span::styled(
                            count_str,
                            Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
                        ),
                    ];
                    if !is_favorites {
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
                    }
                    ListItem::new(Line::from(spans))
                }
            }
        })
        .collect();

    let list_state = if is_favorites {
        &mut app.favorites_sidebar_list_state
    } else {
        &mut app.sidebar_list_state
    };
    list_state.select(if is_navigating { Some(cursor) } else { None });
    f.render_stateful_widget(List::new(items), list_area, list_state);

    if let Some(pb) = maybe_progress {
        super::chrome::draw_progress_bar(f, app, pb);
    }
}

pub(super) fn draw_article_list(f: &mut Frame, app: &mut App, area: Rect) {
    // In the Favorites tab but no feed is selected (cursor on a category or nothing).
    if app.selected_tab == Tab::Favorites && !app.in_favorites_context {
        let block = Block::default()
            .border_set(border_set(app.user_data.border_rounded))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SURFACE0))
            .bg(BASE)
            .title(Span::styled(
                " ⭐ Favorites ",
                Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(" Select a feed to preview its articles.")
                .style(Style::default().fg(SUBTEXT0)),
            inner,
        );
        return;
    }

    let (feed_title, feed_url, feed_updated_secs, articles) = if app.in_favorites_context {
        let title = app
            .favorite_view_articles
            .first()
            .map(|a| format!(" ⭐ {} ", a.source_feed))
            .unwrap_or_else(|| " ⭐ Favorites ".to_string());
        (
            title,
            String::new(),
            None,
            app.favorite_view_articles.as_slice(),
        )
    } else {
        let feed = app.feeds.get(app.selected_feed);
        let title = feed
            .map(|f| format!(" Articles: {} ", f.title))
            .unwrap_or_else(|| " Articles ".to_string());
        let url = feed.map(|f| f.url.clone()).unwrap_or_default();
        let updated = feed.and_then(|f| f.feed_updated_secs);
        let arts = feed.map(|f| f.articles.as_slice()).unwrap_or(&[]);
        (title, url, updated, arts)
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

    // Split inner area: list on top, info bar at bottom.
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(inner);
    let list_area = layout[0];
    let bar_area = layout[1];

    // Info bar: separator + 2 rows (URL, stats).
    let bar_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(SURFACE0))
        .bg(BASE);
    let bar_inner = bar_block.inner(bar_area);
    f.render_widget(bar_block, bar_area);

    let bar_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(bar_inner);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(feed_url, Style::default().fg(SUBTEXT0)),
        ]))
        .bg(BASE),
        bar_rows[0],
    );

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
    if let Some(secs) = feed_updated_secs {
        let age = format_age(secs);
        let color = age_color(secs);
        stat_spans.push(Span::styled("  •  updated ", Style::default().fg(SUBTEXT0)));
        if let Some(number_part) = age.strip_suffix(" ago") {
            stat_spans.push(Span::styled(number_part.to_string(), Style::default().fg(color)));
            stat_spans.push(Span::styled(" ago", Style::default().fg(SUBTEXT0)));
        } else {
            // "just now" — color the whole phrase
            stat_spans.push(Span::styled(age, Style::default().fg(color)));
        }
    }
    f.render_widget(Paragraph::new(Line::from(stat_spans)).bg(BASE), bar_rows[1]);

    if articles.is_empty() {
        if !app.in_favorites_context
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
            return;
        }
        f.render_widget(
            Paragraph::new(" No articles found or fetching...")
                .style(Style::default().fg(SUBTEXT0)),
            list_area,
        );
        return;
    }

    let items: Vec<ListItem> = articles
        .iter()
        .enumerate()
        .map(|(i, article)| {
            let style = if app.selected_article == i
                && (app.state == AppState::ArticleList || app.in_favorites_context)
            {
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

            let read_icon_style = if article.is_read {
                Style::default().fg(SUBTEXT0)
            } else {
                Style::default().fg(BLUE)
            };

            ListItem::new(Line::from(vec![
                Span::styled(article.read_icon(), read_icon_style),
                Span::styled(article.star_icon(), Style::default().fg(YELLOW)),
                Span::raw(article.title.clone()),
            ]))
            .style(style)
        })
        .collect();

    app.article_list_state.select(Some(app.selected_article));
    f.render_stateful_widget(List::new(items), list_area, &mut app.article_list_state);
}

pub(super) fn draw_article_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let article = if app.in_favorites_context {
        app.favorite_view_articles
            .get(app.selected_article)
            .cloned()
    } else {
        app.feeds
            .get(app.selected_feed)
            .and_then(|f| f.articles.get(app.selected_article))
            .cloned()
    };
    let Some(article) = article else { return };

    // Show spinner only when the article's own feed is actively refreshing.
    let feed_refreshing =
        !app.in_favorites_context && app.feeds.get(app.selected_feed).is_some_and(|f| !f.fetched);
    let detail_title = if feed_refreshing {
        let spinner = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
        format!(" {spinner} {} ", article.title)
    } else {
        format!(" {} ", article.title)
    };
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MAUVE))
        .bg(BASE)
        .title(Span::styled(
            detail_title,
            Style::default().fg(MAUVE).add_modifier(Modifier::BOLD),
        ));

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(inner_area);

    let content_area = layout[0];
    let bar_area = layout[1];

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

    app.content_area_height = content_area.height;

    // Build the paragraph first so we can call line_count(width) for the true rendered
    // line count (accounts for word-wrap), not just the logical markdown line count.
    let paragraph = Paragraph::new(tui_markdown::from_str(&stripped))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));
    app.content_line_count = paragraph.line_count(content_area.width).max(1);

    let max_scroll = app
        .content_line_count
        .saturating_sub(app.content_area_height as usize);
    let pct = if max_scroll == 0 {
        100
    } else {
        (app.scroll_offset as usize * 100 / max_scroll).min(100)
    };

    // Bottom bar: separator + link / scroll %
    let bar_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(SURFACE0))
        .bg(BASE);
    let bar_inner = bar_block.inner(bar_area);
    f.render_widget(bar_block, bar_area);

    let pct_str = format!(" {pct}% ");
    let pct_width = pct_str.len() as u16;
    let bar_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(pct_width)])
        .split(bar_inner);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(" "),
            Span::styled(article.link.clone(), Style::default().fg(SUBTEXT0)),
        ]))
        .bg(BASE),
        bar_chunks[0],
    );
    f.render_widget(
        Paragraph::new(pct_str)
            .style(Style::default().fg(YELLOW).add_modifier(Modifier::BOLD))
            .bg(BASE),
        bar_chunks[1],
    );

    f.render_widget(paragraph, content_area);
}
