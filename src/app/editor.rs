//! Feed editor operations: cursor movement, deleting, and moving feeds/categories.

use super::{App, feed_row_indices, step_cursor, visible_cat_only_items, visible_tree_items};
use crate::models::{
    Category, CategoryId, EditorPanel, FAVORITES_URL, FeedEditorMode, FeedTreeItem,
};
use crate::storage::{save_categories, save_feeds};
use std::collections::HashSet;

impl App {
    /// Rows of the feed editor tree for the current collapse state.
    pub(crate) fn editor_items(&self) -> Vec<FeedTreeItem> {
        visible_tree_items(&self.categories, &self.feeds, &self.feed_editor.collapsed)
    }

    /// Move the active editor panel's cursor one step forward or backward.
    ///
    /// - Categories panel: steps through category rows; while a category is being moved
    ///   there is one extra slot past the end (drop at root level).
    /// - Feeds panel while moving a feed: every row is a drop target (a category header
    ///   means "insert as first child").
    /// - Feeds panel otherwise: only feed rows are selectable.
    pub(crate) fn editor_step(&mut self, forward: bool) {
        let wrap = self.user_data.scroll_loop;
        let items = self.editor_items();
        if self.feed_editor.panel == EditorPanel::Categories {
            let cats = items
                .iter()
                .filter(|item| matches!(item, FeedTreeItem::Category { .. }))
                .count();
            if cats == 0 {
                return;
            }
            let len = cats + usize::from(self.editor_moving_category(&items));
            if let Some(c) = step_cursor(self.feed_editor.cat_cursor, len, forward, wrap) {
                self.feed_editor.cat_cursor = c;
            }
        } else if matches!(self.feed_editor.mode, FeedEditorMode::Moving { .. }) {
            if let Some(c) = step_cursor(self.feed_editor.cursor, items.len(), forward, wrap) {
                self.feed_editor.cursor = c;
            }
        } else {
            let rows = feed_row_indices(&items);
            let cur = rows
                .iter()
                .position(|&i| i == self.feed_editor.cursor)
                .unwrap_or(0);
            if let Some(n) = step_cursor(cur, rows.len(), forward, wrap) {
                self.feed_editor.cursor = rows[n];
            }
        }
    }

    /// Nesting depth of the category under the categories-panel cursor (0 = root,
    /// also for the "drop at root" slot past the end).
    pub(crate) fn editor_cat_cursor_depth(&self) -> u8 {
        match visible_cat_only_items(&self.categories, &self.feeds, &self.feed_editor.collapsed)
            .get(self.feed_editor.cat_cursor)
        {
            Some(FeedTreeItem::Category { depth, .. }) => *depth,
            _ => 0,
        }
    }

    /// True when currently moving a category — used to skip feeds during navigation.
    pub(crate) fn editor_moving_category(&self, items: &[FeedTreeItem]) -> bool {
        match &self.feed_editor.mode {
            FeedEditorMode::Moving {
                origin_render_idx, ..
            } => matches!(
                items.get(*origin_render_idx),
                Some(FeedTreeItem::Category { .. })
            ),
            _ => false,
        }
    }

    /// Clamp `editor_cursor` to the nearest Feed item in the full tree.
    pub(crate) fn clamp_editor_cursor_to_feed(&mut self) {
        let feed_indices = feed_row_indices(&self.editor_items());
        if feed_indices.is_empty() {
            self.feed_editor.cursor = 0;
            return;
        }
        self.feed_editor.cursor = feed_indices
            .iter()
            .find(|&&i| i >= self.feed_editor.cursor)
            .or_else(|| feed_indices.last())
            .copied()
            .unwrap_or(0);
    }

    /// Delete the feed at the current cursor position in the feeds panel.
    pub(crate) fn delete_feed_at_editor_cursor(&mut self) {
        if let Some(FeedTreeItem::Feed { feeds_idx, .. }) =
            self.editor_items().get(self.feed_editor.cursor)
        {
            self.feeds.remove(*feeds_idx);
            let _ = save_feeds(&self.feeds);
        }
    }

    /// Count total feeds (including in subcategories) that belong to a category.
    pub(crate) fn count_feeds_recursive(&self, cat_id: CategoryId) -> usize {
        let direct = self
            .feeds
            .iter()
            .filter(|f| f.category_id == Some(cat_id))
            .count();
        let sub: usize = self
            .categories
            .iter()
            .filter(|c| c.parent_id == Some(cat_id))
            .map(|c| self.count_feeds_recursive(c.id))
            .sum();
        direct + sub
    }

    /// Delete a category and all its descendants (subcategories + feeds). Persists changes.
    pub(crate) fn delete_category_recursive(&mut self, cat_id: CategoryId) {
        let mut doomed: HashSet<CategoryId> = HashSet::from([cat_id]);
        let mut frontier = vec![cat_id];
        while let Some(id) = frontier.pop() {
            for child in self.categories.iter().filter(|c| c.parent_id == Some(id)) {
                if doomed.insert(child.id) {
                    frontier.push(child.id);
                }
            }
        }
        self.feeds
            .retain(|f| f.category_id.is_none_or(|id| !doomed.contains(&id)));
        self.categories.retain(|c| !doomed.contains(&c.id));
        let _ = save_categories(&self.categories);
        let _ = save_feeds(&self.feeds);
    }

    /// The category being moved and the category list as it would look if dropped
    /// at the current cursor/depth — the single source for both the on-screen preview
    /// and the actual drop. The list is `None` when the drop would change nothing.
    pub(crate) fn editor_category_move_preview(
        &self,
    ) -> Option<(CategoryId, Option<Vec<Category>>)> {
        let FeedEditorMode::Moving {
            origin_render_idx,
            depth_delta,
            ..
        } = self.feed_editor.mode
        else {
            return None;
        };
        let Some(FeedTreeItem::Category { id: src_id, .. }) =
            self.editor_items().get(origin_render_idx).cloned()
        else {
            return None;
        };
        let cats =
            visible_cat_only_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);
        // One slot past the last row means "last at the root level".
        let cursor_id = match cats.get(self.feed_editor.cat_cursor) {
            Some(FeedTreeItem::Category { id, .. }) => Some(*id),
            _ => None,
        };
        let moved = category_move_result(&self.categories, src_id, cursor_id, depth_delta);
        Some((src_id, moved))
    }

    /// Rows shown in the categories panel, plus the id of the category being moved.
    /// While moving, the rows are the tree as it will look after the drop (with the
    /// target's ancestors expanded), so the moving category appears at its target.
    pub(crate) fn editor_category_rows(&self) -> (Vec<FeedTreeItem>, Option<CategoryId>) {
        let Some((moving_id, moved)) = self.editor_category_move_preview() else {
            let rows =
                visible_cat_only_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);
            return (rows, None);
        };
        let categories = moved.as_deref().unwrap_or(&self.categories);
        let mut collapsed = self.feed_editor.collapsed.clone();
        let mut parent = parent_of(categories, moving_id);
        while let Some(pid) = parent {
            collapsed.remove(&pid);
            parent = parent_of(categories, pid);
        }
        let rows = visible_cat_only_items(categories, &self.feeds, &collapsed);
        (rows, Some(moving_id))
    }

    /// Apply the pending category move and return the moved category's new row.
    pub(crate) fn apply_category_move(&mut self) -> Option<usize> {
        let (src_id, moved) = self.editor_category_move_preview()?;
        self.categories = moved?;
        let _ = save_categories(&self.categories);
        self.editor_cat_position(src_id)
    }

    /// Row of category `cat_id` in the editor's categories panel.
    fn editor_cat_position(&self, cat_id: CategoryId) -> Option<usize> {
        visible_cat_only_items(&self.categories, &self.feeds, &self.feed_editor.collapsed)
            .iter()
            .position(|item| matches!(item, FeedTreeItem::Category { id, .. } if *id == cat_id))
    }

    /// Apply the pending feed move and return the new render-index of the moved feed.
    pub(crate) fn apply_feed_move(&mut self, origin: usize) -> Option<usize> {
        let dest = self.feed_editor.cursor;

        let items = self.editor_items();
        let src_item = items.get(origin)?;
        let dest_item = items.get(dest);

        if origin == dest {
            return None;
        }

        let FeedTreeItem::Feed { feeds_idx, .. } = src_item.clone() else {
            return None;
        };

        let new_parent_cat = match dest_item {
            Some(FeedTreeItem::Category { id, .. }) => Some(*id),
            Some(FeedTreeItem::Feed { feeds_idx: di, .. }) => {
                self.feeds.get(*di).and_then(|f| f.category_id)
            }
            Some(FeedTreeItem::AllFeeds) | None => None,
        };
        let dest_feed_vidx = match dest_item {
            Some(FeedTreeItem::Feed { feeds_idx: di, .. }) => Some(*di),
            _ => None,
        };

        if let Some(feed) = self.feeds.get_mut(feeds_idx) {
            feed.category_id = new_parent_cat;
        }

        self.place_feed_in_parent(feeds_idx, dest_feed_vidx, new_parent_cat);

        let _ = save_feeds(&self.feeds);

        self.editor_items().iter().position(
            |item| matches!(item, FeedTreeItem::Feed { feeds_idx: fi, .. } if *fi == feeds_idx),
        )
    }

    /// Insert `moved_idx` into `parent`'s feed list right after `dest_vidx`, then reassign orders.
    fn place_feed_in_parent(
        &mut self,
        moved_idx: usize,
        dest_vidx: Option<usize>,
        parent: Option<CategoryId>,
    ) {
        let mut siblings: Vec<usize> = self
            .feeds
            .iter()
            .enumerate()
            .filter(|(i, f)| f.url != FAVORITES_URL && f.category_id == parent && *i != moved_idx)
            .map(|(i, _)| i)
            .collect();
        siblings.sort_by_key(|&i| self.feeds[i].order);

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
            self.feeds[idx].order = order;
        }
    }
}

fn parent_of(categories: &[Category], id: CategoryId) -> Option<CategoryId> {
    categories
        .iter()
        .find(|c| c.id == id)
        .and_then(|c| c.parent_id)
}

/// Returns true if `ancestor_id` is an ancestor of (or equal to) `node_id`.
fn is_ancestor_of(categories: &[Category], ancestor_id: CategoryId, node_id: CategoryId) -> bool {
    let mut node = Some(node_id);
    while let Some(id) = node {
        if id == ancestor_id {
            return true;
        }
        node = parent_of(categories, id);
    }
    false
}

/// Where category `src_id` lands when dropped on the row of `cursor_id`:
/// `(new parent, sibling to insert after)`, or `None` if the drop is invalid.
///
/// `depth_delta` is relative to the cursor row: `+1` nests inside it, `0` places it
/// after it as a sibling, and each step below zero climbs one ancestor (so a large
/// enough negative value reaches the root level).
fn category_move_target(
    categories: &[Category],
    src_id: CategoryId,
    cursor_id: CategoryId,
    depth_delta: i8,
) -> Option<(Option<CategoryId>, Option<CategoryId>)> {
    let (parent, after) = if depth_delta >= 1 {
        (Some(cursor_id), None)
    } else {
        let mut anchor = cursor_id;
        for _ in 0..depth_delta.unsigned_abs() {
            match parent_of(categories, anchor) {
                Some(pid) => anchor = pid,
                None => break,
            }
        }
        if anchor == src_id {
            // Dropped back on its own row at its own depth: nothing to move.
            return None;
        }
        (parent_of(categories, anchor), Some(anchor))
    };
    if parent.is_some_and(|pid| is_ancestor_of(categories, src_id, pid)) {
        return None;
    }
    Some((parent, after))
}

/// `categories` with `src_id` dropped on the row of `cursor_id` (`None` = past the
/// last row, i.e. last at the root level), or `None` if the drop is invalid or a no-op.
fn category_move_result(
    categories: &[Category],
    src_id: CategoryId,
    cursor_id: Option<CategoryId>,
    depth_delta: i8,
) -> Option<Vec<Category>> {
    let (parent, after) = match cursor_id {
        Some(cursor_id) => category_move_target(categories, src_id, cursor_id, depth_delta)?,
        None => {
            let last_root = categories
                .iter()
                .filter(|c| c.parent_id.is_none() && c.id != src_id)
                .max_by_key(|c| c.order)
                .map(|c| c.id);
            (None, last_root)
        }
    };
    let mut moved = categories.to_vec();
    moved.iter_mut().find(|c| c.id == src_id)?.parent_id = parent;
    place_category_after_sibling(&mut moved, src_id, after, parent);
    (moved != categories).then_some(moved)
}

/// Insert `moved_id` into `parent`'s sibling list right after `after_id` (first when
/// `None`), then renumber the siblings' orders.
fn place_category_after_sibling(
    categories: &mut [Category],
    moved_id: CategoryId,
    after_id: Option<CategoryId>,
    parent: Option<CategoryId>,
) {
    let mut siblings: Vec<usize> = (0..categories.len())
        .filter(|&i| categories[i].parent_id == parent && categories[i].id != moved_id)
        .collect();
    siblings.sort_by_key(|&i| categories[i].order);
    let Some(moved_pos) = categories.iter().position(|c| c.id == moved_id) else {
        return;
    };
    let insert_at = match after_id {
        Some(aid) => siblings
            .iter()
            .position(|&i| categories[i].id == aid)
            .map_or(siblings.len(), |p| p + 1),
        None => 0,
    };
    siblings.insert(insert_at, moved_pos);
    for (order, &idx) in siblings.iter().enumerate() {
        categories[idx].order = order;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(id: CategoryId, parent_id: Option<CategoryId>, order: usize) -> Category {
        Category {
            id,
            name: format!("c{id}"),
            parent_id,
            order,
        }
    }

    /// A(1) > B(2) > C(3), plus root D(4).
    fn tree() -> Vec<Category> {
        vec![
            cat(1, None, 0),
            cat(2, Some(1), 0),
            cat(3, Some(2), 0),
            cat(4, None, 1),
        ]
    }

    #[test]
    fn child_moved_left_on_itself_lands_at_root_after_parent() {
        assert_eq!(
            category_move_target(&tree(), 2, 2, -1),
            Some((None, Some(1)))
        );
    }

    #[test]
    fn grandchild_climbs_one_level_per_step() {
        let cats = tree();
        assert_eq!(
            category_move_target(&cats, 3, 3, -1),
            Some((Some(1), Some(2)))
        );
        assert_eq!(category_move_target(&cats, 3, 3, -2), Some((None, Some(1))));
    }

    #[test]
    fn cannot_nest_inside_own_subtree() {
        assert_eq!(category_move_target(&tree(), 1, 3, 1), None);
        assert_eq!(category_move_target(&tree(), 1, 2, 0), None);
    }

    fn layout(cats: &[Category]) -> Vec<(CategoryId, Option<CategoryId>, usize)> {
        let mut v: Vec<_> = cats.iter().map(|c| (c.id, c.parent_id, c.order)).collect();
        v.sort();
        v
    }

    #[test]
    fn past_the_end_appends_at_root() {
        let moved = category_move_result(&tree(), 3, None, 0).unwrap();
        // C becomes the last root category, after D.
        assert_eq!(
            layout(&moved),
            vec![(1, None, 0), (2, Some(1), 0), (3, None, 2), (4, None, 1)]
        );
    }

    #[test]
    fn no_op_moves_produce_no_result() {
        // Already last at root.
        assert_eq!(category_move_result(&tree(), 4, None, 0), None);
        assert_eq!(category_move_result(&tree(), 2, Some(2), 0), None);
    }

    #[test]
    fn dropping_on_itself_is_a_no_op() {
        assert_eq!(category_move_target(&tree(), 2, 2, 0), None);
    }

    #[test]
    fn dropping_on_root_row_keeps_root_level() {
        assert_eq!(
            category_move_target(&tree(), 3, 4, 0),
            Some((None, Some(4)))
        );
    }
}
