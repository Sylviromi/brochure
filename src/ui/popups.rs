//! Modal overlay popups: add-feed wizard, OPML paths, confirm dialogs, and category picker.
//!
//! This module renders transient modal dialogs that appear over the main content,
//! including the two-step add-feed wizard, OPML import/export path prompts, confirmation dialogs,
//! and the category picker for saving articles to custom categories.

use crate::{
    app::App,
    models::{AddFeedStep, AppState, CategoryId},
    ui::content::utils::split_cursor,
};
use ratatui::layout::Constraint::{Fill, Length, Min};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::{border_set, content_block, render_scrollbar};

/// Word-wraps `text` with a hanging bullet indent into pre-formatted strings.
///
/// The first line gets `"  • "` and continuations get `"    "` (same width),
/// so wrapped lines visually align with the start of the bullet text rather than column 0.
fn wrap_bullet(text: &str, total_width: usize) -> Vec<String> {
    const BULLET: &str = "  • ";
    const INDENT: &str = "    "; // same display width as BULLET
    let inner_w = total_width.saturating_sub(4);
    if inner_w == 0 {
        return vec![format!("{BULLET}{text}")];
    }
    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let word_w = word.chars().count();
        if current.is_empty() {
            current.push_str(word);
        } else if current.chars().count() + 1 + word_w <= inner_w {
            current.push(' ');
            current.push_str(word);
        } else {
            segments.push(current);
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    if segments.is_empty() {
        return vec![BULLET.to_string()];
    }
    segments
        .into_iter()
        .enumerate()
        .map(|(i, s)| format!("{}{s}", if i == 0 { BULLET } else { INDENT }))
        .collect()
}

/// Renders the add-feed popup with URL and title input fields.
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
    let url_content = if app.add_feed.step == AddFeedStep::Url {
        let (before, cursor_ch, after) =
            split_cursor(&app.add_feed.url_input, app.add_feed.input_cursor);
        Line::from(vec![
            before.fg(app.theme.text),
            cursor_ch.fg(app.theme.bg).bg(app.theme.success),
            after.fg(app.theme.text),
        ])
    } else {
        Line::from(Span::styled(
            app.add_feed.url.clone(),
            Style::default().fg(app.theme.text),
        ))
    };
    let url_block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(
            Style::default().fg(if app.add_feed.step == AddFeedStep::Url {
                app.theme.accent
            } else {
                app.theme.muted
            }),
        )
        .bg(app.theme.bg)
        .title(Span::styled(
            " Feed URL ",
            Style::default().fg(app.theme.link).bold(),
        ));
    f.render_widget(Paragraph::new(url_content).block(url_block), url_area);

    // Title field
    f.render_widget(Clear, title_area);
    let title_label = if app.add_feed.step == AddFeedStep::Url {
        " Feed Title (enter URL first) "
    } else {
        " Feed Title (Enter to save, Esc to cancel) "
    };
    let title_block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(
            Style::default().fg(if app.add_feed.step == AddFeedStep::Title {
                app.theme.accent
            } else {
                app.theme.muted
            }),
        )
        .bg(app.theme.bg)
        .title(Span::styled(
            title_label,
            Style::default().fg(app.theme.link).bold(),
        ));

    if app.add_feed.step == AddFeedStep::Title && app.add_feed.url_input.is_empty() {
        match &app.add_feed.fetched_title {
            Some(t) if !t.is_empty() => {
                f.render_widget(
                    Paragraph::new(t.as_str())
                        .block(title_block)
                        .style(Style::default().fg(app.theme.muted)),
                    title_area,
                );
                return;
            }
            Some(_) => {
                f.render_widget(
                    Paragraph::new("")
                        .block(title_block)
                        .style(Style::default().fg(app.theme.text)),
                    title_area,
                );
            }
            None => {
                f.render_widget(
                    Paragraph::new("⏳ Fetching title...")
                        .block(title_block)
                        .style(Style::default().fg(app.theme.text)),
                    title_area,
                );
            }
        }
        return;
    }

    let title_content =
        if app.add_feed.step == AddFeedStep::Title && !app.add_feed.url_input.is_empty() {
            let (before, cursor_ch, after) =
                split_cursor(&app.add_feed.url_input, app.add_feed.input_cursor);
            Line::from(vec![
                before.fg(app.theme.text),
                cursor_ch.fg(app.theme.bg).bg(app.theme.success),
                after.fg(app.theme.text),
            ])
        } else {
            Line::from(String::new())
        };
    f.render_widget(Paragraph::new(title_content).block(title_block), title_area);
}

/// Renders a centered confirmation dialog with a red bordered block.
/// `horiz_pct` is the percentage width for the center column (60 or 70).
fn draw_confirm_dialog(f: &mut Frame, app: &App, title: &str, body: String, horiz_pct: u16) {
    let area = f.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(38),
            Constraint::Length(7),
            Constraint::Percentage(38),
        ])
        .split(area);
    let side_pct = (100 - horiz_pct) / 2;
    let center = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(side_pct),
            Constraint::Percentage(horiz_pct),
            Constraint::Percentage(side_pct),
        ])
        .split(vertical[1])[1];

    f.render_widget(Clear, center);
    let block = Block::default()
        .border_set(border_set(app.user_data.border_rounded))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.error))
        .bg(app.theme.bg)
        .title(Span::styled(
            title.to_string(),
            Style::default().fg(app.theme.error).bold(),
        ));
    let text = vec![
        Line::from(""),
        Line::from(body.fg(app.theme.text)),
        Line::from(""),
        Line::from(vec![
            "  [Enter] ".fg(app.theme.error).bold(),
            "Confirm   ".fg(app.theme.text),
            "[Esc] ".fg(app.theme.success).bold(),
            "Cancel".fg(app.theme.text),
        ]),
    ];
    f.render_widget(Paragraph::new(text).block(block), center);
}

/// Renders the confirmation dialog for deleting all feeds.
pub(super) fn draw_confirm_delete_all(f: &mut Frame, app: &App) {
    draw_confirm_dialog(
        f,
        app,
        " ⚠  Remove All Feeds ",
        "  This will delete all feeds permanently.".to_string(),
        60,
    );
}

/// Renders the confirmation dialog for clearing the article cache.
pub(super) fn draw_confirm_clear_cache(f: &mut Frame, app: &App) {
    draw_confirm_dialog(
        f,
        app,
        " ⚠  Clear Article Cache ",
        "  This will delete all saved article content.".to_string(),
        60,
    );
}

/// Renders the confirmation dialog for deleting a category and its feeds.
pub(super) fn draw_confirm_delete_cat(
    f: &mut Frame,
    app: &App,
    cat_id: CategoryId,
    feed_count: usize,
) {
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
    draw_confirm_dialog(f, app, " ⚠  Delete Category ", body, 70);
}

/// Renders the OPML import/export path input popup.
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
    let block = content_block(
        title.fg(app.theme.link).bold(),
        true,
        app.user_data.border_rounded,
        &app.theme,
    );

    let (before, cursor_ch, after) = split_cursor(&app.opml.path_input, app.opml.input_cursor);
    let content = Line::from(vec![
        before.fg(app.theme.text),
        cursor_ch.fg(app.theme.bg).bg(app.theme.success),
        after.fg(app.theme.text),
    ]);

    f.render_widget(Paragraph::new(content).block(block), center);
}

/// Renders the category picker popup for saving an article to a custom category.
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

    let block = content_block(
        " Save Article To… ",
        true,
        app.user_data.border_rounded,
        &app.theme,
    );

    f.render_widget(Clear, popup_area);
    f.render_widget(block.clone(), popup_area);

    let inner = block.inner(popup_area);

    let mut lines: Vec<Line> = Vec::new();

    // fixed rows: "New category" + optional separator + "✕ Unsave"
    let fixed_rows = if article_is_saved { 3u16 } else { 1u16 };
    let visible_cats = inner.height.saturating_sub(fixed_rows) as usize;
    let scroll_top = if visible_cats == 0 || cats_len <= visible_cats {
        0usize
    } else {
        let cursor = app.category_picker.cursor.min(cats_len.saturating_sub(1));
        cursor
            .saturating_sub(visible_cats.saturating_sub(1))
            .min(cats_len - visible_cats)
    };

    for (i, cat) in cats[scroll_top..].iter().take(visible_cats).enumerate() {
        let real_idx = scroll_top + i;
        let is_selected = app.category_picker.cursor == real_idx;
        let style = if is_selected {
            Style::default()
                .bg(app.theme.selection)
                .fg(app.theme.accent)
        } else {
            Style::default().fg(app.theme.text)
        };
        lines.push(Line::from(Span::styled(format!("  {}", cat.name), style)));
    }

    let new_idx = cats_len;
    if app.category_picker.new_mode {
        let (before, cursor_ch, after) =
            split_cursor(&app.category_picker.input, app.category_picker.input_cursor);
        lines.push(Line::from(vec![
            "  + ".fg(app.theme.link),
            before.fg(app.theme.text),
            cursor_ch.fg(app.theme.bg).bg(app.theme.link),
            after.fg(app.theme.text),
        ]));
    } else {
        let new_style = if app.category_picker.cursor == new_idx {
            Style::default().bg(app.theme.selection).fg(app.theme.link)
        } else {
            Style::default().fg(app.theme.link)
        };
        lines.push(Line::from(Span::styled("  + New category…", new_style)));
    }

    if article_is_saved {
        lines.push(Line::from(Span::styled(
            "  ──────────────",
            Style::default().fg(app.theme.border),
        )));

        let unsave_idx = cats_len + 1;
        let unsave_style = if app.category_picker.cursor == unsave_idx {
            Style::default().bg(app.theme.selection).fg(app.theme.error)
        } else {
            Style::default().fg(app.theme.error)
        };
        lines.push(Line::from(Span::styled("  ✕ Unsave", unsave_style)));
    }

    let para = Paragraph::new(lines);
    f.render_widget(para, inner);

    if cats_len > visible_cats {
        let bar_area = Rect {
            x: inner.x + inner.width.saturating_sub(1),
            width: 1,
            ..inner
        };
        render_scrollbar(
            f,
            bar_area,
            cats_len,
            inner.height as usize,
            app.category_picker.cursor.min(cats_len.saturating_sub(1)),
            &app.theme,
        );
    }
}

/// Renders the confirmation dialog for deleting a saved category.
pub(super) fn draw_confirm_delete_saved_cat(f: &mut Frame, app: &App) {
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
    draw_confirm_dialog(f, app, " ⚠  Delete Saved Category ", body, 70);
}

/// Renders the update-available popup with scrollable release notes.
pub(super) fn draw_update_popup(f: &mut Frame, app: &mut App) {
    let Some(ref info) = app.update_available else {
        return;
    };
    let releases = &info.releases;
    if releases.is_empty() {
        return;
    }

    let area = f.area();
    let latest = &releases[0].version;
    let count = releases.len();

    // Fixed width 60, height = min(terminal - 6, 22), centered
    let popup_w: u16 = 60;
    let popup_h: u16 = (area.height.saturating_sub(6)).clamp(10, 22);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Fill(1), Length(popup_h), Fill(1)])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Fill(1), Length(popup_w), Fill(1)])
        .split(vertical[1]);
    let popup_area = horizontal[1];

    f.render_widget(Clear, popup_area);

    // Title shows version count
    let title = if count == 1 {
        format!("  Update Available — v{latest} ")
    } else {
        format!("  Update Available — v{latest}  ({count} new versions) ")
    };

    let block = content_block(
        title.fg(app.theme.header).bold(),
        true,
        app.user_data.border_rounded,
        &app.theme,
    );

    // Inner area split: scrollable content (top) + dismiss hint (bottom, 1 line)
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let [content_area, hint_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Min(0), Length(1)])
        .areas(inner);

    // Always reserve 1 col for the scrollbar so pre-wrapped widths stay stable.
    let text_w = content_area.width.saturating_sub(1) as usize;

    // Build all content lines (pre-wrapped so total_lines == visual line count).
    let current = env!("CARGO_PKG_VERSION");
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    for (i, release) in releases.iter().enumerate() {
        // Version header line
        let header = if release.date.is_empty() {
            format!("  v{}  →  v{}", current, release.version) // only on first
        } else if i == 0 {
            format!(
                "  v{}  →  v{}    {}",
                current, release.version, release.date
            )
        } else {
            format!("  v{}    {}", release.version, release.date)
        };
        lines.push(Line::from(Span::styled(
            header,
            Style::default().fg(app.theme.accent).bold(),
        )));
        for h in &release.highlights {
            for wrapped in wrap_bullet(h, text_w) {
                lines.push(Line::from(Span::styled(
                    wrapped,
                    Style::default().fg(app.theme.text),
                )));
            }
        }
        lines.push(Line::from("")); // spacer between releases / bottom padding
    }

    // Scroll
    let total_lines = lines.len() as u16;
    let visible_h = content_area.height;
    let max_scroll = total_lines.saturating_sub(visible_h);
    // Clamp and write back so the handler's value never drifts above the actual limit.
    app.update_popup_scroll = app.update_popup_scroll.min(max_scroll);
    let scroll = app.update_popup_scroll;

    // Render scrollable content (leave 1 col on right for scrollbar)
    let [text_area, bar_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Min(0), Length(1)])
        .areas(content_area);

    f.render_widget(Paragraph::new(lines).scroll((scroll, 0)), text_area);

    // Scrollbar
    if total_lines > visible_h {
        render_scrollbar(
            f,
            bar_area,
            total_lines as usize,
            visible_h as usize,
            scroll as usize,
            &app.theme,
        );
    }

    // Pinned dismiss hint
    f.render_widget(
        Paragraph::new(Line::from(vec![
            "  [Enter/Esc/q] ".fg(app.theme.accent).bold(),
            "Dismiss".fg(app.theme.text),
        ])),
        hint_area,
    );
}
