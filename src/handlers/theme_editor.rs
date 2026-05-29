//! Key event handlers for the full-screen theme editor and its sub-states.
//!
//! Covers `ThemeEditor` (browse/select/manage), `ThemeEditorNew` (clone picker),
//! `ThemeEditorColorEdit` (slot list), `ThemeEditorHexInput` (hex text entry),
//! `ThemeEditorRename`, `ThemeEditorExport`, and `ThemeEditorImport`.

use crossterm::event::{KeyCode, KeyEvent};

use super::handle_text_input;
use crate::{
    app::{App, cursor_next, cursor_prev, resolve_theme},
    models::{AppState, CustomTheme},
    storage::{default_export_path, expand_home_dir, save_user_data},
    ui::theme::{COLOR_SLOTS, ColorTheme},
};
// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns the total number of rows in the theme editor list (builtins + custom).
fn total_themes(app: &App) -> usize {
    ColorTheme::builtin_names().len() + app.user_data.custom_themes.len()
}

/// Returns true if `cursor` points at a built-in theme.
fn is_builtin(cursor: usize) -> bool {
    cursor < ColorTheme::builtin_names().len()
}

/// Returns the `custom_themes` index for `cursor` (caller must verify `!is_builtin`).
fn custom_idx(cursor: usize) -> usize {
    cursor - ColorTheme::builtin_names().len()
}

/// Validate a hex color string (`#rrggbb` or `rrggbb`).
fn hex_valid(hex: &str) -> bool {
    let h = hex.trim_start_matches('#');
    h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit())
}

/// Ensure the hex string has a leading `#`.
fn normalize_hex(hex: &str) -> String {
    if hex.starts_with('#') {
        hex.to_string()
    } else {
        format!("#{hex}")
    }
}

/// Allocate the next custom theme ID (max existing + 1, or 1 if none exist).
fn next_id(app: &App) -> u32 {
    app.user_data
        .custom_themes
        .iter()
        .map(|t| t.id)
        .max()
        .unwrap_or(0)
        + 1
}

/// Resolve the theme at the current cursor position without modifying user_data.
/// Used for live preview as the user navigates the theme list.
fn resolve_at_cursor(app: &App) -> ColorTheme {
    if is_builtin(app.theme_editor.cursor) {
        let name = ColorTheme::builtin_names()[app.theme_editor.cursor];
        let slug = ColorTheme::slug(name);
        ColorTheme::builtin(slug).unwrap_or_else(ColorTheme::catppuccin_mocha)
    } else {
        let idx = custom_idx(app.theme_editor.cursor);
        app.user_data
            .custom_themes
            .get(idx)
            .and_then(|ct| ColorTheme::from_custom_theme(ct).ok())
            .unwrap_or_else(ColorTheme::catppuccin_mocha)
    }
}

// ── Main editor ───────────────────────────────────────────────────────────────

/// Handles key events for `ThemeEditor` — the full-screen theme list.
pub(super) fn handle_theme_editor(app: &mut App, key: KeyEvent) {
    let total = total_themes(app);

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.theme = resolve_theme(&app.user_data);
            app.state = AppState::SettingsList;
        }

        KeyCode::Up | KeyCode::Char('k') if total > 0 => {
            if let Some(c) = cursor_prev(app.theme_editor.cursor, total, app.user_data.scroll_loop)
            {
                app.theme_editor.cursor = c;
                app.theme = resolve_at_cursor(app);
            }
        }
        KeyCode::Down | KeyCode::Char('j') if total > 0 => {
            if let Some(c) = cursor_next(app.theme_editor.cursor, total, app.user_data.scroll_loop)
            {
                app.theme_editor.cursor = c;
                app.theme = resolve_at_cursor(app);
            }
        }

        // Enter — activate selected theme.
        KeyCode::Enter => {
            if is_builtin(app.theme_editor.cursor) {
                let name = ColorTheme::builtin_names()[app.theme_editor.cursor];
                let slug = ColorTheme::slug(name);
                app.user_data.selected_theme = slug.to_string();
                app.user_data.selected_custom_id = None;
                app.theme = resolve_theme(&app.user_data);
                let _ = save_user_data(&app.user_data);
                app.set_status(format!("Theme: {name}"));
            } else {
                let idx = custom_idx(app.theme_editor.cursor);
                if let Some(ct) = app.user_data.custom_themes.get(idx) {
                    let id = ct.id;
                    let name = ct.name.clone();
                    app.user_data.selected_theme = "custom".to_string();
                    app.user_data.selected_custom_id = Some(id);
                    app.theme = resolve_theme(&app.user_data);
                    let _ = save_user_data(&app.user_data);
                    app.set_status(format!("Theme: {name}"));
                }
            }
        }

        // n — new custom theme (open clone picker).
        KeyCode::Char('n') => {
            app.theme_editor.clone_cursor = 0;
            app.state = AppState::ThemeEditorNew;
        }

        // e — edit colors (custom only).
        KeyCode::Char('e') => {
            if is_builtin(app.theme_editor.cursor) {
                app.set_status(
                    "Built-in themes are read-only — press 'n' to create one based on it."
                        .to_string(),
                );
            } else {
                let idx = custom_idx(app.theme_editor.cursor);
                if let Some(ct) = app.user_data.custom_themes.get(idx) {
                    app.theme_editor.editing_id = Some(ct.id);
                    app.theme_editor.color_cursor = 0;
                    app.state = AppState::ThemeEditorColorEdit;
                }
            }
        }

        // r — rename (custom only).
        KeyCode::Char('r') => {
            if is_builtin(app.theme_editor.cursor) {
                app.set_status("Built-in themes cannot be renamed.".to_string());
            } else {
                let idx = custom_idx(app.theme_editor.cursor);
                if let Some(ct) = app.user_data.custom_themes.get(idx) {
                    app.theme_editor.path_input = ct.name.clone();
                    app.theme_editor.input_cursor = app.theme_editor.path_input.chars().count();
                    app.theme_editor.editing_id = Some(ct.id);
                    app.state = AppState::ThemeEditorRename;
                }
            }
        }

        // d — delete (custom only).
        KeyCode::Char('d') => {
            if is_builtin(app.theme_editor.cursor) {
                app.set_status("Built-in themes cannot be deleted.".to_string());
            } else {
                let idx = custom_idx(app.theme_editor.cursor);
                if let Some(ct) = app.user_data.custom_themes.get(idx) {
                    let id = ct.id;
                    let was_active = app.user_data.selected_custom_id == Some(id);
                    app.user_data.custom_themes.retain(|t| t.id != id);
                    if was_active {
                        app.user_data.selected_theme = "catppuccin-mocha".to_string();
                        app.user_data.selected_custom_id = None;
                        app.theme = ColorTheme::catppuccin_mocha();
                    }
                    let new_total = total_themes(app);
                    if new_total > 0 {
                        app.theme_editor.cursor = app.theme_editor.cursor.min(new_total - 1);
                    }
                    let _ = save_user_data(&app.user_data);
                    app.set_status("Custom theme deleted.".to_string());
                }
            }
        }

        // x — export selected theme to a TOML file.
        KeyCode::Char('x') => {
            let default_name = if is_builtin(app.theme_editor.cursor) {
                ColorTheme::slug(ColorTheme::builtin_names()[app.theme_editor.cursor]).to_string()
            } else {
                let idx = custom_idx(app.theme_editor.cursor);
                app.user_data
                    .custom_themes
                    .get(idx)
                    .map(|ct| ct.name.to_lowercase().replace(' ', "-"))
                    .unwrap_or_else(|| "theme".to_string())
            };
            let base = default_export_path();
            let export_path =
                std::path::PathBuf::from(&base).with_file_name(format!("{default_name}.toml"));
            app.theme_editor.path_input = export_path.display().to_string();
            app.theme_editor.input_cursor = app.theme_editor.path_input.chars().count();
            app.state = AppState::ThemeEditorExport;
        }

        // i — import a theme from a TOML file.
        KeyCode::Char('i') => {
            app.theme_editor.path_input.clear();
            app.theme_editor.input_cursor = 0;
            app.state = AppState::ThemeEditorImport;
        }

        _ => {}
    }
}

// ── Clone picker ──────────────────────────────────────────────────────────────

/// Handles key events for `ThemeEditorNew` — the clone-from picker.
pub(super) fn handle_theme_editor_new(app: &mut App, key: KeyEvent) {
    let total = total_themes(app);

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.state = AppState::ThemeEditor;
        }
        KeyCode::Up | KeyCode::Char('k') if total > 0 => {
            if let Some(c) = cursor_prev(
                app.theme_editor.clone_cursor,
                total,
                app.user_data.scroll_loop,
            ) {
                app.theme_editor.clone_cursor = c;
            }
        }
        KeyCode::Down | KeyCode::Char('j') if total > 0 => {
            if let Some(c) = cursor_next(
                app.theme_editor.clone_cursor,
                total,
                app.user_data.scroll_loop,
            ) {
                app.theme_editor.clone_cursor = c;
            }
        }
        KeyCode::Enter => {
            let colors = if is_builtin(app.theme_editor.clone_cursor) {
                let slug =
                    ColorTheme::slug(ColorTheme::builtin_names()[app.theme_editor.clone_cursor]);
                ColorTheme::builtin(slug).map(|t| t.to_custom_colors())
            } else {
                let idx = custom_idx(app.theme_editor.clone_cursor);
                app.user_data
                    .custom_themes
                    .get(idx)
                    .map(|ct| ct.colors.clone())
            };

            if let Some(colors) = colors {
                let base_name = if is_builtin(app.theme_editor.clone_cursor) {
                    ColorTheme::builtin_names()[app.theme_editor.clone_cursor].to_string()
                } else {
                    let idx = custom_idx(app.theme_editor.clone_cursor);
                    app.user_data
                        .custom_themes
                        .get(idx)
                        .map(|t| t.name.clone())
                        .unwrap_or_default()
                };
                let id = next_id(app);
                let name = format!("{base_name} (copy)");
                app.user_data.custom_themes.push(CustomTheme {
                    id,
                    name: name.clone(),
                    colors,
                });
                // Position cursor on the new theme and immediately open rename.
                app.theme_editor.cursor =
                    ColorTheme::builtin_names().len() + app.user_data.custom_themes.len() - 1;
                let _ = save_user_data(&app.user_data);
                app.theme_editor.path_input = name;
                app.theme_editor.input_cursor = app.theme_editor.path_input.chars().count();
                app.theme_editor.editing_id = Some(id);
                app.state = AppState::ThemeEditorRename;
            }
        }
        _ => {}
    }
}

// ── Color-slot editor ─────────────────────────────────────────────────────────

/// Handles key events for `ThemeEditorColorEdit` — the 14-slot color list.
pub(super) fn handle_theme_editor_color_edit(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc | KeyCode::Char('s') | KeyCode::Char('q') => {
            // Refresh active theme if we just finished editing it.
            if app.user_data.selected_theme == "custom"
                && app.user_data.selected_custom_id == app.theme_editor.editing_id
            {
                app.theme = resolve_theme(&app.user_data);
            }
            app.state = AppState::ThemeEditor;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(c) = cursor_prev(
                app.theme_editor.color_cursor,
                COLOR_SLOTS.len(),
                app.user_data.scroll_loop,
            ) {
                app.theme_editor.color_cursor = c;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(c) = cursor_next(
                app.theme_editor.color_cursor,
                COLOR_SLOTS.len(),
                app.user_data.scroll_loop,
            ) {
                app.theme_editor.color_cursor = c;
            }
        }
        KeyCode::Enter => {
            if let Some(id) = app.theme_editor.editing_id
                && let Some(ct) = app.user_data.custom_themes.iter().find(|t| t.id == id)
            {
                app.theme_editor.hex_input = ct
                    .colors
                    .get(app.theme_editor.color_cursor)
                    .trim_start_matches('#')
                    .to_string();
                app.theme_editor.input_cursor = app.theme_editor.hex_input.len();
                app.state = AppState::ThemeEditorHexInput;
            }
        }
        _ => {}
    }
}

// ── Hex value input ───────────────────────────────────────────────────────────

/// Handles key events for `ThemeEditorHexInput` — inline hex entry for a single slot.
pub(super) fn handle_theme_editor_hex_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.theme_editor.hex_input.clear();
            app.theme_editor.input_cursor = 0;
            app.state = AppState::ThemeEditorColorEdit;
        }
        KeyCode::Enter => {
            let raw = app.theme_editor.hex_input.trim().to_string();
            if !hex_valid(&raw) {
                app.set_status(format!("'{raw}' is not a valid hex color — use #rrggbb"));
                return;
            }
            let hex = normalize_hex(&raw);
            if let Some(id) = app.theme_editor.editing_id
                && let Some(ct) = app.user_data.custom_themes.iter_mut().find(|t| t.id == id)
            {
                ct.colors.set(app.theme_editor.color_cursor, hex);
                let _ = save_user_data(&app.user_data);
            }
            app.theme_editor.hex_input.clear();
            app.theme_editor.input_cursor = 0;
            app.state = AppState::ThemeEditorColorEdit;
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Backspace | KeyCode::Char(_) => {
            handle_text_input(
                &mut app.theme_editor.hex_input,
                &mut app.theme_editor.input_cursor,
                key.code,
                Some(6),
            );
        }
        _ => {}
    }
}

// ── Rename ────────────────────────────────────────────────────────────────────

/// Handles key events for `ThemeEditorRename` — text input for a custom theme name.
pub(super) fn handle_theme_editor_rename(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.theme_editor.path_input.clear();
            app.state = AppState::ThemeEditor;
        }
        KeyCode::Enter => {
            let new_name = app.theme_editor.path_input.trim().to_string();
            if new_name.is_empty() {
                app.set_status("Name cannot be empty.".to_string());
                return;
            }
            if let Some(id) = app.theme_editor.editing_id {
                if let Some(ct) = app.user_data.custom_themes.iter_mut().find(|t| t.id == id) {
                    ct.name = new_name.clone();
                }
                // Also update the live theme name if this is the active theme.
                if app.user_data.selected_custom_id == Some(id) {
                    app.theme.name = new_name;
                }
                let _ = save_user_data(&app.user_data);
            }
            app.theme_editor.path_input.clear();
            app.state = AppState::ThemeEditor;
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Backspace | KeyCode::Char(_) => {
            handle_text_input(
                &mut app.theme_editor.path_input,
                &mut app.theme_editor.input_cursor,
                key.code,
                None,
            );
        }
        _ => {}
    }
}

// ── Export ────────────────────────────────────────────────────────────────────

/// Handles key events for `ThemeEditorExport` — path input for writing a TOML file.
pub(super) fn handle_theme_editor_export(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.theme_editor.path_input.clear();
            app.state = AppState::ThemeEditor;
        }
        KeyCode::Enter => {
            let path = expand_home_dir(&app.theme_editor.path_input);
            let toml = if is_builtin(app.theme_editor.cursor) {
                let name = ColorTheme::builtin_names()[app.theme_editor.cursor];
                let slug = ColorTheme::slug(name);
                ColorTheme::builtin(slug).map(|t| t.to_custom_colors().to_toml(name))
            } else {
                let idx = custom_idx(app.theme_editor.cursor);
                app.user_data
                    .custom_themes
                    .get(idx)
                    .map(|ct| ct.colors.to_toml(&ct.name))
            };
            if let Some(content) = toml {
                match std::fs::write(&path, content) {
                    Ok(_) => {
                        app.set_status(format!("Exported to {path}"));
                        app.theme_editor.path_input.clear();
                        app.state = AppState::ThemeEditor;
                    }
                    Err(e) => app.set_status(format!("Export failed: {e}")),
                }
            }
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Backspace | KeyCode::Char(_) => {
            handle_text_input(
                &mut app.theme_editor.path_input,
                &mut app.theme_editor.input_cursor,
                key.code,
                None,
            );
        }
        _ => {}
    }
}

// ── Import ────────────────────────────────────────────────────────────────────

/// Handles key events for `ThemeEditorImport` — path input for reading a TOML file.
pub(super) fn handle_theme_editor_import(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.theme_editor.path_input.clear();
            app.state = AppState::ThemeEditor;
        }
        KeyCode::Enter => {
            let path = expand_home_dir(&app.theme_editor.path_input);
            match std::fs::read_to_string(&path) {
                Err(e) => {
                    app.set_status(format!("Cannot read file: {e}"));
                }
                Ok(src) => match ColorTheme::from_toml_str(&src) {
                    Err(e) => {
                        app.set_status(format!("Parse error: {e}"));
                    }
                    Ok(theme) => {
                        let id = next_id(app);
                        let colors = theme.to_custom_colors();
                        let name = theme.name.clone();
                        app.user_data.custom_themes.push(CustomTheme {
                            id,
                            name: name.clone(),
                            colors,
                        });
                        app.theme_editor.cursor = ColorTheme::builtin_names().len()
                            + app.user_data.custom_themes.len()
                            - 1;
                        let _ = save_user_data(&app.user_data);
                        app.set_status(format!("Imported: {name}"));
                        app.theme_editor.path_input.clear();
                        app.state = AppState::ThemeEditor;
                    }
                },
            }
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Backspace | KeyCode::Char(_) => {
            handle_text_input(
                &mut app.theme_editor.path_input,
                &mut app.theme_editor.input_cursor,
                key.code,
                None,
            );
        }
        _ => {}
    }
}
