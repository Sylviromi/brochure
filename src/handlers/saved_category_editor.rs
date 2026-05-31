//! Key event handling for the saved-category editor and its sub-states.
//!
//! Covers `SavedCategoryEditor`, `SavedCategoryEditorRename`, `SavedCategoryEditorDeleteConfirm`,
//! and `SavedCategoryEditorNew`.

use crossterm::event::{KeyCode, KeyEvent};

use crate::{
    app::App,
    models::{AppState, SavedCategory},
    storage::save_user_data,
};

/// Handles key events for the `SavedCategoryEditor` list state.
///
/// `r` enters rename mode, `d` enters delete-confirmation mode, `n` enters new-category mode,
/// and Esc/`q` returns to `SavedCategoryList`.
pub(super) fn handle_saved_category_editor(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Up => {
            app.saved_cat_editor_scroll.move_up();
        }
        KeyCode::Down => {
            let len = app.user_data.saved_categories.len();
            app.saved_cat_editor_scroll.move_down(len);
        }
        KeyCode::Char('r') => {
            let cursor = app.saved_cat_editor_scroll.cursor;
            if cursor < app.user_data.saved_categories.len() {
                app.feed_editor.input.text = app.user_data.saved_categories[cursor].name.clone();
                app.feed_editor.input.cursor = app.feed_editor.input.text.chars().count();
                app.state = AppState::SavedCategoryEditorRename;
            }
        }
        KeyCode::Char('d') => {
            let cursor = app.saved_cat_editor_scroll.cursor;
            if cursor < app.user_data.saved_categories.len() {
                app.state = AppState::SavedCategoryEditorDeleteConfirm;
            }
        }
        KeyCode::Char('n') => {
            app.feed_editor.input.clear();
            app.state = AppState::SavedCategoryEditorNew;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.state = AppState::SavedCategoryList;
        }
        _ => {}
    }
}

/// Handles key events for the `SavedCategoryEditorDeleteConfirm` dialog.
///
/// Enter removes the category and all articles belonging to it from `user_data`, then persists the
/// change; Esc/`q` cancels and returns to the editor list.
pub(super) fn handle_saved_category_editor_delete_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let cursor = app.saved_cat_editor_scroll.cursor;
            if cursor < app.user_data.saved_categories.len() {
                let cat_id = app.user_data.saved_categories[cursor].id;
                let article_count = app
                    .user_data
                    .saved_articles
                    .iter()
                    .filter(|s| s.category_id == cat_id)
                    .count();
                app.user_data
                    .saved_articles
                    .retain(|s| s.category_id != cat_id);
                app.user_data.saved_categories.remove(cursor);
                let new_len = app.user_data.saved_categories.len();
                app.saved_cat_editor_scroll.clamp(new_len);
                let _ = save_user_data(&app.user_data);
                app.set_status(format!(
                    "Category deleted. {article_count} article(s) unsaved."
                ));
            }
            app.state = AppState::SavedCategoryEditor;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.state = AppState::SavedCategoryEditor;
        }
        _ => {}
    }
}

/// Handles key events for the `SavedCategoryEditorNew` text-input state.
///
/// Enter creates a new category with the typed name (silently skips duplicates), persists, and
/// returns to the editor list; Esc discards the input.
pub(super) fn handle_saved_category_editor_new(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let name = app.feed_editor.input.text.trim().to_string();
            if !name.is_empty() {
                // Reuse existing category if same name already exists.
                let already_exists = app
                    .user_data
                    .saved_categories
                    .iter()
                    .any(|c| c.name.eq_ignore_ascii_case(&name));
                if !already_exists {
                    let new_id = app
                        .user_data
                        .saved_categories
                        .iter()
                        .map(|c| c.id)
                        .max()
                        .unwrap_or(0)
                        + 1;
                    app.user_data.saved_categories.push(SavedCategory {
                        id: new_id,
                        name: name.clone(),
                    });
                    let _ = save_user_data(&app.user_data);
                    app.set_status(format!("Category '{name}' created."));
                } else {
                    app.set_status(format!("Category '{name}' already exists."));
                }
            }
            app.feed_editor.input.clear();
            app.state = AppState::SavedCategoryEditor;
        }
        KeyCode::Esc => {
            app.feed_editor.input.clear();
            app.state = AppState::SavedCategoryEditor;
        }
        _ => app.feed_editor.input.handle_key(key.code, None),
    }
}

/// Handles key events for the `SavedCategoryEditorRename` text-input state.
///
/// Enter overwrites the category name with the trimmed input and persists; Esc discards and
/// returns to the editor list.
pub(super) fn handle_saved_category_editor_rename(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => {
            let name = app.feed_editor.input.text.trim().to_string();
            if !name.is_empty() {
                if let Some(cat) = app
                    .user_data
                    .saved_categories
                    .get_mut(app.saved_cat_editor_scroll.cursor)
                {
                    cat.name = name;
                }
                let _ = save_user_data(&app.user_data);
                app.set_status("Category renamed.".to_string());
            }
            app.feed_editor.input.clear();
            app.state = AppState::SavedCategoryEditor;
        }
        KeyCode::Esc => {
            app.feed_editor.input.clear();
            app.state = AppState::SavedCategoryEditor;
        }
        _ => app.feed_editor.input.handle_key(key.code, None),
    }
}
