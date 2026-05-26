//! Unified one-row footer rendered below the article list or article detail panel.

use crate::{app::App, handlers::article::get_selected_article, models::Tab};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::utils::{age_color, format_age};

/// Unified footer renderer used by both `draw_article_list` and `draw_article_detail`.
///
/// When `is_article_view` is `true`, renders article info (link, publish date, scroll %).
/// When `is_article_view` is `false`, renders feed stats (counts, fetch age) on a single row.
/// `area` must be exactly 1 row tall.
pub(super) fn draw_article_footer(f: &mut Frame, app: &App, area: Rect, is_article_view: bool) {
    if is_article_view {
        // ── Article detail footer: link, publish date, scroll % ──
        let Some(article) = get_selected_article(app) else {
            return;
        };

        let mut link_spans = vec![Span::raw(" "), article.link.clone().fg(app.theme.muted)];
        if let Some(secs) = article.published_secs {
            let age = format_age(secs);
            let color = age_color(secs, &app.theme);
            link_spans.push("  •  ".fg(app.theme.muted));
            if let Some(number_part) = age.strip_suffix(" ago") {
                link_spans.push(number_part.to_string().fg(color));
                link_spans.push(" ago".fg(app.theme.muted));
            } else {
                link_spans.push(age.fg(color));
            }
        }

        let scroll_offset = app.article_scroll.get(&article.link);
        let line_count = app.content_line_count.max(1);
        let content_height = app.content_area_height;
        let max_scroll = line_count.saturating_sub(content_height as usize);
        let pct = (scroll_offset as usize * 100)
            .checked_div(max_scroll)
            .unwrap_or(100)
            .min(100);
        let pct_str = format!(" {pct}% ");
        let pct_width = pct_str.len() as u16;
        let bar_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(pct_width)])
            .split(area);
        f.render_widget(
            Paragraph::new(Line::from(link_spans)).bg(app.theme.bg),
            bar_chunks[0],
        );
        f.render_widget(
            Paragraph::new(pct_str)
                .style(Style::default().fg(app.theme.warning).bold())
                .bg(app.theme.bg),
            bar_chunks[1],
        );
    } else {
        // ── Feed stats footer: article count, unread, fetch age on a single row ──
        if app.selected_tab == Tab::Saved && !app.in_saved_context {
            f.render_widget(Paragraph::new("").bg(app.theme.bg), area);
            return;
        }

        if (app.in_category_context || app.selected_sidebar_category.is_some())
            && app.selected_tab != Tab::Saved
        {
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
            let unread_color = if unread_count > 0 {
                app.theme.warning
            } else {
                app.theme.success
            };
            let stat_spans = vec![
                " ".fg(app.theme.muted),
                article_count.to_string().fg(app.theme.link),
                " articles  •  ".fg(app.theme.muted),
                unread_count.to_string().fg(unread_color),
                " unread".fg(app.theme.muted),
            ];
            f.render_widget(
                Paragraph::new(Line::from(stat_spans)).bg(app.theme.bg),
                area,
            );
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
        let unread_color = if unread_count > 0 {
            app.theme.warning
        } else {
            app.theme.success
        };
        let mut stat_spans = vec![
            " ".fg(app.theme.muted),
            article_count.to_string().fg(app.theme.link),
            " articles  •  ".fg(app.theme.muted),
            unread_count.to_string().fg(unread_color),
            " unread".fg(app.theme.muted),
        ];
        if let Some(secs) = last_fetched_secs {
            let age = format_age(secs);
            let color = age_color(secs, &app.theme);
            stat_spans.push("  •  fetched ".fg(app.theme.muted));
            if let Some(number_part) = age.strip_suffix(" ago") {
                stat_spans.push(number_part.to_string().fg(color));
                stat_spans.push(" ago".fg(app.theme.muted));
            } else {
                stat_spans.push(age.fg(color));
            }
        }
        f.render_widget(
            Paragraph::new(Line::from(stat_spans)).bg(app.theme.bg),
            area,
        );
    }
}
