use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{App, visible_cat_only_items, visible_tree_items},
    models::{
        AddFeedStep, AppEvent, AppState, Category, CategoryId, EditorPanel, FAVORITES_URL, Feed,
        FeedEditorMode, FeedTreeItem,
    },
    storage::{save_categories, save_feeds},
};

pub(super) fn handle_feed_editor(app: &mut App, key: KeyEvent, _tx: &UnboundedSender<AppEvent>) {
    match app.state {
        AppState::FeedEditorRename => match key.code {
            KeyCode::Enter => {
                let name = app.editor_input.trim().to_string();
                if !name.is_empty() {
                    match &app.editor_mode {
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
                                &app.editor_collapsed,
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
                                None => {}
                            }
                        }
                        _ => {}
                    }
                }
                app.editor_input.clear();
                app.editor_mode = FeedEditorMode::Normal;
                app.state = AppState::FeedEditor;
            }
            KeyCode::Esc => app.unselect(),
            KeyCode::Char(c) => app.editor_input.push(c),
            KeyCode::Backspace => {
                app.editor_input.pop();
            }
            _ => {}
        },
        AppState::FeedEditor => {
            // ── Pending category-delete confirmation (right panel) ─────────────
            if let Some((cat_id, _)) = app.editor_delete_cat {
                match key.code {
                    KeyCode::Enter => {
                        delete_category_recursive(app, cat_id);
                        app.editor_delete_cat = None;
                        let new_len = visible_cat_only_items(
                            &app.categories,
                            &app.feeds,
                            &app.editor_collapsed,
                        )
                        .len();
                        if app.editor_cat_cursor >= new_len && new_len > 0 {
                            app.editor_cat_cursor = new_len - 1;
                        }
                        // Also clamp the feeds panel cursor since deleted feeds shift the tree.
                        clamp_editor_cursor_to_feed(app);
                    }
                    KeyCode::Esc => {
                        app.editor_delete_cat = None;
                    }
                    _ => {}
                }
                return;
            }

            match &app.editor_mode.clone() {
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
                        let items =
                            visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
                        matches!(items.get(origin), Some(FeedTreeItem::Category { .. }))
                    };
                    match key.code {
                        KeyCode::Char('j') | KeyCode::Down => app.next(),
                        KeyCode::Char('k') | KeyCode::Up => app.previous(),
                        KeyCode::Left if is_cat_move => {
                            // Go shallower (sibling of cursor's parent).
                            app.editor_mode = FeedEditorMode::Moving {
                                origin_render_idx: origin,
                                original_cursor: orig,
                                depth_delta: (depth_delta - 1).max(-1),
                            };
                        }
                        KeyCode::Right if is_cat_move => {
                            // Go deeper (child of cursor).
                            app.editor_mode = FeedEditorMode::Moving {
                                origin_render_idx: origin,
                                original_cursor: orig,
                                depth_delta: (depth_delta + 1).min(1),
                            };
                        }
                        KeyCode::Char(' ') => {
                            if is_cat_move {
                                let new_pos = apply_category_move(
                                    app,
                                    origin,
                                    app.editor_cat_cursor,
                                    depth_delta,
                                );
                                if let Some(pos) = new_pos {
                                    app.editor_cat_cursor = pos;
                                }
                                app.editor_mode = FeedEditorMode::Normal;
                                // Stays on Categories panel
                            } else {
                                let new_cursor = apply_move(app, origin);
                                if let Some(pos) = new_cursor {
                                    app.editor_cursor = pos;
                                }
                                app.editor_mode = FeedEditorMode::Normal;
                            }
                        }
                        KeyCode::Esc => {
                            if is_cat_move {
                                app.editor_cat_cursor = orig; // restore cat cursor
                            } else {
                                app.editor_cursor = orig;
                            }
                            app.editor_mode = FeedEditorMode::Normal;
                        }
                        _ => {}
                    }
                }
                FeedEditorMode::Normal => {
                    // Tab always switches panel focus.
                    if key.code == KeyCode::Tab {
                        app.editor_panel = match app.editor_panel {
                            EditorPanel::Categories => {
                                clamp_editor_cursor_to_feed(app);
                                EditorPanel::Feeds
                            }
                            EditorPanel::Feeds => EditorPanel::Categories,
                        };
                        return;
                    }

                    match app.editor_panel {
                        // ── Right panel: categories only ──────────────────────
                        EditorPanel::Categories => match key.code {
                            KeyCode::Char('j') | KeyCode::Down => app.next(),
                            KeyCode::Char('k') | KeyCode::Up => app.previous(),
                            KeyCode::Enter => {
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.editor_cat_cursor)
                                {
                                    let id = *id;
                                    if app.editor_collapsed.contains(&id) {
                                        app.editor_collapsed.remove(&id);
                                    } else {
                                        app.editor_collapsed.insert(id);
                                    }
                                }
                            }
                            KeyCode::Char('n') => {
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                let parent_id = if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.editor_cat_cursor)
                                {
                                    Some(*id)
                                } else {
                                    None
                                };
                                app.editor_input.clear();
                                app.editor_mode = FeedEditorMode::NewCategory { parent_id };
                                app.state = AppState::FeedEditorRename;
                            }
                            KeyCode::Char('r') => {
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.editor_cat_cursor)
                                {
                                    let cat_id = *id;
                                    let full = visible_tree_items(
                                        &app.categories,
                                        &app.feeds,
                                        &app.editor_collapsed,
                                    );
                                    let full_idx = full
                                        .iter()
                                        .position(|item| {
                                            matches!(item, FeedTreeItem::Category { id, .. } if *id == cat_id)
                                        })
                                        .unwrap_or(0);
                                    app.editor_input = app
                                        .categories
                                        .iter()
                                        .find(|c| c.id == cat_id)
                                        .map(|c| c.name.clone())
                                        .unwrap_or_default();
                                    app.editor_mode = FeedEditorMode::Renaming {
                                        render_idx: full_idx,
                                    };
                                    app.state = AppState::FeedEditorRename;
                                }
                            }
                            KeyCode::Char('d') => {
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.editor_cat_cursor)
                                {
                                    let cat_id = *id;
                                    let feed_count = count_feeds_recursive(app, cat_id);
                                    app.editor_delete_cat = Some((cat_id, feed_count));
                                }
                            }
                            KeyCode::Char(' ') => {
                                // Start moving the selected category — stay on Categories panel.
                                let cats = visible_cat_only_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                if let Some(FeedTreeItem::Category { id, .. }) =
                                    cats.get(app.editor_cat_cursor)
                                {
                                    let cat_id = *id;
                                    let full = visible_tree_items(
                                        &app.categories,
                                        &app.feeds,
                                        &app.editor_collapsed,
                                    );
                                    if let Some(full_idx) = full.iter().position(|item| {
                                        matches!(item, FeedTreeItem::Category { id, .. } if *id == cat_id)
                                    }) {
                                        app.editor_mode = FeedEditorMode::Moving {
                                            origin_render_idx: full_idx,
                                            original_cursor: app.editor_cat_cursor,
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
                                    &app.editor_collapsed,
                                );
                                if matches!(
                                    items.get(app.editor_cursor),
                                    Some(FeedTreeItem::Feed { .. })
                                ) {
                                    let origin = app.editor_cursor;
                                    app.editor_mode = FeedEditorMode::Moving {
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
                                    &app.editor_collapsed,
                                );
                                let cursor_item = items.get(app.editor_cursor);
                                // Cursor is always on a Feed item in the Feeds panel
                                app.add_feed_target_category = match cursor_item {
                                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                                        app.feeds.get(*feeds_idx).and_then(|f| f.category_id)
                                    }
                                    _ => None,
                                };
                                app.add_feed_target_order = match cursor_item {
                                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                                        app.feeds.get(*feeds_idx).map(|f| f.order + 1)
                                    }
                                    _ => None,
                                };
                                app.input.clear();
                                app.add_feed_step = AddFeedStep::Url;
                                app.add_feed_url.clear();
                                app.add_feed_fetched_title = None;
                                app.add_feed_return_state = AppState::FeedEditor;
                                app.state = AppState::AddFeed;
                            }
                            KeyCode::Char('r') => {
                                // Cursor is always on a Feed item — rename the feed.
                                let items = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                if let Some(FeedTreeItem::Feed { feeds_idx, .. }) =
                                    items.get(app.editor_cursor)
                                {
                                    let current_name = app
                                        .feeds
                                        .get(*feeds_idx)
                                        .map(|f| f.title.clone())
                                        .unwrap_or_default();
                                    app.editor_input = current_name;
                                    app.editor_mode = FeedEditorMode::Renaming {
                                        render_idx: app.editor_cursor,
                                    };
                                    app.state = AppState::FeedEditorRename;
                                }
                            }
                            KeyCode::Char('d') => {
                                delete_at_cursor(app);
                                clamp_editor_cursor_to_feed(app);
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

/// Clamp `editor_cursor` to the nearest Feed item in the full tree.
fn clamp_editor_cursor_to_feed(app: &mut App) {
    let items = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
    let feed_indices: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| matches!(item, FeedTreeItem::Feed { .. }))
        .map(|(i, _)| i)
        .collect();
    if feed_indices.is_empty() {
        app.editor_cursor = 0;
        return;
    }
    app.editor_cursor = feed_indices
        .iter()
        .find(|&&i| i >= app.editor_cursor)
        .or_else(|| feed_indices.last())
        .copied()
        .unwrap_or(0);
}

fn delete_at_cursor(app: &mut App) {
    let items = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
    // Feeds panel cursor is always on a Feed item (navigation skips categories).
    if let Some(FeedTreeItem::Feed { feeds_idx, .. }) = items.get(app.editor_cursor) {
        let idx = *feeds_idx;
        if idx > 0 {
            app.feeds.remove(idx);
            let _ = save_feeds(&app.feeds);
        }
    }
}

/// Count total feeds (including in subcategories) that belong to a category.
fn count_feeds_recursive(app: &App, cat_id: CategoryId) -> usize {
    let direct = app
        .feeds
        .iter()
        .filter(|f| f.category_id == Some(cat_id))
        .count();
    let sub: usize = app
        .categories
        .iter()
        .filter(|c| c.parent_id == Some(cat_id))
        .map(|c| count_feeds_recursive(app, c.id))
        .sum();
    direct + sub
}

/// Delete a category and all its descendants (subcategories + feeds). Persists changes.
fn delete_category_recursive(app: &mut App, cat_id: CategoryId) {
    // Recursively delete children first.
    let children: Vec<CategoryId> = app
        .categories
        .iter()
        .filter(|c| c.parent_id == Some(cat_id))
        .map(|c| c.id)
        .collect();
    for child in children {
        delete_category_recursive(app, child);
    }
    // Remove feeds that belong to this category.
    app.feeds.retain(|f| f.category_id != Some(cat_id));
    // Remove the category itself.
    app.categories.retain(|c| c.id != cat_id);
    let _ = save_categories(&app.categories);
    let _ = save_feeds(&app.feeds);
}

/// Returns true if `ancestor_id` is an ancestor of (or equal to) `node_id`.
fn is_ancestor_of(categories: &[Category], ancestor_id: CategoryId, node_id: CategoryId) -> bool {
    if node_id == ancestor_id {
        return true;
    }
    let parent = categories
        .iter()
        .find(|c| c.id == node_id)
        .and_then(|c| c.parent_id);
    match parent {
        Some(pid) => is_ancestor_of(categories, ancestor_id, pid),
        None => false,
    }
}

/// Apply a category move: move `origin_full_idx` category to be a child of the category
/// at `dest_cat_cursor` in the cats-only list (or virtual root if out of bounds).
/// Returns the new `editor_cat_cursor` position for the moved category.
/// Apply a category move.
///
/// `depth_delta` controls where the item lands relative to cursor's depth:
/// -  0 = sibling after cursor (cursor's parent becomes the new parent)
/// - +1 = child of cursor (old "drop on" behaviour)
/// - -1 = sibling after cursor's parent (one level up)
fn apply_category_move(
    app: &mut App,
    origin_full_idx: usize,
    dest_cat_cursor: usize,
    depth_delta: i8,
) -> Option<usize> {
    let items = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
    let src_id = match items.get(origin_full_idx) {
        Some(FeedTreeItem::Category { id, .. }) => *id,
        _ => return None,
    };

    let cats = visible_cat_only_items(&app.categories, &app.feeds, &app.editor_collapsed);

    // Virtual root: move to root level, append.
    if dest_cat_cursor >= cats.len() {
        if let Some(cat) = app.categories.iter_mut().find(|c| c.id == src_id) {
            cat.parent_id = None;
        }
        place_category_after_sibling(&mut app.categories, src_id, None, None);
        let _ = save_categories(&app.categories);
        let new_cats = visible_cat_only_items(&app.categories, &app.feeds, &app.editor_collapsed);
        return new_cats
            .iter()
            .position(|item| matches!(item, FeedTreeItem::Category { id, .. } if *id == src_id));
    }

    let cursor_cat_id = match cats.get(dest_cat_cursor) {
        Some(FeedTreeItem::Category { id, .. }) => *id,
        _ => return None,
    };

    // Determine new parent and "insert-after" anchor based on depth_delta.
    let (new_parent_id, after_anchor_id) = if depth_delta >= 1 {
        // Child of cursor.
        if cursor_cat_id == src_id {
            return None;
        }
        (Some(cursor_cat_id), None) // insert as first child
    } else {
        // Walk depth_delta levels up from cursor to find the anchor and its parent.
        let mut anchor_id = cursor_cat_id;
        for _ in 0..depth_delta.unsigned_abs() {
            let p = app
                .categories
                .iter()
                .find(|c| c.id == anchor_id)
                .and_then(|c| c.parent_id);
            match p {
                Some(pid) => anchor_id = pid,
                None => break,
            }
        }
        let anchor_parent = app
            .categories
            .iter()
            .find(|c| c.id == anchor_id)
            .and_then(|c| c.parent_id);
        let after = if anchor_id == src_id {
            None
        } else {
            Some(anchor_id)
        };
        (anchor_parent, after)
    };

    // Prevent cycle.
    if let Some(pid) = new_parent_id
        && is_ancestor_of(&app.categories, src_id, pid)
    {
        return None;
    }
    // Prevent drop on self (sibling case).
    if after_anchor_id == Some(src_id) {
        return None;
    }

    if let Some(cat) = app.categories.iter_mut().find(|c| c.id == src_id) {
        cat.parent_id = new_parent_id;
    }

    place_category_after_sibling(&mut app.categories, src_id, after_anchor_id, new_parent_id);
    let _ = save_categories(&app.categories);

    let new_cats = visible_cat_only_items(&app.categories, &app.feeds, &app.editor_collapsed);
    new_cats
        .iter()
        .position(|item| matches!(item, FeedTreeItem::Category { id, .. } if *id == src_id))
}

/// Apply the pending feed move and return the new render-index of the moved feed (for cursor update).
fn apply_move(app: &mut App, origin: usize) -> Option<usize> {
    let dest = app.editor_cursor;

    let items = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
    let src_item = items.get(origin)?;
    let dest_item = items.get(dest);

    if origin == dest {
        return None;
    }

    // Feeds panel cursor is always on a Feed item; only feed moves use this function.
    let FeedTreeItem::Feed { feeds_idx, .. } = src_item.clone() else {
        return None;
    };

    let new_parent_cat = match dest_item {
        Some(FeedTreeItem::Category { id, .. }) => Some(*id),
        Some(FeedTreeItem::Feed { feeds_idx: di, .. }) => {
            app.feeds.get(*di).and_then(|f| f.category_id)
        }
        None => None,
    };
    let dest_feed_vidx = match dest_item {
        Some(FeedTreeItem::Feed { feeds_idx: di, .. }) => Some(*di),
        _ => None,
    };

    if let Some(feed) = app.feeds.get_mut(feeds_idx) {
        feed.category_id = new_parent_cat;
    }

    place_feed_in_parent(&mut app.feeds, feeds_idx, dest_feed_vidx, new_parent_cat);

    let _ = save_feeds(&app.feeds);

    let new_items = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
    new_items.iter().position(
        |item| matches!(item, FeedTreeItem::Feed { feeds_idx: fi, .. } if *fi == feeds_idx),
    )
}

/// Insert `moved_idx` (feed Vec index) into `parent`'s feed list right after
/// `dest_vidx` (or at the start if None), then reassign sequential orders.
fn place_feed_in_parent(
    feeds: &mut [Feed],
    moved_idx: usize,
    dest_vidx: Option<usize>,
    parent: Option<CategoryId>,
) {
    let mut siblings: Vec<usize> = feeds
        .iter()
        .enumerate()
        .filter(|(i, f)| f.url != FAVORITES_URL && f.category_id == parent && *i != moved_idx)
        .map(|(i, _)| i)
        .collect();
    siblings.sort_by_key(|&i| feeds[i].order);

    let insert_pos = match dest_vidx {
        Some(di) => siblings
            .iter()
            .position(|&i| i == di)
            .map(|p| p + 1)
            .unwrap_or(siblings.len()),
        None => 0,
    };
    siblings.insert(insert_pos, moved_idx);

    for (order, &idx) in siblings.iter().enumerate() {
        feeds[idx].order = order;
    }
}

/// Insert `moved_id` into `parent`'s sibling list right after `after_id`
/// (or prepend if `after_id` is None), then reassign sequential orders.
fn place_category_after_sibling(
    categories: &mut [Category],
    moved_id: CategoryId,
    after_id: Option<CategoryId>,
    parent: Option<CategoryId>,
) {
    let mut siblings: Vec<usize> = categories
        .iter()
        .enumerate()
        .filter(|(_, c)| c.parent_id == parent && c.id != moved_id)
        .map(|(i, _)| i)
        .collect();
    siblings.sort_by_key(|&i| categories[i].order);

    let moved_pos = categories.iter().position(|c| c.id == moved_id).unwrap();
    let insert_at = match after_id {
        Some(aid) => siblings
            .iter()
            .position(|&i| categories[i].id == aid)
            .map(|p| p + 1)
            .unwrap_or(siblings.len()),
        None => 0,
    };
    siblings.insert(insert_at, moved_pos);

    for (order, &idx) in siblings.iter().enumerate() {
        categories[idx].order = order;
    }
}
