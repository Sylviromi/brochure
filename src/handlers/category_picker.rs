//! Key event handling for the CategoryPicker overlay.
//!
//! Manages the save-to-category flow: list navigation, new-category text input,
//! and save/unsave operations for the currently selected article.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use super::article::get_selected_article;
use crate::{
    app::App,
    models::{AppEvent, AppState, SavedArticle, SavedCategory},
    storage::save_user_data,
};

/// Handles key events for the `CategoryPicker` overlay.
///
/// Manages both the list-navigation mode and the inline text-input mode for creating a new
/// category.  Saves or unsaves the currently selected article when the user confirms a choice.
pub(super) fn handle_category_picker(
    app: &mut App,
    key: KeyEvent,
    _tx: &UnboundedSender<AppEvent>,
) {
    let cats_len = app.user_data.saved_categories.len();
    let article_is_saved = get_selected_article(app).is_some_and(|art| {
        app.user_data
            .saved_articles
            .iter()
            .any(|s| s.article.link == art.link)
    });
    let total_items = if article_is_saved {
        cats_len + 2
    } else {
        cats_len + 1
    };

    if app.category_picker.new_mode {
        match key.code {
            KeyCode::Enter => {
                let name = app.category_picker.input.text.trim().to_string();
                if !name.is_empty() {
                    let target_id = app
                        .user_data
                        .saved_categories
                        .iter()
                        .find(|c| c.name.eq_ignore_ascii_case(&name))
                        .map(|c| c.id)
                        .unwrap_or_else(|| {
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
                            new_id
                        });
                    save_to_category(app, target_id);
                    app.set_status(format!("Saved to '{name}'!"));
                }
                app.category_picker.new_mode = false;
                app.category_picker.input.clear();
                app.state = app.category_picker.return_state.clone();
            }
            KeyCode::Esc => {
                app.category_picker.new_mode = false;
                app.category_picker.input.clear();
            }
            _ => app.category_picker.input.handle_key(key.code, None),
        }
        return;
    }

    match key.code {
        KeyCode::Up => {
            app.category_picker.cursor = app
                .category_picker
                .cursor
                .checked_sub(1)
                .unwrap_or(total_items - 1);
        }
        KeyCode::Down => {
            app.category_picker.cursor = (app.category_picker.cursor + 1) % total_items;
        }
        KeyCode::Enter => {
            if app.category_picker.cursor < cats_len {
                let cat_id = app.user_data.saved_categories[app.category_picker.cursor].id;
                let cat_name = app.user_data.saved_categories[app.category_picker.cursor]
                    .name
                    .clone();
                save_to_category(app, cat_id);
                app.set_status(format!("Saved to '{cat_name}'!"));
                app.state = app.category_picker.return_state.clone();
            } else if app.category_picker.cursor == cats_len {
                app.category_picker.new_mode = true;
                app.category_picker.input.clear();
            } else if article_is_saved {
                unsave_article(app);
                if app.state == AppState::CategoryPicker {
                    app.state = app.category_picker.return_state.clone();
                }
            }
        }
        KeyCode::Esc => {
            app.state = app.category_picker.return_state.clone();
        }
        _ => {}
    }
}

/// Opens the category picker overlay for the currently selected article.
///
/// Pre-selects the cursor on the article's current category if it is already saved.
pub(super) fn open_category_picker(app: &mut App) {
    let article = match get_selected_article(app) {
        Some(a) => a,
        None => return,
    };

    let current_cat_idx = app
        .user_data
        .saved_articles
        .iter()
        .find(|s| s.article.link == article.link)
        .and_then(|s| {
            app.user_data
                .saved_categories
                .iter()
                .position(|c| c.id == s.category_id)
        });

    app.category_picker.cursor = current_cat_idx.unwrap_or(0);
    app.category_picker.new_mode = false;
    app.category_picker.input.clear();
    app.category_picker.return_state = app.state.clone();
    app.state = AppState::CategoryPicker;
}

/// Saves the currently selected article to the given category, or moves it if already saved.
///
/// Persists `user_data` to disk and syncs the saved-view preview when in saved context.
fn save_to_category(app: &mut App, category_id: u32) {
    let article = match get_selected_article(app) {
        Some(a) => a,
        None => return,
    };

    if let Some(s) = app
        .user_data
        .saved_articles
        .iter_mut()
        .find(|s| s.article.link == article.link)
    {
        s.category_id = category_id;
    } else {
        app.user_data.saved_articles.push(SavedArticle {
            article: article.clone(),
            category_id,
        });
    }

    update_is_saved_flag(app, true);
    let _ = save_user_data(&app.user_data);
    if app.in_saved_context {
        app.sync_saved_preview();
        if !app.in_saved_context {
            app.selected_article = 0;
            if matches!(app.state, AppState::ArticleList | AppState::ArticleDetail) {
                app.state = AppState::SavedCategoryList;
            }
        } else if app.selected_article >= app.saved_view_articles.len() {
            app.selected_article = app.saved_view_articles.len().saturating_sub(1);
        }
    }
}

/// Removes the currently selected article from saved articles and adjusts the saved view.
///
/// Also clamps `selected_article` to a valid index when the saved-view list shrinks.
fn unsave_article(app: &mut App) {
    let article = match get_selected_article(app) {
        Some(a) => a,
        None => return,
    };

    app.user_data
        .saved_articles
        .retain(|s| s.article.link != article.link);
    update_is_saved_flag(app, false);

    if app.in_saved_context {
        app.saved_view_articles.retain(|a| a.link != article.link);
        if app.saved_view_articles.is_empty() {
            app.in_saved_context = false;
            app.selected_article = 0;
            if matches!(
                app.state,
                AppState::ArticleList | AppState::ArticleDetail | AppState::CategoryPicker
            ) {
                app.state = AppState::SavedCategoryList;
            }
        } else if app.selected_article >= app.saved_view_articles.len() {
            app.selected_article = app.saved_view_articles.len().saturating_sub(1);
        }
    }

    app.set_status("Article unsaved.");
    let _ = save_user_data(&app.user_data);
}

/// Updates the `is_saved` flag on the in-memory article that is currently selected.
///
/// Handles all three view contexts: regular feed, category view, and saved view, including
/// back-propagation to the source feed when in saved or category context.
fn update_is_saved_flag(app: &mut App, is_saved: bool) {
    if app.in_category_context {
        if let Some(&(fi, ai)) = app.category_view_articles.get(app.selected_article)
            && let Some(art) = app.feeds.get_mut(fi).and_then(|f| f.articles.get_mut(ai))
        {
            art.is_saved = is_saved;
        }
    } else if app.in_saved_context {
        if let Some(art) = app.saved_view_articles.get_mut(app.selected_article) {
            art.is_saved = is_saved;
            let link = art.link.clone();
            let source_feed = art.source_feed.clone();
            if let Some(feed) = app.feeds.iter_mut().find(|f| f.title == source_feed)
                && let Some(src) = feed.articles.iter_mut().find(|a| a.link == link)
            {
                src.is_saved = is_saved;
            }
        }
    } else if let Some(art) = app
        .feeds
        .get_mut(app.selected_feed)
        .and_then(|f| f.articles.get_mut(app.selected_article))
    {
        art.is_saved = is_saved;
    }
}
