use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, ScrollbarState},
};

use ratatui::prelude::Stylize;

use crate::{
    app::App,
    models::{AddFeedStep, AppState, CategoryId},
};

use super::{BASE, BLUE, GREEN, MAUVE, RED, SUBTEXT0, SURFACE0, TEXT, border_set};

pub(super) fn draw_add_feed_popup(f: &mut Frame, app: &App) {
    let area = f.area();

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(35),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Percentage(35),
        ])
        .split(area);

    let center = |row: Rect| {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ])
            .split(row)[1]
    };

    let url_area = center(vertical[1]);
    let title_area = center(vertical[3]);

    // URL field
    f.render_widget(Clear, url_area);
    let url_content = if app.add_feed_step == AddFeedStep::Url {
        app.input.clone()
    } else {
        app.add_feed_url.clone()
    };
    let url_block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(
            Style::default().fg(if app.add_feed_step == AddFeedStep::Url {
                MAUVE
            } else {
                SUBTEXT0
            }),
        )
        .bg(BASE)
        .title(Span::styled(
            " Feed URL ",
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ));
    f.render_widget(
        Paragraph::new(url_content)
            .block(url_block)
            .style(Style::default().fg(TEXT)),
        url_area,
    );

    // Title field
    f.render_widget(Clear, title_area);
    let title_label = if app.add_feed_step == AddFeedStep::Url {
        " Feed Title (enter URL first) "
    } else {
        " Feed Title (Enter to save, Esc to cancel) "
    };
    let title_block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(
            Style::default().fg(if app.add_feed_step == AddFeedStep::Title {
                MAUVE
            } else {
                SUBTEXT0
            }),
        )
        .bg(BASE)
        .title(Span::styled(
            title_label,
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ));

    if app.add_feed_step == AddFeedStep::Title && app.input.is_empty() {
        match &app.add_feed_fetched_title {
            Some(t) if !t.is_empty() => {
                f.render_widget(
                    Paragraph::new(t.as_str())
                        .block(title_block)
                        .style(Style::default().fg(SUBTEXT0)),
                    title_area,
                );
                return;
            }
            Some(_) => {
                f.render_widget(
                    Paragraph::new("")
                        .block(title_block)
                        .style(Style::default().fg(TEXT)),
                    title_area,
                );
            }
            None => {
                f.render_widget(
                    Paragraph::new("⏳ Fetching title...")
                        .block(title_block)
                        .style(Style::default().fg(TEXT)),
                    title_area,
                );
            }
        }
        return;
    }

    let title_content = if app.add_feed_step == AddFeedStep::Title {
        app.input.clone()
    } else {
        String::new()
    };
    f.render_widget(
        Paragraph::new(title_content)
            .block(title_block)
            .style(Style::default().fg(TEXT)),
        title_area,
    );
}

pub(super) fn draw_confirm_delete_all(f: &mut Frame, app: &App) {
    let area = f.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(38),
            Constraint::Length(7),
            Constraint::Percentage(38),
        ])
        .split(area);
    let center = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vertical[1])[1];

    f.render_widget(Clear, center);
    let red = Color::Rgb(243, 139, 168); // Catppuccin Red — intentional, no constant
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(red))
        .bg(BASE)
        .title(Span::styled(
            " ⚠  Remove All Feeds ",
            Style::default().fg(red).add_modifier(Modifier::BOLD),
        ));
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  This will delete all feeds permanently.",
            Style::default().fg(TEXT),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [Enter] ",
                Style::default().fg(red).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Confirm   ", Style::default().fg(TEXT)),
            Span::styled(
                "[Esc] ",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Cancel", Style::default().fg(TEXT)),
        ]),
    ];
    f.render_widget(Paragraph::new(text).block(block), center);
}

pub(super) fn draw_confirm_clear_cache(f: &mut Frame, app: &App) {
    use super::RED;
    let area = f.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(38),
            Constraint::Length(7),
            Constraint::Percentage(38),
        ])
        .split(area);
    let center = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20),
            Constraint::Percentage(60),
            Constraint::Percentage(20),
        ])
        .split(vertical[1])[1];

    f.render_widget(Clear, center);
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(RED))
        .bg(BASE)
        .title(Span::styled(
            " ⚠  Clear Article Cache ",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        ));
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  This will delete all saved article content.",
            Style::default().fg(super::TEXT),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [Enter] ",
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Confirm   ", Style::default().fg(super::TEXT)),
            Span::styled(
                "[Esc] ",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Cancel", Style::default().fg(super::TEXT)),
        ]),
    ];
    f.render_widget(Paragraph::new(text).block(block), center);
}

pub(super) fn draw_confirm_delete_cat(
    f: &mut Frame,
    app: &App,
    cat_id: CategoryId,
    feed_count: usize,
) {
    use super::RED;
    let cat_name = app
        .categories
        .iter()
        .find(|c| c.id == cat_id)
        .map(|c| c.name.as_str())
        .unwrap_or("?");
    let body = if feed_count == 0 {
        format!("  Delete category \"{cat_name}\"?")
    } else {
        format!("  Delete \"{cat_name}\" and {feed_count} feed(s) inside?")
    };

    let area = f.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(38),
            Constraint::Length(7),
            Constraint::Percentage(38),
        ])
        .split(area);
    let center = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ])
        .split(vertical[1])[1];

    f.render_widget(Clear, center);
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(RED))
        .bg(BASE)
        .title(Span::styled(
            " ⚠  Delete Category ",
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        ));
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(body, Style::default().fg(TEXT))),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [Enter] ",
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Confirm   ", Style::default().fg(TEXT)),
            Span::styled(
                "[Esc] ",
                Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Cancel", Style::default().fg(TEXT)),
        ]),
    ];
    f.render_widget(Paragraph::new(text).block(block), center);
}

pub(super) fn draw_opml_path_popup(f: &mut Frame, app: &App) {
    let is_export = app.state == AppState::OPMLExportPath;
    let title = if is_export {
        " Export OPML — destination path "
    } else {
        " Import OPML — source path "
    };

    let area = f.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Length(3),
            Constraint::Percentage(40),
        ])
        .split(area);

    let center = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ])
        .split(vertical[1])[1];

    f.render_widget(Clear, center);
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(MAUVE))
        .bg(BASE)
        .title(Span::styled(
            title,
            Style::default().fg(BLUE).add_modifier(Modifier::BOLD),
        ));

    f.render_widget(
        Paragraph::new(app.opml_path_input.clone())
            .block(block)
            .style(Style::default().fg(TEXT)),
        center,
    );
}

pub(super) fn draw_category_picker(f: &mut Frame, app: &App) {
    let area = f.area();
    let cats = &app.user_data.saved_categories;
    let cats_len = cats.len();

    let article_link: Option<String> = if app.in_category_context {
        app.category_view_articles
            .get(app.selected_article)
            .and_then(|&(fi, ai)| app.feeds.get(fi).and_then(|f| f.articles.get(ai)))
            .map(|a| a.link.clone())
    } else if app.in_saved_context {
        app.saved_view_articles
            .get(app.selected_article)
            .map(|a| a.link.clone())
    } else {
        app.feeds
            .get(app.selected_feed)
            .and_then(|f| f.articles.get(app.selected_article))
            .map(|a| a.link.clone())
    };
    let article_is_saved = article_link.is_some_and(|link| {
        app.user_data
            .saved_articles
            .iter()
            .any(|s| s.article.link == link)
    });

    let height =
        (cats_len as u16 + if article_is_saved { 5 } else { 3 }).min(area.height.saturating_sub(4));
    let width = 40u16.min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect {
        x,
        y,
        width,
        height,
    };

    let rounded = app.user_data.border_rounded;
    let block = Block::default()
        .title(" Save Article To… ")
        .borders(Borders::ALL)
        .border_set(border_set(rounded))
        .border_style(Style::default().fg(MAUVE))
        .style(Style::default().bg(BASE));

    f.render_widget(ratatui::widgets::Clear, popup_area);
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

    let mut lines: Vec<Line> = Vec::new();

    // fixed rows: "New category" + optional separator + "✕ Unsave"
    let fixed_rows = if article_is_saved { 3u16 } else { 1u16 };
    let visible_cats = inner.height.saturating_sub(fixed_rows) as usize;
    let scroll_top = if visible_cats == 0 || cats_len <= visible_cats {
        0usize
    } else {
        let cursor = app.category_picker_cursor.min(cats_len.saturating_sub(1));
        cursor
            .saturating_sub(visible_cats.saturating_sub(1))
            .min(cats_len - visible_cats)
    };

    for (i, cat) in cats[scroll_top..].iter().take(visible_cats).enumerate() {
        let real_idx = scroll_top + i;
        let is_selected = app.category_picker_cursor == real_idx;
        let style = if is_selected {
            Style::default().bg(SURFACE0).fg(MAUVE)
        } else {
            Style::default().fg(TEXT)
        };
        lines.push(Line::from(Span::styled(format!("  {}", cat.name), style)));
    }

    let new_idx = cats_len;
    if app.category_picker_new_mode {
        lines.push(Line::from(vec![
            Span::styled("  + ", Style::default().fg(BLUE)),
            Span::styled(
                format!("{}|", app.category_picker_input),
                Style::default().fg(TEXT),
            ),
        ]));
    } else {
        let new_style = if app.category_picker_cursor == new_idx {
            Style::default().bg(SURFACE0).fg(BLUE)
        } else {
            Style::default().fg(BLUE)
        };
        lines.push(Line::from(Span::styled("  + New category…", new_style)));
    }

    if article_is_saved {
        lines.push(Line::from(Span::styled(
            "  ──────────────",
            Style::default().fg(SURFACE0),
        )));

        let unsave_idx = cats_len + 1;
        let unsave_style = if app.category_picker_cursor == unsave_idx {
            Style::default().bg(SURFACE0).fg(RED)
        } else {
            Style::default().fg(RED)
        };
        lines.push(Line::from(Span::styled("  ✕ Unsave", unsave_style)));
    }

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);

    if cats_len > visible_cats {
        let mut sb_state = ScrollbarState::new(cats_len)
            .position(app.category_picker_cursor.min(cats_len.saturating_sub(1)));
        f.render_stateful_widget(
            ratatui::widgets::Scrollbar::new(ratatui::widgets::ScrollbarOrientation::VerticalRight)
                .style(ratatui::style::Style::default().fg(super::SURFACE0)),
            inner,
            &mut sb_state,
        );
    }
}

pub(super) fn draw_confirm_delete_saved_cat(f: &mut Frame, app: &App) {
    use super::RED;
    let cursor = app.saved_cat_editor_scroll.cursor;
    let cat = app.user_data.saved_categories.get(cursor);
    let Some(cat) = cat else { return };
    let article_count = app
        .user_data
        .saved_articles
        .iter()
        .filter(|s| s.category_id == cat.id)
        .count();
    let body = if article_count == 0 {
        format!("  Delete category \"{}\"?", cat.name)
    } else {
        format!(
            "  Delete \"{}\" and unsave {} article{}?",
            cat.name,
            article_count,
            if article_count == 1 { "" } else { "s" }
        )
    };

    let area = f.area();
    let vertical = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Percentage(38),
            ratatui::layout::Constraint::Length(7),
            ratatui::layout::Constraint::Percentage(38),
        ])
        .split(area);
    let center = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            ratatui::layout::Constraint::Percentage(15),
            ratatui::layout::Constraint::Percentage(70),
            ratatui::layout::Constraint::Percentage(15),
        ])
        .split(vertical[1])[1];

    f.render_widget(ratatui::widgets::Clear, center);
    let block = ratatui::widgets::Block::default()
        .border_set(super::border_set(app.user_data.border_rounded))
        .borders(ratatui::widgets::Borders::ALL)
        .border_style(ratatui::style::Style::default().fg(RED))
        .bg(super::BASE)
        .title(ratatui::text::Span::styled(
            " ⚠  Delete Saved Category ",
            ratatui::style::Style::default()
                .fg(RED)
                .add_modifier(ratatui::style::Modifier::BOLD),
        ));
    let text = vec![
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(ratatui::text::Span::styled(
            body,
            ratatui::style::Style::default().fg(super::TEXT),
        )),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(
                "  [Enter] ",
                ratatui::style::Style::default()
                    .fg(RED)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            ratatui::text::Span::styled(
                "Confirm   ",
                ratatui::style::Style::default().fg(super::TEXT),
            ),
            ratatui::text::Span::styled(
                "[Esc] ",
                ratatui::style::Style::default()
                    .fg(super::GREEN)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            ratatui::text::Span::styled("Cancel", ratatui::style::Style::default().fg(super::TEXT)),
        ]),
    ];
    f.render_widget(ratatui::widgets::Paragraph::new(text).block(block), center);
}
