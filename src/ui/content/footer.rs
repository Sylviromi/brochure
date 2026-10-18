//! Unified one-row footer rendered below the article list or article detail panel.

use crate::{
    app::App,
    models::{Article, Tab},
    ui::theme::ColorTheme,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::utils::{age_color, format_age};

/// Push an age (`3h ago`) with the number tinted by freshness and the suffix muted.
fn push_age_spans(spans: &mut Vec<Span<'static>>, secs: i64, theme: &ColorTheme) {
    let age = format_age(secs);
    let color = age_color(secs, theme);
    match age.strip_suffix(" ago") {
        Some(number) => {
            spans.push(number.to_string().fg(color));
            spans.push(" ago".fg(theme.muted));
        }
        None => spans.push(age.fg(color)),
    }
}

/// `" N articles  •  M unread"` with the unread count coloured by whether any remain.
fn stat_spans(total: usize, unread: usize, theme: &ColorTheme) -> Vec<Span<'static>> {
    let unread_color = if unread > 0 {
        theme.warning
    } else {
        theme.success
    };
    vec![
        " ".fg(theme.muted),
        total.to_string().fg(theme.link),
        " articles  •  ".fg(theme.muted),
        unread.to_string().fg(unread_color),
        " unread".fg(theme.muted),
    ]
}

/// Unified footer renderer used by both `draw_article_list` and `draw_article_detail`.
///
/// When `is_article_view` is `true`, renders article info (link, publish date, scroll %).
/// When `is_article_view` is `false`, renders feed stats (counts, fetch age) on a single row.
/// `area` must be exactly 1 row tall.
pub(super) fn draw_article_footer(f: &mut Frame, app: &App, area: Rect, is_article_view: bool) {
    if is_article_view {
        draw_article_info(f, app, area);
    } else {
        draw_list_stats(f, app, area);
    }
}

/// Article detail footer: link, publish age, and scroll percentage.
fn draw_article_info(f: &mut Frame, app: &App, area: Rect) {
    let Some(article) = app.selected_article() else {
        return;
    };
    let theme = &app.theme;
    let mut link_spans = vec![Span::raw(" "), article.link.clone().fg(theme.muted)];
    if let Some(secs) = article.published_secs {
        link_spans.push("  •  ".fg(theme.muted));
        push_age_spans(&mut link_spans, secs, theme);
    }

    let scroll_offset = app.article_scroll.get(&article.link) as usize;
    let max_scroll = app
        .content_line_count
        .max(1)
        .saturating_sub(app.content_area_height as usize);
    let pct = (scroll_offset * 100)
        .checked_div(max_scroll)
        .unwrap_or(100)
        .min(100);
    let pct_str = format!(" {pct}% ");
    let bar_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(pct_str.len() as u16)])
        .split(area);
    f.render_widget(
        Paragraph::new(Line::from(link_spans)).bg(theme.bg),
        bar_chunks[0],
    );
    f.render_widget(
        Paragraph::new(pct_str)
            .style(Style::default().fg(theme.warning).bold())
            .bg(theme.bg),
        bar_chunks[1],
    );
}

/// List footer: article/unread counts, plus fetch age for a single feed.
fn draw_list_stats(f: &mut Frame, app: &App, area: Rect) {
    let theme = &app.theme;
    let spans = if app.selected_tab == Tab::Saved && !app.in_saved_context {
        Vec::new()
    } else if (app.in_category_context || app.selected_sidebar_category.is_some())
        && app.selected_tab != Tab::Saved
    {
        let unread = app
            .category_view_articles
            .iter()
            .filter(|&&(fi, ai)| {
                app.feeds
                    .get(fi)
                    .and_then(|f| f.articles.get(ai))
                    .is_some_and(|a| !a.is_read)
            })
            .count();
        stat_spans(app.category_view_articles.len(), unread, theme)
    } else {
        let (articles, last_fetched_secs): (&[Article], Option<i64>) = if app.in_saved_context {
            (&app.saved_view_articles, None)
        } else {
            let feed = app.feeds.get(app.selected_feed);
            (
                feed.map_or(&[][..], |f| &f.articles),
                feed.and_then(|f| f.last_fetched_secs),
            )
        };
        let unread = articles.iter().filter(|a| !a.is_read).count();
        let mut spans = stat_spans(articles.len(), unread, theme);
        if let Some(secs) = last_fetched_secs {
            spans.push("  •  fetched ".fg(theme.muted));
            push_age_spans(&mut spans, secs, theme);
        }
        spans
    };
    f.render_widget(Paragraph::new(Line::from(spans)).bg(theme.bg), area);
}
