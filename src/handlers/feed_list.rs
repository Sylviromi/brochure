use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{visible_tree_items, App},
    fetch::fetch_feed,
    models::{AppEvent, AppState, FeedEditorMode, FeedTreeItem},
};

pub(super) fn handle_feed_list(
    app: &mut App,
    key: KeyEvent,
    tx: &UnboundedSender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Char('r') if !app.in_saved_context => {
            let idx = app.selected_feed;
            if let Some(feed) = app.feeds.get_mut(idx) {
                let url = feed.url.clone();
                let title = feed.title.clone();
                feed.fetched = false;
                feed.fetch_error = None;
                app.set_status(format!("Refreshing {title}..."));
                app.feeds_pending += 1;
                app.feeds_total += 1;
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let result = fetch_feed(&url).await;
                    let _ = tx2.send(AppEvent::FeedFetched(idx, result));
                });
            }
        }
        KeyCode::Char('R') if !app.in_saved_context => {
            let count = app.feeds.iter().filter(|f| f.url != crate::models::FAVORITES_URL).count();
            if count > 0 {
                app.feeds_total += count;
                app.feeds_pending += count;
                app.set_status("Fetching all feeds...");
                for (idx, feed) in app.feeds.iter_mut().enumerate() {
                    if feed.url == crate::models::FAVORITES_URL { continue; }
                    feed.fetched = false;
                    feed.fetch_error = None;
                    let url = feed.url.clone();
                    let tx2 = tx.clone();
                    tokio::spawn(async move {
                        let result = fetch_feed(&url).await;
                        let _ = tx2.send(AppEvent::FeedFetched(idx, result));
                    });
                }
            }
        }
        KeyCode::Down => app.next(),
        KeyCode::Up => app.previous(),
        KeyCode::Tab => app.switch_tab_right(),
        KeyCode::BackTab => app.switch_tab_left(),
        KeyCode::Enter => app.select(),
        KeyCode::Char('e') => {
            app.editor_cursor = 0;
            app.editor_collapsed = app.sidebar_collapsed.clone();
            app.editor_mode = FeedEditorMode::Normal;
            app.state = AppState::FeedEditor;
        }
        KeyCode::Char('g') => {
            // Jump to top of feed list
            app.sidebar_cursor = 0;
            app.sidebar_title_start_tick = app.tick;

            let items = visible_tree_items(&app.categories, &app.feeds, &app.sidebar_collapsed);
            if let Some(FeedTreeItem::Feed { feeds_idx, .. }) = items.first() {
                app.selected_feed = *feeds_idx;
            }
        }
        KeyCode::Char('C') => {
            // Collapse all categories
            for category in &app.categories {
                app.sidebar_collapsed.insert(category.id);
            }

            app.sidebar_cursor = 0;
            app.sidebar_title_start_tick = app.tick;

            let items = visible_tree_items(&app.categories, &app.feeds, &app.sidebar_collapsed);
            if let Some(FeedTreeItem::Feed { feeds_idx, .. }) = items.first() {
                app.selected_feed = *feeds_idx;
            }
        }
        _ => {}
    }
    false
}

pub(super) fn handle_saved_feed_list(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Tab => app.switch_tab_right(),
        KeyCode::BackTab => app.switch_tab_left(),
        KeyCode::Char('j') | KeyCode::Down => app.next(),
        KeyCode::Char('k') | KeyCode::Up => app.previous(),
        KeyCode::Enter => app.select(),
        KeyCode::Char('e') => {
            app.saved_cat_editor_cursor = 0;
            app.state = AppState::SavedCategoryEditor;
        }
        KeyCode::Esc => app.unselect(),
        _ => {}
    }
    false
}
