//! Footer rendered below the article list or article detail panel.

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
    widgets::{Block, Borders, Paragraph},
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

/// Height of a bordered footer: one content row plus the bottom border.
const FOOTER_HEIGHT: u16 = 2;

/// Splits `area` into the content above and a bordered footer strip at the bottom.
pub(super) fn split_footer(area: Rect) -> (Rect, Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(FOOTER_HEIGHT)])
        .split(area);
    (rows[0], rows[1])
}

/// Bordered footer drawn below a panel, as carved out by [`split_footer`].
///
/// Must be drawn after the panels above it: their bottom corners are turned into
/// junctions so the footer reads as part of the same frame.
///
/// When `is_article_view` is `true`, renders article info (link, scroll %).
/// When `is_article_view` is `false`, renders feed stats (counts, fetch age).
pub(super) fn draw_article_footer(f: &mut Frame, app: &App, area: Rect, is_article_view: bool) {
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
        .border_set(crate::ui::border_set(app.user_data.border_rounded))
        .border_style(Style::default().fg(app.theme.border))
        .bg(app.theme.bg);
    let inner = block.inner(area);
    f.render_widget(block, area);
    join_panels_above(f, area);
    if is_article_view {
        draw_article_info(f, app, inner);
    } else {
        draw_list_stats(f, app, inner);
    }
}

/// Replaces the bottom corners of the panels directly above `area` with T-junctions
/// (`├`, `┴`, `┤`). Only the symbol changes, so a focused panel keeps its colour.
fn join_panels_above(f: &mut Frame, area: Rect) {
    let Some(y) = area.y.checked_sub(1) else {
        return;
    };
    let buf = f.buffer_mut();
    let right = area.right().saturating_sub(1);
    for x in area.x..=right {
        let Some(cell) = buf.cell_mut((x, y)) else {
            continue;
        };
        if !matches!(cell.symbol(), "╰" | "╯" | "└" | "┘") {
            continue;
        }
        let junction = if x == area.x {
            "├"
        } else if x == right {
            "┤"
        } else {
            "┴"
        };
        cell.set_symbol(junction);
    }
}

/// Article detail footer: link and scroll percentage. `area` is one row.
pub(super) fn draw_article_info(f: &mut Frame, app: &App, area: Rect) {
    let Some(article) = app.selected_article() else {
        return;
    };
    let theme = &app.theme;
    let link_spans = vec![Span::raw(" "), article.link.clone().fg(theme.muted)];

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::border_set;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn panel_corners_become_junctions() {
        let mut terminal = Terminal::new(TestBackend::new(10, 3)).unwrap();
        terminal
            .draw(|f| {
                for x in [0, 5] {
                    let block = Block::default()
                        .borders(Borders::ALL)
                        .border_set(border_set(true));
                    f.render_widget(block, Rect::new(x, 0, 5, 2));
                }
                join_panels_above(f, Rect::new(0, 2, 10, 1));
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let row: String = (0..10).map(|x| buf[(x, 1)].symbol()).collect();
        assert_eq!(row, "├───┴┴───┤");
    }
}
