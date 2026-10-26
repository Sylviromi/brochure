//! Key handlers for the main feed list and saved articles category list views.
//!
//! Manages navigation, feed refresh, feed editor entry, and category collapse/expand.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{App, feeds_in_category},
    handlers::article::copy_with_status,
    models::{AppEvent, AppState, FeedEditorMode, FeedTreeItem},
};

/// Refresh every feed in a given category.
fn refresh_category(app: &mut App, cat_id: u64, tx: &UnboundedSender<AppEvent>) {
    let feed_indices = feeds_in_category(cat_id, &app.categories, &app.feeds);
    if feed_indices.is_empty() {
        return;
    }
    app.refresh_feeds(&feed_indices, tx);
}

/// Refresh everything (local feeds + a FreshRSS sync when configured).
fn refresh_all_feeds(app: &mut App, tx: &UnboundedSender<AppEvent>) {
    app.refresh_all(tx);
}

/// Handle key input while viewing the main feed list; supports navigation, refresh, and feed editor entry.
pub(super) fn handle_feed_list(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Char('r') if !app.in_saved_context => {
            if app.in_all_feeds_context {
                refresh_all_feeds(app, tx);
            } else if let Some(cat_id) = app.selected_sidebar_category {
                refresh_category(app, cat_id, tx);
            } else {
                app.refresh_feed_at(app.selected_feed, tx);
            }
        }
        KeyCode::Char('R') if !app.in_saved_context => {
            refresh_all_feeds(app, tx);
        }
        KeyCode::Down => app.next(),
        KeyCode::Up => app.previous(),
        KeyCode::Tab => app.switch_tab_right(),
        KeyCode::BackTab => app.switch_tab_left(),
        KeyCode::Enter => app.select(),
        KeyCode::Char('e') => {
            app.feed_editor.cursor = 0;
            app.feed_editor.collapsed = app.sidebar_collapsed.clone();
            app.feed_editor.mode = FeedEditorMode::Normal;
            app.state = AppState::FeedEditor;
        }
        // Jump to top of feed list
        KeyCode::Char('g') => app.set_sidebar_cursor(0),
        KeyCode::Char('C') => {
            // Toggle collapse/expand all categories
            let all_collapsed = app
                .categories
                .iter()
                .all(|c| app.sidebar_collapsed.contains(&c.id));
            if all_collapsed {
                app.sidebar_collapsed.clear();
            } else {
                for category in &app.categories {
                    app.sidebar_collapsed.insert(category.id);
                }
            }
            app.persist_sidebar_collapsed();
            app.set_sidebar_cursor(0);
        }
        KeyCode::Char('y') if !app.in_saved_context => {
            if let Some(FeedTreeItem::Feed { feeds_idx, .. }) =
                app.sidebar_items().get(app.sidebar_cursor)
                && let Some(feed) = app.feeds.get(*feeds_idx)
            {
                let url = feed.url.clone();
                copy_with_status(app, &url, "Copied feed URL");
            }
        }
        KeyCode::Char(' ') => {
            if let Some(FeedTreeItem::Category { id, .. }) =
                app.sidebar_items().get(app.sidebar_cursor)
            {
                app.toggle_category_collapse(*id);
            }
        }
        _ => {}
    }
    false
}

/// Handle key input while viewing the saved articles category list.
pub(super) fn handle_saved_feed_list(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Tab => app.switch_tab_right(),
        KeyCode::BackTab => app.switch_tab_left(),
        KeyCode::Char('j') | KeyCode::Down => app.next(),
        KeyCode::Char('k') | KeyCode::Up => app.previous(),
        KeyCode::Enter => app.select(),
        KeyCode::Char('e') => {
            app.saved_cat_editor_scroll.set(0);
            app.state = AppState::SavedCategoryEditor;
        }
        KeyCode::Esc => app.unselect(),
        _ => {}
    }
    false
}
