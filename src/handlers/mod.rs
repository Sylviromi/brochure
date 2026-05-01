//! Key event handlers routed by app state.
//!
//! Dispatches keyboard input to state-specific handlers (feed list, article detail, settings, etc.).

mod article;
mod changelog;
mod feed_editor;
mod feed_list;
mod settings;

use crossterm::event::KeyEvent;
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::App,
    models::{AppEvent, AppState},
};

/// Route a key event to the correct handler based on the current app state.
pub async fn handle_key(app: &mut App, key: KeyEvent, tx: &UnboundedSender<AppEvent>) -> bool {
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
        AppState::FeedList => return feed_list::handle_feed_list(app, key, tx),
        AppState::SavedCategoryList => return feed_list::handle_saved_feed_list(app, key),
        AppState::FeedEditor | AppState::FeedEditorRename => {
            feed_editor::handle_feed_editor(app, key, tx)
        }
        AppState::CategoryPicker => article::handle_category_picker(app, key, tx),
        AppState::SavedCategoryEditor => settings::handle_saved_category_editor(app, key),
        AppState::SavedCategoryEditorRename => {
            settings::handle_saved_category_editor_rename(app, key)
        }
        AppState::SavedCategoryEditorDeleteConfirm => {
            settings::handle_saved_category_editor_delete_confirm(app, key)
        }
        AppState::SavedCategoryEditorNew => settings::handle_saved_category_editor_new(app, key),
        AppState::Changelog => return changelog::handle_changelog(app, key),
    }
    false
}
