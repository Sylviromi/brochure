//! Key event handlers routed by app state.
//!
//! Dispatches keyboard input to state-specific handlers (feed list, article detail, settings, etc.).

pub(crate) mod article;
mod category_picker;
mod changelog;
mod feed_editor;
mod feed_list;
mod saved_category_editor;
mod settings;
mod theme_editor;

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::App,
    handlers::article::get_selected_article,
    models::{AppEvent, AppState},
};

/// Handles key events while zen mode is active (full-screen article reading).
fn handle_zen_mode(app: &mut App, key: KeyEvent, _tx: &UnboundedSender<AppEvent>) -> bool {
    match key.code {
        KeyCode::Char('z') | KeyCode::Esc => {
            app.zen_mode = false;
            false
        }
        KeyCode::Char('q') => true,
        KeyCode::Down | KeyCode::Char('j') => {
            let max = app
                .content_line_count
                .saturating_sub(app.content_area_height as usize) as u16;
            if let Some(article) = get_selected_article(app) {
                app.article_scroll.scroll_down(&article.link, max);
            }
            false
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(article) = get_selected_article(app) {
                app.article_scroll.scroll_up(&article.link);
            }
            false
        }
        KeyCode::PageDown => {
            let lines = app.content_area_height.saturating_sub(1);
            let max = app
                .content_line_count
                .saturating_sub(app.content_area_height as usize) as u16;
            if let Some(article) = get_selected_article(app) {
                let key = &article.link;
                for _ in 0..lines {
                    app.article_scroll.scroll_down(key, max);
                }
            }
            false
        }
        KeyCode::PageUp => {
            let lines = app.content_area_height.saturating_sub(1);
            if let Some(article) = get_selected_article(app) {
                let key = &article.link;
                for _ in 0..lines {
                    app.article_scroll.scroll_up(key);
                }
            }
            false
        }
        KeyCode::Home => {
            if let Some(article) = get_selected_article(app) {
                app.article_scroll.scroll_up(&article.link);
                // Scroll all the way to top by resetting to 0.
                // TextScroll only supports incremental up/down, so we
                // repeatedly scroll up by a large amount. The scroll_up
                // method uses saturating_sub so this bottoms out at 0.
                for _ in 0..u16::MAX {
                    app.article_scroll.scroll_up(&article.link);
                }
            }
            false
        }
        KeyCode::End => {
            let max = app
                .content_line_count
                .saturating_sub(app.content_area_height as usize) as u16;
            if let Some(article) = get_selected_article(app) {
                for _ in 0..u16::MAX {
                    app.article_scroll.scroll_down(&article.link, max);
                }
            }
            false
        }
        KeyCode::Char('n') => {
            app.move_article_cursor(true);
            false
        }
        KeyCode::Char('p') => {
            app.move_article_cursor(false);
            false
        }
        _ => false,
    }
}

/// Route a key event to the correct handler based on the current app state.
pub async fn handle_key(app: &mut App, key: KeyEvent, tx: &UnboundedSender<AppEvent>) -> bool {
    if app.zen_mode {
        return handle_zen_mode(app, key, tx);
    }
    if app.update_available.is_some() {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                app.update_popup_scroll = app.update_popup_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.update_popup_scroll = app.update_popup_scroll.saturating_add(1);
            }
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                app.update_available = None;
                app.update_popup_scroll = 0;
            }
            _ => {}
        }
        return false;
    }
    match app.state {
        AppState::AddFeed => settings::handle_add_feed(app, key, tx),
        AppState::SettingsList => return settings::handle_settings(app, key),
        AppState::OPMLExportPath | AppState::OPMLImportPath => {
            settings::handle_opml_path(app, key, tx)
        }
        AppState::ClearData => settings::handle_confirm_delete_all(app, key),
        AppState::ClearArticleCache => settings::handle_confirm_clear_cache(app, key),
        AppState::ArticleList | AppState::ArticleDetail => {
            return article::handle_article(app, key, tx).await;
        }
        AppState::FeedList => {
            let should_quit = feed_list::handle_feed_list(app, key, tx);
            return should_quit;
        }
        AppState::SavedCategoryList => return feed_list::handle_saved_feed_list(app, key),
        AppState::FeedEditor | AppState::FeedEditorRename => {
            feed_editor::handle_feed_editor(app, key, tx)
        }
        AppState::CategoryPicker => category_picker::handle_category_picker(app, key, tx),
        AppState::SavedCategoryEditor => {
            saved_category_editor::handle_saved_category_editor(app, key)
        }
        AppState::SavedCategoryEditorRename => {
            saved_category_editor::handle_saved_category_editor_rename(app, key)
        }
        AppState::SavedCategoryEditorDeleteConfirm => {
            saved_category_editor::handle_saved_category_editor_delete_confirm(app, key)
        }
        AppState::SavedCategoryEditorNew => {
            saved_category_editor::handle_saved_category_editor_new(app, key)
        }
        AppState::Changelog => return changelog::handle_changelog(app, key),
        AppState::ThemeEditor => theme_editor::handle_theme_editor(app, key),
        AppState::ThemeEditorNew => theme_editor::handle_theme_editor_new(app, key),
        AppState::ThemeEditorColorEdit => theme_editor::handle_theme_editor_color_edit(app, key),
        AppState::ThemeEditorHexInput => theme_editor::handle_theme_editor_hex_input(app, key),
        AppState::ThemeEditorRename => theme_editor::handle_theme_editor_rename(app, key),
        AppState::ThemeEditorExport => theme_editor::handle_theme_editor_export(app, key),
        AppState::ThemeEditorImport => theme_editor::handle_theme_editor_import(app, key),
    }
    false
}
