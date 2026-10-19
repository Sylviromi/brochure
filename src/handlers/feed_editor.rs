//! Key handlers for the feed editor state: rename, move, and delete feeds and categories.
//!
//! Manages two-panel editor (feeds and categories), move operations, and category deletion.

use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{App, visible_cat_only_items, visible_tree_items},
    models::{
        AddFeedStep, AppEvent, AppState, Category, EditorPanel, FeedEditorMode, FeedTreeItem,
    },
    storage::{save_categories, save_feeds},
};

/// Handle key input in the feed editor: rename, move, and delete feeds or categories.
pub(super) fn handle_feed_editor(app: &mut App, key: KeyEvent, _tx: &UnboundedSender<AppEvent>) {
    match app.state {
        AppState::FeedEditorRename => match key.code {
            KeyCode::Enter => {
                let name = app.feed_editor.input.text.trim().to_string();
                if !name.is_empty() {
                    match &app.feed_editor.mode {
                        FeedEditorMode::NewCategory { parent_id } => {
                            let parent_id = *parent_id;
                            let next_id =
                                app.categories.iter().map(|c| c.id).max().unwrap_or(0) + 1;
                            let next_order = app
                                .categories
                                .iter()
                                .filter(|c| c.parent_id == parent_id)
                                .map(|c| c.order)
                                .max()
                                .unwrap_or(0)
                                + 1;
                            app.categories.push(Category {
                                id: next_id,
                                name,
                                parent_id,
                                order: next_order,
                            });
                            let _ = save_categories(&app.categories);
                        }
                        FeedEditorMode::Renaming { render_idx } => {
                            let items = visible_tree_items(
                                &app.categories,
                                &app.feeds,
                                &app.feed_editor.collapsed,
                            );
                            match items.get(*render_idx) {
                                Some(FeedTreeItem::Category { id, .. }) => {
                                    if let Some(cat) =
                                        app.categories.iter_mut().find(|c| c.id == *id)
                                    {
                                        cat.name = name;
                                    }
                                    let _ = save_categories(&app.categories);
                                }
                                Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                                    if let Some(feed) = app.feeds.get_mut(*feeds_idx) {
                                        feed.title = name;
                                    }
                                    let _ = save_feeds(&app.feeds);
                                }
                                Some(FeedTreeItem::AllFeeds) | None => {}
                            }
                        }
                        FeedEditorMode::EditingUrl { render_idx } => {
                            let items = visible_tree_items(
                                &app.categories,
                                &app.feeds,
                                &app.feed_editor.collapsed,
                            );
                            if let Some(FeedTreeItem::Feed { feeds_idx, .. }) =
                                items.get(*render_idx)
                            {
                                if let Some(feed) = app.feeds.get_mut(*feeds_idx) {
                                    feed.url = name;
                                }
                                let _ = save_feeds(&app.feeds);
                            }
                        }
                        _ => {}
                    }
                }
                app.feed_editor.input.clear();
                app.feed_editor.mode = FeedEditorMode::Normal;
                app.state = AppState::FeedEditor;
            }
            KeyCode::Esc => app.unselect(),
            _ => app.feed_editor.input.handle_key(key.code, None),
        },
        AppState::FeedEditor => {
            // ── Pending category-delete confirmation (right panel) ─────────────
            if let Some((cat_id, _)) = app.feed_editor.delete_cat {
                match key.code {
                    KeyCode::Enter => {
                        app.delete_category_recursive(cat_id);
                        app.feed_editor.delete_cat = None;
                        let new_len = visible_cat_only_items(
                            &app.categories,
                            &app.feeds,
                            &app.feed_editor.collapsed,
                        )
                        .len();
                        if app.feed_editor.cat_cursor >= new_len && new_len > 0 {
                            app.feed_editor.cat_cursor = new_len - 1;
                        }
                        // Also clamp the feeds panel cursor since deleted feeds shift the tree.
                        app.clamp_editor_cursor_to_feed();
                    }
                    KeyCode::Esc => {
                        app.feed_editor.delete_cat = None;
                    }
                    _ => {}
                }
                return;
            }

            match &app.feed_editor.mode.clone() {
                // ── Moving mode ───────────────────────────────────────────────
                FeedEditorMode::Moving {
                    origin_render_idx,
                    original_cursor,
                    depth_delta,
                } => {
                    let origin = *origin_render_idx;
                    let orig = *original_cursor;
                    let depth_delta = *depth_delta;
                    let is_cat_move = {
                        let items = visible_tree_items(
                            &app.categories,
                            &app.feeds,
                            &app.feed_editor.collapsed,
                        );
                        matches!(items.get(origin), Some(FeedTreeItem::Category { .. }))
                    };
                    match key.code {
                        KeyCode::Down | KeyCode::Up => {
                            if key.code == KeyCode::Down {
                                app.next();
                            } else {
                                app.previous();
                            }
                            if is_cat_move {
                                // Keep the depth offset reachable from the new cursor row,
                                // so the next ←/→ press always changes something.
                                let min = -(app.editor_cat_cursor_depth() as i8);
                                app.feed_editor.mode = FeedEditorMode::Moving {
                                    origin_render_idx: origin,
                                    original_cursor: orig,
                                    depth_delta: depth_delta.max(min),
                                };
                            }
                        }
                        KeyCode::Left if is_cat_move => {
                            // Go shallower, one ancestor per press, down to the root level.
                            let min = -(app.editor_cat_cursor_depth() as i8);
                            app.feed_editor.mode = FeedEditorMode::Moving {
                                origin_render_idx: origin,
                                original_cursor: orig,
                                depth_delta: (depth_delta - 1).max(min),
                            };
                        }
                        KeyCode::Right if is_cat_move => {
                            // Go deeper (child of cursor).
                            app.feed_editor.mode = FeedEditorMode::Moving {
                                origin_render_idx: origin,
                                original_cursor: orig,
                                depth_delta: (depth_delta + 1).min(1),
                            };
                        }
                        KeyCode::Char(' ') => {
                            if is_cat_move {
                                // A no-op drop returns the cursor to the category.
                                app.feed_editor.cat_cursor =
                                    app.apply_category_move().unwrap_or(orig);
                                app.feed_editor.mode = FeedEditorMode::Normal;
                                // Stays on Categories panel
                            } else {
                                let new_cursor = app.apply_feed_move(origin);
                                if let Some(pos) = new_cursor {
                                    app.feed_editor.cursor = pos;
                                }
                                app.feed_editor.mode = FeedEditorMode::Normal;
                            }
                        }
                        KeyCode::Esc => {
                            if is_cat_move {
                                app.feed_editor.cat_cursor = orig; // restore cat cursor
                            } else {
                                app.feed_editor.cursor = orig;
                            }
                            app.feed_editor.mode = FeedEditorMode::Normal;
                        }
                        _ => {}
                    }
                }
                FeedEditorMode::Normal => {
                    // Tab always switches panel focus.
                    if key.code == KeyCode::Tab {
                        app.feed_editor.panel = match app.feed_editor.panel {
                            EditorPanel::Categories => {
                                app.clamp_editor_cursor_to_feed();
                                EditorPanel::Feeds
                            }
                            EditorPanel::Feeds => EditorPanel::Categories,
                        };
                        return;
                    }

                    match app.feed_editor.panel {
                        // ── Right panel: categories only ──────────────────────
                        EditorPanel::Categories => match key.code {
                            KeyCode::Char('j') | KeyCode::Down => app.next(),
                            KeyCode::Char('k') | KeyCode::Up => app.previous(),
                            KeyCode::Enter => {
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.feed_editor.collapsed,
                                );
                                if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.feed_editor.cat_cursor)
                                {
                                    let id = *id;
                                    if app.feed_editor.collapsed.contains(&id) {
                                        app.feed_editor.collapsed.remove(&id);
                                    } else {
                                        app.feed_editor.collapsed.insert(id);
                                    }
                                }
                            }
                            KeyCode::Char('n') => {
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.feed_editor.collapsed,
                                );
                                let parent_id = if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.feed_editor.cat_cursor)
                                {
                                    Some(*id)
                                } else {
                                    None
                                };
                                app.feed_editor.input.clear();
                                app.feed_editor.mode = FeedEditorMode::NewCategory { parent_id };
                                app.state = AppState::FeedEditorRename;
                            }
                            KeyCode::Char('r') => {
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.feed_editor.collapsed,
                                );
                                if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.feed_editor.cat_cursor)
                                {
                                    let cat_id = *id;
                                    let full = visible_tree_items(
                                        &app.categories,
                                        &app.feeds,
                                        &app.feed_editor.collapsed,
                                    );
                                    let full_idx = full
                                        .iter()
                                        .position(|item| {
                                            matches!(item, FeedTreeItem::Category { id, .. } if *id == cat_id)
                                        })
                                        .unwrap_or(0);
                                    app.feed_editor.input.text = app
                                        .categories
                                        .iter()
                                        .find(|c| c.id == cat_id)
                                        .map(|c| c.name.clone())
                                        .unwrap_or_default();
                                    app.feed_editor.input.cursor =
                                        app.feed_editor.input.text.chars().count();
                                    app.feed_editor.mode = FeedEditorMode::Renaming {
                                        render_idx: full_idx,
                                    };
                                    app.state = AppState::FeedEditorRename;
                                }
                            }
                            KeyCode::Char('d') => {
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.feed_editor.collapsed,
                                );
                                if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.feed_editor.cat_cursor)
                                {
                                    let cat_id = *id;
                                    let feed_count = app.count_feeds_recursive(cat_id);
                                    app.feed_editor.delete_cat = Some((cat_id, feed_count));
                                }
                            }
                            KeyCode::Char(' ') => {
                                // Start moving the selected category — stay on Categories panel.
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.feed_editor.collapsed,
                                );
                                if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.feed_editor.cat_cursor)
                                {
                                    let cat_id = *id;
                                    let full = visible_tree_items(
                                        &app.categories,
                                        &app.feeds,
                                        &app.feed_editor.collapsed,
                                    );
                                    if let Some(full_idx) = full.iter().position(|item| {
                                        matches!(item, FeedTreeItem::Category { id, .. } if *id == cat_id)
                                    }) {
                                        app.feed_editor.mode = FeedEditorMode::Moving {
                                            origin_render_idx: full_idx,
                                            original_cursor: app.feed_editor.cat_cursor,
                                            depth_delta: 0,
                                        };
                                        // DON'T change panel — stays on Categories
                                    }
                                }
                            }
                            KeyCode::Esc | KeyCode::Char('q') => app.unselect(),
                            _ => {}
                        },
                        // ── Left panel: feeds only ────────────────────────────
                        EditorPanel::Feeds => match key.code {
                            KeyCode::Char('j') | KeyCode::Down => app.next(),
                            KeyCode::Char('k') | KeyCode::Up => app.previous(),
                            KeyCode::Char(' ') => {
                                // Only start moving Feed items from the Feeds panel.
                                let items = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.feed_editor.collapsed,
                                );
                                if matches!(
                                    items.get(app.feed_editor.cursor),
                                    Some(FeedTreeItem::Feed { .. })
                                ) {
                                    let origin = app.feed_editor.cursor;
                                    app.feed_editor.mode = FeedEditorMode::Moving {
                                        origin_render_idx: origin,
                                        original_cursor: origin,
                                        depth_delta: 0,
                                    };
                                }
                            }
                            KeyCode::Char('a') => {
                                let items = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.feed_editor.collapsed,
                                );
                                let cursor_item = items.get(app.feed_editor.cursor);
                                // Cursor is always on a Feed item in the Feeds panel
                                app.add_feed.target_category = match cursor_item {
                                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                                        app.feeds.get(*feeds_idx).and_then(|f| f.category_id)
                                    }
                                    _ => None,
                                };
                                app.add_feed.target_order = match cursor_item {
                                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                                        app.feeds.get(*feeds_idx).map(|f| f.order + 1)
                                    }
                                    _ => None,
                                };
                                app.add_feed.url_input.clear();
                                app.add_feed.step = AddFeedStep::Url;
                                app.add_feed.url.clear();
                                app.add_feed.fetched_title = None;
                                app.add_feed.return_state = AppState::FeedEditor;
                                app.state = AppState::AddFeed;
                            }
                            KeyCode::Char('r') => {
                                // Cursor is always on a Feed item — rename the feed.
                                let items = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.feed_editor.collapsed,
                                );
                                if let Some(FeedTreeItem::Feed { feeds_idx, .. }) =
                                    items.get(app.feed_editor.cursor)
                                {
                                    let current_name = app
                                        .feeds
                                        .get(*feeds_idx)
                                        .map(|f| f.title.clone())
                                        .unwrap_or_default();
                                    app.feed_editor.input.text = current_name;
                                    app.feed_editor.input.cursor =
                                        app.feed_editor.input.text.chars().count();
                                    app.feed_editor.mode = FeedEditorMode::Renaming {
                                        render_idx: app.feed_editor.cursor,
                                    };
                                    app.state = AppState::FeedEditorRename;
                                }
                            }
                            KeyCode::Char('u') => {
                                let items = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.feed_editor.collapsed,
                                );
                                if let Some(FeedTreeItem::Feed { feeds_idx, .. }) =
                                    items.get(app.feed_editor.cursor)
                                {
                                    let current_url = app
                                        .feeds
                                        .get(*feeds_idx)
                                        .map(|f| f.url.clone())
                                        .unwrap_or_default();
                                    app.feed_editor.input.text = current_url;
                                    app.feed_editor.input.cursor =
                                        app.feed_editor.input.text.chars().count();
                                    app.feed_editor.mode = FeedEditorMode::EditingUrl {
                                        render_idx: app.feed_editor.cursor,
                                    };
                                    app.state = AppState::FeedEditorRename;
                                }
                            }
                            KeyCode::Char('d') => {
                                app.delete_feed_at_editor_cursor();
                                app.clamp_editor_cursor_to_feed();
                            }
                            KeyCode::Esc | KeyCode::Char('q') => app.unselect(),
                            _ => {}
                        },
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[tokio::test]
    async fn left_climbs_to_root_from_any_depth() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new();
        let cat = |id, parent_id| Category {
            id,
            name: format!("Cat {id}"),
            parent_id,
            order: 0,
        };
        // Cat 1 > Cat 2 > Cat 3
        app.categories = vec![cat(1, None), cat(2, Some(1)), cat(3, Some(2))];
        app.feed_editor.collapsed.clear();
        app.state = AppState::FeedEditor;
        app.feed_editor.panel = EditorPanel::Categories;
        app.feed_editor.cat_cursor = 2;
        let mut press = |code| {
            handle_feed_editor(&mut app, KeyEvent::new(code, KeyModifiers::NONE), &tx);
            match app.feed_editor.mode {
                FeedEditorMode::Moving { depth_delta, .. } => depth_delta,
                _ => panic!("not moving"),
            }
        };
        assert_eq!(press(KeyCode::Char(' ')), 0);
        assert_eq!(press(KeyCode::Left), -1);
        assert_eq!(press(KeyCode::Left), -2);
        assert_eq!(press(KeyCode::Left), -2, "stops at the root level");
        assert_eq!(press(KeyCode::Right), -1);
    }

    #[tokio::test]
    async fn move_preview_shows_the_category_where_it_will_land() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new();
        app.feeds.clear();
        let cat = |id, parent_id, order| Category {
            id,
            name: format!("Cat {id}"),
            parent_id,
            order,
        };
        // 1 > (2 > 3, 4), 5
        app.categories = vec![
            cat(1, None, 0),
            cat(2, Some(1), 0),
            cat(3, Some(2), 0),
            cat(4, Some(1), 1),
            cat(5, None, 1),
        ];
        app.feed_editor.collapsed.clear();
        app.state = AppState::FeedEditor;
        app.feed_editor.panel = EditorPanel::Categories;
        app.feed_editor.cat_cursor = 2;
        let rows = |app: &App| {
            let (rows, moving) = app.editor_category_rows();
            let rows: Vec<(u64, u8)> = rows
                .iter()
                .filter_map(|item| match item {
                    FeedTreeItem::Category { id, depth, .. } => Some((*id, *depth)),
                    _ => None,
                })
                .collect();
            (rows, moving)
        };
        for code in [KeyCode::Char(' '), KeyCode::Left, KeyCode::Left] {
            handle_feed_editor(&mut app, KeyEvent::new(code, KeyModifiers::NONE), &tx);
        }
        // Cat 3 previews at the root level, right after Cat 1's subtree.
        assert_eq!(
            rows(&app),
            (vec![(1, 0), (2, 1), (4, 1), (3, 0), (5, 0)], Some(3))
        );
        assert_eq!(app.categories[2].parent_id, Some(2), "nothing saved yet");
    }
}
