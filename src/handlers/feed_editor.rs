use crossterm::event::{KeyCode, KeyEvent};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{visible_cat_only_items, visible_tree_items, App},
    models::{
        AddFeedStep, AppEvent, AppState, Category, CategoryId, EditorPanel, Feed, FeedEditorMode,
        FeedTreeItem, FAVORITES_URL,
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
            // ── Pending category-delete confirmation (left panel) ──────────────
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
                    }
                    KeyCode::Esc => {
                        app.editor_delete_cat = None;
                        app.set_status("");
                    }
                    _ => {}
                }
                return;
            }

            match &app.editor_mode.clone() {
                // ── Right panel: Moving mode ───────────────────────────────────
                FeedEditorMode::Moving { origin_render_idx, original_cursor } => {
                    let origin = *origin_render_idx;
                    let orig = *original_cursor;
                    match key.code {
                        KeyCode::Char('j') | KeyCode::Down => app.next(),
                        KeyCode::Char('k') | KeyCode::Up => app.previous(),
                        KeyCode::Enter => {
                            apply_move(app, origin);
                            app.editor_mode = FeedEditorMode::Normal;
                        }
                        KeyCode::Esc => {
                            app.editor_cursor = orig;
                            app.editor_mode = FeedEditorMode::Normal;
                        }
                        _ => {}
                    }
                }
                FeedEditorMode::Normal => {
                    // Tab always switches panel focus.
                    if key.code == KeyCode::Tab {
                        app.editor_panel = match app.editor_panel {
                            EditorPanel::Categories => EditorPanel::Feeds,
                            EditorPanel::Feeds => EditorPanel::Categories,
                        };
                        return;
                    }

                    match app.editor_panel {
                        // ── Left panel: category manager ──────────────────────
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
                                let parent_id =
                                    if let Some(FeedTreeItem::Category { id, .. }) =
                                        cats.get(app.editor_cat_cursor)
                                    {
                                        Some(*id)
                                    } else {
                                        None
                                    };
                                // Point right-panel cursor to the end of this category for rendering.
                                let full = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                app.editor_cursor = full.len(); // past end = append
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
                                    app.editor_cursor = full_idx;
                                    app.editor_input = app
                                        .categories
                                        .iter()
                                        .find(|c| c.id == cat_id)
                                        .map(|c| c.name.clone())
                                        .unwrap_or_default();
                                    app.editor_mode =
                                        FeedEditorMode::Renaming { render_idx: full_idx };
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
                                    let cat_name = app
                                        .categories
                                        .iter()
                                        .find(|c| c.id == cat_id)
                                        .map(|c| c.name.clone())
                                        .unwrap_or_default();
                                    app.editor_delete_cat = Some((cat_id, feed_count));
                                    app.set_status(format!(
                                        "Delete '{cat_name}' with {feed_count} feed(s)? [Enter] confirm  [Esc] cancel"
                                    ));
                                }
                            }
                            KeyCode::Esc | KeyCode::Char('q') => app.unselect(),
                            _ => {}
                        },
                        // ── Right panel: feed + category tree ─────────────────
                        EditorPanel::Feeds => match key.code {
                            KeyCode::Char('j') | KeyCode::Down => app.next(),
                            KeyCode::Char('k') | KeyCode::Up => app.previous(),
                            KeyCode::Enter => {
                                let items = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                if let Some(FeedTreeItem::Category { id, .. }) =
                                    items.get(app.editor_cursor)
                                {
                                    let id = *id;
                                    if app.editor_collapsed.contains(&id) {
                                        app.editor_collapsed.remove(&id);
                                    } else {
                                        app.editor_collapsed.insert(id);
                                    }
                                }
                            }
                            KeyCode::Char(' ') => {
                                let origin = app.editor_cursor;
                                app.editor_mode = FeedEditorMode::Moving {
                                    origin_render_idx: origin,
                                    original_cursor: origin,
                                };
                            }
                            KeyCode::Char('a') => {
                                let items = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                let cursor_item = items.get(app.editor_cursor);
                                app.add_feed_target_category = match cursor_item {
                                    Some(FeedTreeItem::Category { id, .. }) => Some(*id),
                                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                                        app.feeds.get(*feeds_idx).and_then(|f| f.category_id)
                                    }
                                    None => None,
                                };
                                app.add_feed_target_order = match cursor_item {
                                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                                        app.feeds.get(*feeds_idx).map(|f| f.order + 1)
                                    }
                                    Some(FeedTreeItem::Category { id, .. }) => {
                                        let cat_id = *id;
                                        let min = app
                                            .feeds
                                            .iter()
                                            .filter(|f| f.category_id == Some(cat_id))
                                            .map(|f| f.order)
                                            .min();
                                        Some(min.unwrap_or(0))
                                    }
                                    None => None,
                                };
                                app.input.clear();
                                app.add_feed_step = AddFeedStep::Url;
                                app.add_feed_url.clear();
                                app.add_feed_fetched_title = None;
                                app.add_feed_return_state = AppState::FeedEditor;
                                app.state = AppState::AddFeed;
                            }
                            KeyCode::Char('n') => {
                                let items = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                let parent_id = match items.get(app.editor_cursor) {
                                    Some(FeedTreeItem::Category { id, .. }) => Some(*id),
                                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                                        app.feeds.get(*feeds_idx).and_then(|f| f.category_id)
                                    }
                                    None => None,
                                };
                                app.editor_input.clear();
                                app.editor_mode = FeedEditorMode::NewCategory { parent_id };
                                app.state = AppState::FeedEditorRename;
                            }
                            KeyCode::Char('r') => {
                                let items = visible_tree_items(
                                    &app.categories,
                                    &app.feeds,
                                    &app.editor_collapsed,
                                );
                                let current_name = match items.get(app.editor_cursor) {
                                    Some(FeedTreeItem::Category { id, .. }) => app
                                        .categories
                                        .iter()
                                        .find(|c| c.id == *id)
                                        .map(|c| c.name.clone())
                                        .unwrap_or_default(),
                                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => app
                                        .feeds
                                        .get(*feeds_idx)
                                        .map(|f| f.title.clone())
                                        .unwrap_or_default(),
                                    None => String::new(),
                                };
                                app.editor_input = current_name;
                                app.editor_mode = FeedEditorMode::Renaming {
                                    render_idx: app.editor_cursor,
                                };
                                app.state = AppState::FeedEditorRename;
                            }
                            KeyCode::Char('d') => delete_at_cursor(app),
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

fn delete_at_cursor(app: &mut App) {
    let items = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
    match items.get(app.editor_cursor) {
        Some(FeedTreeItem::Category { id, .. }) => {
            let id = *id;
            orphan_category_feeds(app, id);
            remove_category_recursive(app, id);
            let _ = save_categories(&app.categories);
            let _ = save_feeds(&app.feeds);
            let new_len =
                visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed).len();
            if app.editor_cursor >= new_len && new_len > 0 {
                app.editor_cursor = new_len - 1;
            }
        }
        Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
            let idx = *feeds_idx;
            if idx > 0 {
                app.feeds.remove(idx);
                let _ = save_feeds(&app.feeds);
                let new_len =
                    visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed).len();
                if app.editor_cursor >= new_len && new_len > 0 {
                    app.editor_cursor = new_len - 1;
                }
            }
        }
        None => {}
    }
}

/// Count total feeds (including in subcategories) that belong to a category.
fn count_feeds_recursive(app: &App, cat_id: CategoryId) -> usize {
    let direct = app.feeds.iter().filter(|f| f.category_id == Some(cat_id)).count();
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

fn orphan_category_feeds(app: &mut App, cat_id: CategoryId) {
    let children: Vec<CategoryId> = app
        .categories
        .iter()
        .filter(|c| c.parent_id == Some(cat_id))
        .map(|c| c.id)
        .collect();
    for child_id in children {
        orphan_category_feeds(app, child_id);
    }
    for feed in app.feeds.iter_mut() {
        if feed.category_id == Some(cat_id) {
            feed.category_id = None;
        }
    }
}

fn remove_category_recursive(app: &mut App, cat_id: CategoryId) {
    let children: Vec<CategoryId> = app
        .categories
        .iter()
        .filter(|c| c.parent_id == Some(cat_id))
        .map(|c| c.id)
        .collect();
    for child_id in children {
        remove_category_recursive(app, child_id);
    }
    app.categories.retain(|c| c.id != cat_id);
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

fn apply_move(app: &mut App, origin: usize) {
    let dest = app.editor_cursor;

    let items = visible_tree_items(&app.categories, &app.feeds, &app.editor_collapsed);
    let Some(src_item) = items.get(origin) else {
        return;
    };
    // dest == items.len() is the virtual root position (categories only).
    let dest_item = items.get(dest);

    if origin == dest && dest < items.len() {
        return;
    }

    match src_item.clone() {
        FeedTreeItem::Feed { feeds_idx, .. } => {
            let new_parent_cat = match dest_item {
                Some(FeedTreeItem::Category { id, .. }) => Some(*id),
                Some(FeedTreeItem::Feed { feeds_idx: di, .. }) => {
                    app.feeds.get(*di).and_then(|f| f.category_id)
                }
                None => None,
            };
            // Vec index of the dest feed (if landing on a feed).
            let dest_feed_vidx = match dest_item {
                Some(FeedTreeItem::Feed { feeds_idx: di, .. }) => Some(*di),
                _ => None,
            };

            // Move feed to new parent.
            if let Some(feed) = app.feeds.get_mut(feeds_idx) {
                feed.category_id = new_parent_cat;
            }

            // Rebuild sibling order: collect all feeds in new parent (excl. moved),
            // find dest's position, insert moved after it, then reassign sequential orders.
            place_feed_in_parent(&mut app.feeds, feeds_idx, dest_feed_vidx, new_parent_cat);

            let _ = save_feeds(&app.feeds);
        }
        FeedTreeItem::Category { id: src_id, .. } => {
            // Onto a category → first child of that category.
            // Virtual root (dest_item == None) → first root-level category.
            let new_parent_id = match dest_item {
                Some(FeedTreeItem::Category { id, .. }) => Some(*id),
                _ => None,
            };

            // Prevent cycle: src cannot become a descendant of itself.
            if let Some(pid) = new_parent_id
                && is_ancestor_of(&app.categories, src_id, pid)
            {
                return;
            }

            // Set new parent.
            if let Some(cat) = app.categories.iter_mut().find(|c| c.id == src_id) {
                cat.parent_id = new_parent_id;
            }

            // Rebuild sibling order: insert moved category as first child (position 0),
            // then reassign sequential orders.
            place_category_first_in_parent(&mut app.categories, src_id, new_parent_id);

            let _ = save_categories(&app.categories);
        }
    }
}

/// Insert `moved_idx` (feed Vec index) into `parent`'s feed list right after
/// `dest_vidx` (or at the start if None), then reassign orders 0, 1, 2, …
fn place_feed_in_parent(
    feeds: &mut [Feed],
    moved_idx: usize,
    dest_vidx: Option<usize>,
    parent: Option<CategoryId>,
) {
    // All siblings except the moved feed, sorted by current order.
    let mut siblings: Vec<usize> = feeds
        .iter()
        .enumerate()
        .filter(|(i, f)| f.url != FAVORITES_URL && f.category_id == parent && *i != moved_idx)
        .map(|(i, _)| i)
        .collect();
    siblings.sort_by_key(|&i| feeds[i].order);

    // Insert after dest (or at position 0 when landing on a category header).
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

/// Insert `moved_id` (category id) as the first child of `parent`, then
/// reassign orders 0, 1, 2, … among all siblings.
fn place_category_first_in_parent(
    categories: &mut [Category],
    moved_id: CategoryId,
    parent: Option<CategoryId>,
) {
    // All siblings except the moved category, sorted by current order.
    let mut siblings: Vec<usize> = categories
        .iter()
        .enumerate()
        .filter(|(_, c)| c.parent_id == parent && c.id != moved_id)
        .map(|(i, _)| i)
        .collect();
    siblings.sort_by_key(|&i| categories[i].order);

    // Moved category goes first.
    let moved_pos = categories.iter().position(|c| c.id == moved_id).unwrap();
    siblings.insert(0, moved_pos);

    for (order, &idx) in siblings.iter().enumerate() {
        categories[idx].order = order;
    }
}
