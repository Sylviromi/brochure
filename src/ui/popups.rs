//! Modal overlay popups: add-feed wizard, import/export paths, confirm dialogs, and category picker.
//!
//! This module renders transient modal dialogs that appear over the main content,
//! including the two-step add-feed wizard, import/export path prompts, confirmation dialogs,
//! and the category picker for saving articles to custom categories.

use crate::{
    app::App,
    models::{AddFeedStep, AppState, CategoryId, FreshRssStep, HitAreas, ScrollTarget},
    ui::{content::utils::split_cursor, theme::ColorTheme},
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Constraint::{Fill, Length, Min};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph},
};

use super::{border_set, content_block, render_scrollbar};

/// Word-wraps `text` to `total_width` into pre-formatted strings: the first line starts
/// with `first`, continuations with `rest` (same width, so wrapped text stays aligned).
fn wrap_indented(text: &str, total_width: usize, first: &str, rest: &str) -> Vec<String> {
    let inner_w = total_width.saturating_sub(first.chars().count());
    if inner_w == 0 {
        return vec![format!("{first}{text}")];
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
        return vec![first.to_string()];
    }
    segments
        .into_iter()
        .enumerate()
        .map(|(i, s)| format!("{}{s}", if i == 0 { first } else { rest }))
        .collect()
}

/// One labelled input box inside a [`draw_form_popup`].
struct FormField<'a> {
    label: &'a str,
    content: Line<'a>,
    active: bool,
}

/// `text` with a block cursor at char index `cursor`.
fn cursor_line(text: &str, cursor: usize, theme: &ColorTheme) -> Line<'static> {
    let (before, cursor_ch, after) = split_cursor(text, cursor);
    Line::from(vec![
        before.fg(theme.text),
        cursor_ch.fg(theme.bg).bg(theme.success),
        after.fg(theme.text),
    ])
}

/// A `[key] action` entry in a popup's hint row; clicking it presses the key.
struct Hint {
    label: &'static str,
    key: KeyEvent,
    action: &'static str,
    color: Color,
}

impl Hint {
    fn new(label: &'static str, code: KeyCode, action: &'static str, color: Color) -> Self {
        Self {
            label,
            key: KeyEvent::new(code, KeyModifiers::NONE),
            action,
            color,
        }
    }
}

/// The usual `[Enter] … [Esc] …` pair.
fn enter_esc(enter: &'static str, esc: &'static str, theme: &ColorTheme) -> Vec<Hint> {
    vec![
        Hint::new("Enter", KeyCode::Enter, enter, theme.accent),
        Hint::new("Esc", KeyCode::Esc, esc, theme.accent),
    ]
}

/// Draw `hints` on the row `area` and record each one as a mouse button.
fn draw_hints(f: &mut Frame, hit: &mut HitAreas, area: Rect, hints: &[Hint], text: Color) {
    let mut spans = vec![Span::raw(" ")];
    let mut x = area.x + 1;
    for h in hints {
        let key = format!("[{}] ", h.label);
        let width = (key.chars().count() + h.action.chars().count()) as u16;
        hit.buttons
            .push((Rect::new(x, area.y, width, 1).intersection(area), h.key));
        spans.push(key.fg(h.color).bold());
        spans.push(h.action.fg(text));
        spans.push(Span::raw("   "));
        x = x.saturating_add(width + 3);
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Renders a centered, framed form: `fields` stacked as input boxes with a key
/// hint underneath, all on one solid background so nothing behind shows through.
fn draw_form_popup(
    f: &mut Frame,
    app: &mut App,
    title: &str,
    fields: &[FormField],
    hints: &[Hint],
) {
    let theme = &app.theme;
    let rounded = app.user_data.border_rounded;
    // Border + top padding + 3 rows per field + gaps + padding + hint + border.
    let n = fields.len() as u16;
    let height = 2 + 1 + n * 3 + n.saturating_sub(1) + 1 + 1;
    let popup = f
        .area()
        .centered(Constraint::Percentage(60), Constraint::Length(height));

    f.render_widget(Clear, popup);
    let frame = content_block(title.fg(theme.header).bold(), true, rounded, theme);
    let inner = frame.inner(popup);
    f.render_widget(frame, popup);

    let mut constraints = vec![Length(1)];
    for i in 0..fields.len() {
        if i > 0 {
            constraints.push(Length(1));
        }
        constraints.push(Length(3));
    }
    constraints.extend([Fill(1), Length(1)]);
    let rows = Layout::vertical(constraints)
        .horizontal_margin(1)
        .split(inner);

    for (i, field) in fields.iter().enumerate() {
        let block = Block::default()
            .border_set(border_set(rounded))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if field.active {
                theme.accent
            } else {
                theme.border
            }))
            .padding(Padding::horizontal(1))
            .title(Span::styled(
                field.label,
                Style::default().fg(theme.link).bold(),
            ));
        f.render_widget(
            Paragraph::new(field.content.clone()).block(block),
            rows[1 + i * 2],
        );
        app.hit.form_fields.push((rows[1 + i * 2], i));
    }
    draw_hints(
        f,
        &mut app.hit,
        rows[rows.len() - 1],
        hints,
        app.theme.muted,
    );
}

/// Renders the add-feed popup with URL and title input fields.
pub(super) fn draw_add_feed_popup(f: &mut Frame, app: &mut App) {
    let theme = &app.theme;
    let state = &app.add_feed;
    let on_url = state.step == AddFeedStep::Url;
    let input = &state.url_input;

    let url_content = if on_url {
        cursor_line(&input.text, input.cursor, theme)
    } else {
        Line::from(state.url.clone().fg(theme.text))
    };
    // The title step reuses `url_input`; when it's blank the fetched title is the default.
    let title_content = if on_url {
        Line::default()
    } else if input.text.is_empty() {
        match state.fetched_title.as_deref() {
            Some(t) => Line::from(t.to_string().fg(theme.muted)),
            None => Line::from("⏳ Fetching title...".fg(theme.muted)),
        }
    } else {
        cursor_line(&input.text, input.cursor, theme)
    };
    let fields = [
        FormField {
            label: " Feed URL ",
            content: url_content,
            active: on_url,
        },
        FormField {
            label: if on_url {
                " Feed title "
            } else {
                " Feed title (blank uses the fetched title) "
            },
            content: title_content,
            active: !on_url,
        },
    ];
    let hints = enter_esc(if on_url { "Next" } else { "Add feed" }, "Cancel", theme);
    draw_form_popup(f, app, " Add Feed ", &fields, &hints);
}

/// Renders the FreshRSS setup wizard: server, username, and masked API password fields.
pub(super) fn draw_freshrss_setup_popup(f: &mut Frame, app: &mut App) {
    let theme = &app.theme;
    let setup = &app.freshrss_setup;
    let field = |label, input: &crate::models::TextInput, step, masked: bool| {
        let active = setup.step == step;
        let display = if masked {
            "•".repeat(input.text.chars().count())
        } else {
            input.text.clone()
        };
        // Masking keeps one char per char, so the cursor index still lines up.
        let content = if active {
            cursor_line(&display, input.cursor, theme)
        } else {
            Line::from(display.fg(theme.text))
        };
        FormField {
            label,
            content,
            active,
        }
    };
    let editing = setup.editing_username.is_some();
    let fields = [
        field(
            " Server (https://rss.example.com) ",
            &setup.server_input,
            FreshRssStep::Server,
            false,
        ),
        field(
            " Username ",
            &setup.username_input,
            FreshRssStep::Username,
            false,
        ),
        field(
            if editing {
                " API password (leave blank to keep current) "
            } else {
                " API password "
            },
            &setup.password_input,
            FreshRssStep::Password,
            true,
        ),
    ];
    let (enter, esc) = match setup.step {
        FreshRssStep::Server => ("Next", "Cancel"),
        FreshRssStep::Username => ("Next", "Back"),
        FreshRssStep::Password => ("Save + sync", "Back"),
    };
    let title = if editing {
        " Edit FreshRSS Connection "
    } else {
        " Connect to FreshRSS "
    };
    let mut hints = enter_esc(enter, esc, theme);
    if editing {
        hints.push(Hint {
            label: "Ctrl+D",
            key: KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL),
            action: "Disconnect",
            color: theme.error,
        });
    }
    draw_form_popup(f, app, title, &fields, &hints);
}

/// Renders a centered confirmation dialog with a red bordered block.
/// `horiz_pct` is the percentage width for the center column (60 or 70).
fn draw_confirm_dialog(f: &mut Frame, app: &mut App, title: &str, body: String, horiz_pct: u16) {
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
    let inner = block.inner(center);
    let text = vec![Line::from(""), Line::from(body.fg(app.theme.text))];
    f.render_widget(Paragraph::new(text).block(block), center);
    let hints = [
        Hint::new("Enter", KeyCode::Enter, "Confirm", app.theme.error),
        Hint::new("Esc", KeyCode::Esc, "Cancel", app.theme.success),
    ];
    let row = Rect {
        y: inner.y + 3,
        height: 1,
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
    };
    draw_hints(f, &mut app.hit, row, &hints, app.theme.text);
}

/// Renders the confirmation dialog for deleting all feeds.
pub(super) fn draw_confirm_delete_all(f: &mut Frame, app: &mut App) {
    draw_confirm_dialog(
        f,
        app,
        " ⚠  Remove All Feeds ",
        "  This will delete all feeds permanently.".to_string(),
        60,
    );
}

/// Renders the confirmation dialog for clearing the article cache.
pub(super) fn draw_confirm_clear_cache(f: &mut Frame, app: &mut App) {
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
    app: &mut App,
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

/// Renders the import/export file path input popup.
pub(super) fn draw_file_path_popup(f: &mut Frame, app: &mut App) {
    let what = app.file_prompt.format.label();
    let title = if app.state == AppState::ExportPath {
        format!(" Export {what} — destination path ")
    } else {
        format!(" Import {what} — source path ")
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

    let content = cursor_line(
        &app.file_prompt.path_input.text,
        app.file_prompt.path_input.cursor,
        &app.theme,
    );

    f.render_widget(Paragraph::new(content).block(block), center);
    let row = Rect {
        y: center.bottom(),
        height: 1,
        ..center
    };
    f.render_widget(Clear, row);
    let verb = if app.state == AppState::ExportPath {
        "Export"
    } else {
        "Import"
    };
    let hints = enter_esc(verb, "Cancel", &app.theme);
    f.render_widget(Block::default().bg(app.theme.bg), row);
    draw_hints(f, &mut app.hit, row, &hints, app.theme.muted);
}

/// Renders the category picker popup for saving an article to a custom category.
pub(super) fn draw_category_picker(f: &mut Frame, app: &mut App) {
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
        let (before, cursor_ch, after) = split_cursor(
            &app.category_picker.input.text,
            app.category_picker.input.cursor,
        );
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

    let row_count = lines.len();
    f.render_widget(Paragraph::new(lines), inner);
    app.hit.popup = popup_area;
    if !app.category_picker.new_mode {
        let shown = cats_len.min(visible_cats);
        app.hit.record_rows(inner, 0, row_count, |row| match row {
            r if r < shown => Some(scroll_top + r),
            r if r == shown => Some(cats_len), // + New category…
            r if r == shown + 2 => Some(cats_len + 1), // ✕ Unsave (after separator)
            _ => None,
        });
    }

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
            None,
        );
    }
}

/// Renders the confirmation dialog for deleting a saved category.
pub(super) fn draw_confirm_delete_saved_cat(f: &mut Frame, app: &mut App) {
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

/// Renders the update-available popup: what's new since the running version, newest
/// release first, and how to update. Sized to its content; scrolls when it doesn't fit.
pub(super) fn draw_update_popup(f: &mut Frame, app: &mut App) {
    let Some(info) = &app.update_available else {
        return;
    };
    let Some(latest) = info.releases.first() else {
        return;
    };
    let theme = &app.theme;
    let area = f.area();
    let popup_w = 60.min(area.width);
    // Borders and a column reserved for the scrollbar, so wrapping never shifts.
    let text_w = popup_w.saturating_sub(3) as usize;

    let mut summary = vec![
        Span::styled("  You have ", theme.muted),
        Span::styled(format!("v{}", env!("CARGO_PKG_VERSION")), theme.text),
        Span::styled(" · ", theme.muted),
        Span::styled(format!("v{}", latest.version), theme.success).bold(),
        Span::styled(" is out", theme.muted),
    ];
    if info.releases.len() > 1 {
        summary.push(Span::styled(
            format!(" ({} releases)", info.releases.len()),
            theme.muted,
        ));
    }
    let mut lines = vec![Line::from(summary)];
    let how = "To update, re-run the command you installed with, e.g. cargo binstall brochure";
    for row in wrap_indented(how, text_w, "  ", "  ") {
        lines.push(Line::styled(row, theme.muted));
    }
    for release in &info.releases {
        lines.push(Line::from(""));
        let mut header = vec![Span::styled(format!("  v{}", release.version), theme.accent).bold()];
        if !release.date.is_empty() {
            header.push(Span::styled(format!("  {}", release.date), theme.muted));
        }
        lines.push(Line::from(header));
        if release.highlights.is_empty() {
            lines.push(Line::styled("    No release notes", theme.muted).italic());
        }
        for highlight in &release.highlights {
            for row in wrap_indented(highlight, text_w, "  • ", "    ") {
                lines.push(Line::styled(row, theme.text));
            }
        }
    }
    lines.push(Line::from(""));

    // Content plus borders and the key hint row, capped so the screen keeps some margin.
    let total = lines.len() as u16;
    let popup_h = (total + 3).min(24).min(area.height.saturating_sub(2));
    let [popup] = Layout::horizontal([Length(popup_w)])
        .flex(ratatui::layout::Flex::Center)
        .areas(area);
    let [popup] = Layout::vertical([Length(popup_h)])
        .flex(ratatui::layout::Flex::Center)
        .areas(popup);
    f.render_widget(Clear, popup);
    let block = content_block(
        " Update available ".fg(theme.header).bold(),
        true,
        app.user_data.border_rounded,
        theme,
    );
    let inner = block.inner(popup);
    f.render_widget(block, popup);
    let [content, hint] = Layout::vertical([Min(0), Length(1)]).areas(inner);
    let [text_area, bar_area] = Layout::horizontal([Min(0), Length(1)]).areas(content);

    let max_scroll = total.saturating_sub(content.height);
    app.update_popup_scroll = app.update_popup_scroll.min(max_scroll);
    let scroll = app.update_popup_scroll;
    let mut keys = vec![
        "  Enter/Esc/q ".fg(app.theme.accent).bold(),
        "dismiss".fg(app.theme.text),
    ];
    if max_scroll > 0 {
        keys.push("   j/k ".fg(app.theme.accent).bold());
        keys.push("scroll".fg(app.theme.text));
    }
    f.render_widget(Paragraph::new(lines).scroll((scroll, 0)), text_area);
    f.render_widget(Paragraph::new(Line::from(keys)), hint);
    if max_scroll > 0 {
        render_scrollbar(
            f,
            bar_area,
            total as usize,
            content.height as usize,
            scroll as usize,
            &app.theme,
            Some((&mut app.hit, ScrollTarget::UpdatePopup)),
        );
    }
}
