//! Full-screen theme editor: browse, select, create, edit, rename, delete, export, and import themes.
//!
//! Renders `ThemeEditor`, `ThemeEditorNew` (clone picker), `ThemeEditorColorEdit`,
//! `ThemeEditorHexInput`, `ThemeEditorRename`, `ThemeEditorExport`, and `ThemeEditorImport`.

use crate::{
    app::App,
    models::AppState,
    ui::content::utils::split_cursor,
    ui::theme::{BUILTIN_DARK_COUNT, COLOR_SLOTS, ColorTheme},
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use super::{border_set, content_block};
// ── Entry points ──────────────────────────────────────────────────────────────

/// Top-level draw dispatcher for all `ThemeEditor*` states.
pub(super) fn draw_theme_editor_screen(f: &mut Frame, app: &App, area: Rect) {
    match app.state {
        AppState::ThemeEditor | AppState::ThemeEditorExport | AppState::ThemeEditorImport => {
            draw_main(f, app, area);
            if matches!(
                app.state,
                AppState::ThemeEditorExport | AppState::ThemeEditorImport
            ) {
                draw_text_input_popup(f, app);
            }
        }
        AppState::ThemeEditorNew => {
            draw_main(f, app, area);
            draw_clone_picker(f, app);
        }
        AppState::ThemeEditorColorEdit | AppState::ThemeEditorHexInput => {
            draw_color_edit(f, app, area);
            if app.state == AppState::ThemeEditorHexInput {
                draw_hex_input_popup(f, app);
            }
        }
        AppState::ThemeEditorRename => {
            draw_main(f, app, area);
            draw_text_input_popup(f, app);
        }
        _ => {}
    }
}

// ── Main editor (theme list + color preview) ──────────────────────────────────

fn draw_main(f: &mut Frame, app: &App, area: Rect) {
    let bg = Block::default().bg(app.theme.bg);
    f.render_widget(bg, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);

    draw_theme_list(f, app, cols[0]);
    draw_color_preview(f, app, cols[1]);
}

fn draw_theme_list(f: &mut Frame, app: &App, area: Rect) {
    let block = content_block(
        Line::from(" Themes ").fg(app.theme.accent).bold(),
        true,
        app.user_data.border_rounded,
        &app.theme,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let builtin_names = ColorTheme::builtin_names();
    let cursor = app.theme_editor.cursor;

    let mut items: Vec<ListItem> = Vec::new();

    // Dark themes
    for (i, name) in builtin_names.iter().take(BUILTIN_DARK_COUNT).enumerate() {
        let is_active = app.user_data.selected_theme != "custom"
            && ColorTheme::slug(name) == app.user_data.selected_theme;
        let is_cursor = i == cursor;
        let style = if is_cursor {
            Style::default()
                .fg(app.theme.bg_dark)
                .bg(app.theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text)
        };
        let marker = if is_active { " ●" } else { "  " };
        items.push(ListItem::new(
            Line::from(format!("{marker} {name}")).style(style),
        ));
    }

    // Light section header
    items.push(ListItem::new(
        Line::from(format!(
            "  {}",
            "─ Light ────".to_string() + &"─".repeat(inner.width.saturating_sub(13) as usize)
        ))
        .fg(app.theme.border),
    ));

    // Light themes
    for (i, name) in builtin_names.iter().skip(BUILTIN_DARK_COUNT).enumerate() {
        let cursor_idx = BUILTIN_DARK_COUNT + i;
        let is_active = app.user_data.selected_theme != "custom"
            && ColorTheme::slug(name) == app.user_data.selected_theme;
        let is_cursor = cursor_idx == cursor;
        let style = if is_cursor {
            Style::default()
                .fg(app.theme.bg_dark)
                .bg(app.theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text)
        };
        let marker = if is_active { " ●" } else { "  " };
        items.push(ListItem::new(
            Line::from(format!("{marker} {name}")).style(style),
        ));
    }

    // Custom section header
    items.push(ListItem::new(
        Line::from(format!(
            "  {}",
            "─ Custom ───".to_string() + &"─".repeat(inner.width.saturating_sub(13) as usize)
        ))
        .fg(app.theme.border),
    ));

    let sep_idx = builtin_names.len();

    for (i, ct) in app.user_data.custom_themes.iter().enumerate() {
        let abs_idx = sep_idx + 1 + i; // +1 for separator row (display only)
        let list_idx = builtin_names.len() + i; // index in cursor space (no separator)
        let is_active = app.user_data.selected_theme == "custom"
            && app.user_data.selected_custom_id == Some(ct.id);
        let is_cursor = list_idx == cursor;
        let style = if is_cursor {
            Style::default()
                .fg(app.theme.bg_dark)
                .bg(app.theme.link)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.text)
        };
        let marker = if is_active { " ●" } else { "  " };
        let _ = abs_idx; // used for layout only
        items.push(ListItem::new(
            Line::from(format!("{marker} {}", ct.name)).style(style),
        ));
    }

    if app.user_data.custom_themes.is_empty() {
        items.push(ListItem::new(
            Line::from("  (no custom themes)").fg(app.theme.muted),
        ));
    }

    // Adjust list_state cursor past section headers
    let display_cursor = if cursor < BUILTIN_DARK_COUNT {
        cursor
    } else if cursor < builtin_names.len() {
        cursor + 1 // skip "Light" section header
    } else {
        cursor + 2 // skip "Light" + "Custom" section headers
    };
    let mut list_state = ListState::default();
    list_state.select(Some(display_cursor));
    f.render_stateful_widget(List::new(items), inner, &mut list_state);
}

fn draw_color_preview(f: &mut Frame, app: &App, area: Rect) {
    let builtin_names = ColorTheme::builtin_names();
    let cursor = app.theme_editor.cursor;

    let (title_str, colors) = if cursor < builtin_names.len() {
        let name = builtin_names[cursor];
        let slug = ColorTheme::slug(name);
        let cols = ColorTheme::builtin(slug).map(|t| t.to_custom_colors());
        (name.to_string(), cols)
    } else {
        let idx = cursor - builtin_names.len();
        if let Some(ct) = app.user_data.custom_themes.get(idx) {
            (ct.name.clone(), Some(ct.colors.clone()))
        } else {
            ("—".to_string(), None)
        }
    };

    let block = content_block(
        Line::from(format!(" {title_str} "))
            .fg(app.theme.header)
            .bold(),
        false,
        app.user_data.border_rounded,
        &app.theme,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(colors) = colors else {
        return;
    };

    let lines: Vec<Line> = COLOR_SLOTS
        .iter()
        .enumerate()
        .map(|(i, (slot, label))| {
            let hex = colors.get(i);
            let swatch = get_swatch(app, hex);
            let (hashtag, rendered_hex) = to_renderable_hex(app, hex);
            Line::from(vec![
                format!("  {slot:<10} ").fg(app.theme.muted),
                swatch,
                Span::from(" "),
                hashtag,
                rendered_hex,
                Span::from(" "),
                label.fg(app.theme.border),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Clone picker popup ────────────────────────────────────────────────────────

fn draw_clone_picker(f: &mut Frame, app: &App) {
    let builtin_names = ColorTheme::builtin_names();
    let custom_count = app.user_data.custom_themes.len();
    let total = builtin_names.len() + custom_count;
    let popup_height = (total.min(16) + 2) as u16;
    let popup_width = 40u16;

    let area = f.area();
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(popup_height),
            Constraint::Fill(1),
        ])
        .split(area);
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(popup_width),
            Constraint::Fill(1),
        ])
        .split(vert[1]);
    let popup_area = horiz[1];

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(Line::from(" Clone From ").fg(app.theme.link).bold())
        .borders(Borders::ALL)
        .border_set(border_set(app.user_data.border_rounded))
        .border_style(Style::default().fg(app.theme.link))
        .bg(app.theme.bg);

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let cursor = app.theme_editor.clone_cursor;
    let mut items: Vec<ListItem> = builtin_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let style = if i == cursor {
                Style::default()
                    .fg(app.theme.bg_dark)
                    .bg(app.theme.link)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(app.theme.text)
            };
            ListItem::new(Line::from(format!("  {name}")).style(style))
        })
        .collect();

    for (i, ct) in app.user_data.custom_themes.iter().enumerate() {
        let abs = builtin_names.len() + i;
        let style = if abs == cursor {
            Style::default()
                .fg(app.theme.bg_dark)
                .bg(app.theme.link)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(app.theme.sky)
        };
        items.push(ListItem::new(
            Line::from(format!("  {} (custom)", ct.name)).style(style),
        ));
    }

    let mut list_state = ListState::default();
    list_state.select(Some(cursor));
    f.render_stateful_widget(List::new(items), inner, &mut list_state);
}

// ── Color-slot editor ─────────────────────────────────────────────────────────

fn draw_color_edit(f: &mut Frame, app: &App, area: Rect) {
    let bg = Block::default().bg(app.theme.bg);
    f.render_widget(bg, area);

    let theme_name = app
        .theme_editor
        .editing_id
        .and_then(|id| {
            app.user_data
                .custom_themes
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.name.clone())
        })
        .unwrap_or_else(|| "—".to_string());

    let block = content_block(
        Line::from(format!(" Edit: {theme_name} "))
            .fg(app.theme.link)
            .bold(),
        true,
        app.user_data.border_rounded,
        &app.theme,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let colors = app.theme_editor.editing_id.and_then(|id| {
        app.user_data
            .custom_themes
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.colors.clone())
    });

    let Some(colors) = colors else { return };
    let cursor = app.theme_editor.color_cursor;

    let lines: Vec<Line> = COLOR_SLOTS
        .iter()
        .enumerate()
        .map(|(i, (slot, label))| {
            let hex = colors.get(i);
            let is_cursor = i == cursor;

            let prefix = if is_cursor { " ▶ " } else { "   " };
            let row_style = if is_cursor {
                Style::default().bg(app.theme.selection)
            } else {
                Style::default()
            };
            let swatch = get_swatch(app, hex);
            let (hashtag, rendered_hex) = to_renderable_hex(app, hex);

            Line::from(vec![
                Span::styled(
                    format!("{prefix}{slot:<10} "),
                    row_style.fg(if is_cursor {
                        app.theme.accent
                    } else {
                        app.theme.muted
                    }),
                ),
                swatch,
                Span::from(" "),
                hashtag,
                rendered_hex.add_modifier(if is_cursor {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
                Span::from(" "),
                label.fg(app.theme.border),
            ])
        })
        .collect();

    f.render_widget(Paragraph::new(lines), inner);
}

// ── Shared text-input popup ───────────────────────────────────────────────────

fn draw_text_input_popup(f: &mut Frame, app: &App) {
    let (title, prompt) = match app.state {
        AppState::ThemeEditorRename => (" Rename Theme ", "New name:"),
        AppState::ThemeEditorExport => (" Export Theme ", "Export to path:"),
        AppState::ThemeEditorImport => (" Import Theme ", "Path to .toml file:"),
        _ => (" Input ", "Value:"),
    };

    let area = f.area();
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Fill(1),
        ])
        .split(area);
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(60),
            Constraint::Fill(1),
        ])
        .split(vert[1]);
    let popup_area = horiz[1];

    f.render_widget(Clear, popup_area);

    let block = content_block(
        Line::from(title).fg(app.theme.link).bold(),
        true,
        app.user_data.border_rounded,
        &app.theme,
    );
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let (before, cursor_ch, after) =
        split_cursor(&app.theme_editor.path_input, app.theme_editor.input_cursor);
    let text = vec![
        Line::from(prompt).fg(app.theme.muted),
        Line::from(vec![
            before.fg(app.theme.text).bold(),
            cursor_ch.fg(app.theme.bg).bg(app.theme.success).bold(),
            after.fg(app.theme.text).bold(),
        ]),
        Line::from(""),
        Line::from("Enter = confirm   Esc = cancel").fg(app.theme.border),
    ];
    f.render_widget(Paragraph::new(text), inner);
}

// ── Hex input popup (overlaid on color editor) ────────────────────────────────

fn draw_hex_input_popup(f: &mut Frame, app: &App) {
    let slot_name = COLOR_SLOTS
        .get(app.theme_editor.color_cursor)
        .map(|(s, _)| *s)
        .unwrap_or("color");

    let area = f.area();
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Fill(1),
        ])
        .split(area);
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(50),
            Constraint::Fill(1),
        ])
        .split(vert[1]);
    let popup_area = horiz[1];

    f.render_widget(Clear, popup_area);

    let block = content_block(
        Line::from(format!(" Edit: {slot_name} "))
            .fg(app.theme.accent)
            .bold(),
        true,
        app.user_data.border_rounded,
        &app.theme,
    );
    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let (before, cursor_ch, after) =
        split_cursor(&app.theme_editor.hex_input, app.theme_editor.input_cursor);
    let padding = 7 - app.theme_editor.hex_input.len();
    let cursor_space = if app.theme_editor.input_cursor == app.theme_editor.hex_input.len() {
        ""
    } else {
        " "
    };
    let swatch = get_swatch(app, &app.theme_editor.hex_input);

    let text = vec![
        Line::from("Hex color (#rrggbb):").fg(app.theme.muted),
        Line::from(vec![
            "#".fg(app.theme.muted).bold(),
            before.fg(app.theme.text).bold(),
            cursor_ch.fg(app.theme.bg).bg(app.theme.success).bold(),
            after.fg(app.theme.text).bold(),
            Span::from(format!("{}{:width$}", cursor_space, "", width = padding)),
            swatch,
        ]),
        Line::from("Enter = confirm   Esc = cancel").fg(app.theme.border),
    ];
    f.render_widget(Paragraph::new(text), inner);
}

// ── Color parsing helper ──────────────────────────────────────────────────────

/// Parse a `#rrggbb` hex string into a ratatui `Color::Rgb` for swatches.
fn parse_hex_color(hex: &str) -> Option<Color> {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Get the proper swatch symbol and color
fn get_swatch(app: &App, hex: &str) -> Span<'static> {
    let color = parse_hex_color(hex).unwrap_or(app.theme.error);
    let symbol = if parse_hex_color(hex).is_some() {
        "███"
    } else {
        "?¿?"
    };
    symbol.fg(color).bold()
}

fn to_renderable_hex<'a>(app: &App, hex: &'a str) -> (Span<'static>, Span<'a>) {
    let h = hex.trim_start_matches('#');

    ("#".fg(app.theme.muted), h.fg(app.theme.text))
}
