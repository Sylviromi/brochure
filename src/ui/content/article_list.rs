//! Article list panel rendering with archived sections and scrolling.

use crate::{
    app::App,
    models::{AppState, Article, Tab},
};
use ratatui::{
    Frame,
    layout::Rect,
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::{ListItem, Paragraph, Wrap},
};

use super::super::{content_block, render_scrollable_list};
use super::{
    footer::draw_article_footer,
    helpers::split_articles,
    utils::{age_color, scroll_title, short_age, truncate_title},
};

/// Builds a single article list item with icon, title (scrolling if selected), and age badge.
fn build_article_list_item(
    article: &Article,
    is_selected: bool,
    is_nav_highlight: bool,
    list_width: u16,
    tick: usize,
    article_title_start_tick: usize,
    theme: &crate::ui::theme::ColorTheme,
) -> ListItem<'static> {
    let style = if is_selected {
        Style::default().fg(theme.accent).bg(theme.border).bold()
    } else if is_nav_highlight {
        Style::default().fg(theme.accent)
    } else if article.is_read {
        Style::default().fg(theme.muted_text)
    } else {
        Style::default().fg(theme.text)
    };

    let age_str: Option<String> = article.published_secs.map(short_age);
    let age_width = age_str.as_ref().map(|s| s.chars().count()).unwrap_or(0);

    let mut title_spans: Vec<Span> = Vec::new();
    if article.published_secs.is_none() {
        title_spans.push("⚠ ".fg(theme.unread));
    }
    let title_available = (list_width as usize).saturating_sub(
        2 + age_width
            + if article.published_secs.is_none() {
                2
            } else {
                0
            },
    );
    if is_selected {
        let elapsed = tick.saturating_sub(article_title_start_tick);
        title_spans.push(Span::raw(scroll_title(
            &article.title,
            title_available,
            elapsed,
        )));
    } else {
        title_spans.push(Span::raw(truncate_title(&article.title, title_available)));
    }
    if let Some(ref age) = age_str {
        title_spans.push(
            age.clone()
                .fg(age_color(article.published_secs.unwrap(), theme))
                .dim(),
        );
    }

    ListItem::new(Line::from(
        vec![Span::styled(
            article.get_icon(),
            article.get_icon_style(theme.unread, theme.muted_text, theme.link),
        )]
        .into_iter()
        .chain(title_spans)
        .collect::<Vec<_>>(),
    ))
    .style(style)
}

/// Renders the article list for the currently selected feed or category.
///
/// When `show_footer` is `false` (called from three-panel mode), the per-panel footer is
/// suppressed; `draw_three_panel` renders a single shared footer instead.
pub(super) fn draw_article_list(f: &mut Frame, app: &mut App, area: Rect, show_footer: bool) {
    let (area, footer_area) = if show_footer {
        let cf = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                ratatui::layout::Constraint::Min(0),
                ratatui::layout::Constraint::Length(1),
            ])
            .split(area);
        (cf[0], cf[1])
    } else {
        (area, Rect::default())
    };

    // ── Category context: flat date-sorted list from all feeds in selected category ──
    if app.in_category_context {
        // Derive the category name from the sidebar selection for the panel title.
        let cat_name: String = if app.in_all_feeds_context {
            "🞴 All Feeds".to_string()
        } else {
            app.selected_sidebar_category
                .and_then(|id| app.categories.iter().find(|c| c.id == id))
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Category".to_string())
        };

        let is_navigating = matches!(app.state, AppState::ArticleList);
        let block = content_block(
            format!(" {} ", cat_name).fg(app.theme.link).bold(),
            is_navigating,
            app.user_data.border_rounded,
            &app.theme,
        );
        let inner = block.inner(area);
        f.render_widget(block, area);

        if app.category_view_articles.is_empty() {
            f.render_widget(
                Paragraph::new(" No articles in this category.")
                    .style(Style::default().fg(app.theme.muted_text)),
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
                let Some(article) = app.feeds.get(fi).and_then(|f| f.articles.get(ai)) else {
                    return ListItem::new("");
                };
                let is_selected = app.selected_article == i
                    && matches!(app.state, AppState::ArticleList | AppState::ArticleDetail);
                let style = if is_selected {
                    Style::default()
                        .fg(app.theme.accent)
                        .bg(app.theme.border)
                        .bold()
                } else if article.is_read {
                    Style::default().fg(app.theme.muted_text)
                } else {
                    Style::default().fg(app.theme.text)
                };
                let title_available = (inner.width as usize).saturating_sub(2);
                let displayed_title = if is_selected {
                    let elapsed = app.tick.saturating_sub(app.article_title_start_tick);
                    scroll_title(&article.title, title_available, elapsed)
                } else {
                    article.title.clone()
                };
                let age_str: Option<String> = article.published_secs.map(short_age);
                let mut spans = vec![
                    Span::styled(
                        article.get_icon(),
                        article.get_icon_style(
                            app.theme.unread,
                            app.theme.muted_text,
                            app.theme.link,
                        ),
                    ),
                    Span::raw(displayed_title),
                ];
                if let Some(ref age) = age_str {
                    spans.push(
                        age.clone()
                            .fg(age_color(article.published_secs.unwrap(), &app.theme))
                            .dim(),
                    );
                }
                ListItem::new(Line::from(spans)).style(style)
            })
            .collect();

        let clamped = app
            .selected_article
            .min(app.category_view_articles.len().saturating_sub(1));
        app.article_list_state.select(Some(clamped));
        render_scrollable_list(f, items, inner, &mut app.article_list_state, &app.theme);
        if show_footer {
            draw_article_footer(f, app, footer_area, false);
        }
        return;
    }

    // In the Saved tab but no category is selected (cursor on a category or nothing).
    if app.selected_tab == Tab::Saved && !app.in_saved_context {
        let block = content_block(
            " ★ Saved ".fg(app.theme.link).bold(),
            false,
            app.user_data.border_rounded,
            &app.theme,
        );
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(" Select a category to view saved articles.")
                .style(Style::default().fg(app.theme.muted_text)),
            inner,
        );
        if show_footer {
            draw_article_footer(f, app, footer_area, false);
        }
        return;
    }

    let (feed_title, articles): (String, &[Article]) = if app.in_saved_context {
        let title = if let Some(cat_id) = app.selected_saved_category {
            app.user_data
                .saved_categories
                .iter()
                .find(|c| c.id == cat_id)
                .map(|c| format!(" {} ", c.name))
                .unwrap_or_else(|| " Saved ".to_string())
        } else {
            " 🞴 All Saved ".to_string()
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

    let is_navigating = matches!(app.state, AppState::ArticleList);
    let block = content_block(
        feed_title.fg(app.theme.link).bold(),
        is_navigating,
        app.user_data.border_rounded,
        &app.theme,
    );

    let inner = block.inner(area);
    f.render_widget(block, area);

    let list_area = inner;

    if articles.is_empty() {
        if !app.in_saved_context
            && let Some(feed) = app.feeds.get(app.selected_feed)
            && let Some(err) = &feed.fetch_error
        {
            let text = Line::from(vec![
                " ⚠ ".fg(app.theme.error),
                err.clone().fg(app.theme.text),
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
                .style(Style::default().fg(app.theme.muted_text)),
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
        let is_nav_highlight = is_navigating && app.selected_article == i && !is_selected;
        items.push(build_article_list_item(
            article,
            is_selected,
            is_nav_highlight,
            list_area.width,
            app.tick,
            app.article_title_start_tick,
            &app.theme,
        ));
    }

    // Add separator if there are archived articles
    if has_archived {
        items.push(ListItem::new(Line::from(
            " ── Archived ──".fg(app.theme.muted_text),
        )));
    }

    // Add archived articles
    for &i in &archived_indices {
        let article = &articles[i];
        let is_selected = app.selected_article == i
            && (matches!(app.state, AppState::ArticleList | AppState::ArticleDetail)
                || app.in_saved_context);
        let is_nav_highlight = is_navigating && app.selected_article == i && !is_selected;
        items.push(build_article_list_item(
            article,
            is_selected,
            is_nav_highlight,
            list_area.width,
            app.tick,
            app.article_title_start_tick,
            &app.theme,
        ));
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
                .unwrap_or(0)
        };

    app.article_list_state.select(Some(visual_selected));
    render_scrollable_list(f, items, list_area, &mut app.article_list_state, &app.theme);

    if show_footer {
        draw_article_footer(f, app, footer_area, false);
    }
}
