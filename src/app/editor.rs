//! Feed editor operations: cursor movement, deleting, and moving feeds/categories.

use super::{App, feed_row_indices, step_cursor, visible_cat_only_items, visible_tree_items};
use crate::models::{CategoryId, EditorPanel, FAVORITES_URL, FeedEditorMode, FeedTreeItem};
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

    /// Returns true if `ancestor_id` is an ancestor of (or equal to) `node_id`.
    fn is_ancestor_of(&self, ancestor_id: CategoryId, node_id: CategoryId) -> bool {
        if node_id == ancestor_id {
            return true;
        }
        let parent = self
            .categories
            .iter()
            .find(|c| c.id == node_id)
            .and_then(|c| c.parent_id);
        match parent {
            Some(pid) => self.is_ancestor_of(ancestor_id, pid),
            None => false,
        }
    }

    /// Apply a category move and return the new `editor_cat_cursor` for the moved category.
    pub(crate) fn apply_category_move(
        &mut self,
        origin_full_idx: usize,
        dest_cat_cursor: usize,
        depth_delta: i8,
    ) -> Option<usize> {
        let src_id = match self.editor_items().get(origin_full_idx) {
            Some(FeedTreeItem::Category { id, .. }) => *id,
            _ => return None,
        };

        let cats =
            visible_cat_only_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);

        if dest_cat_cursor >= cats.len() {
            if let Some(cat) = self.categories.iter_mut().find(|c| c.id == src_id) {
                cat.parent_id = None;
            }
            self.place_category_after_sibling(src_id, None, None);
            let _ = save_categories(&self.categories);
            return self.editor_cat_position(src_id);
        }

        let cursor_cat_id = match cats.get(dest_cat_cursor) {
            Some(FeedTreeItem::Category { id, .. }) => *id,
            _ => return None,
        };

        let (new_parent_id, after_anchor_id) = if depth_delta >= 1 {
            if cursor_cat_id == src_id {
                return None;
            }
            (Some(cursor_cat_id), None)
        } else {
            let mut anchor_id = cursor_cat_id;
            for _ in 0..depth_delta.unsigned_abs() {
                let p = self
                    .categories
                    .iter()
                    .find(|c| c.id == anchor_id)
                    .and_then(|c| c.parent_id);
                match p {
                    Some(pid) => anchor_id = pid,
                    None => break,
                }
            }
            let anchor_parent = self
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

        if let Some(pid) = new_parent_id
            && self.is_ancestor_of(src_id, pid)
        {
            return None;
        }
        if after_anchor_id == Some(src_id) {
            return None;
        }

        if let Some(cat) = self.categories.iter_mut().find(|c| c.id == src_id) {
            cat.parent_id = new_parent_id;
        }

        self.place_category_after_sibling(src_id, after_anchor_id, new_parent_id);
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

    /// Insert `moved_id` into `parent`'s sibling list right after `after_id`, then reassign orders.
    fn place_category_after_sibling(
        &mut self,
        moved_id: CategoryId,
        after_id: Option<CategoryId>,
        parent: Option<CategoryId>,
    ) {
        let mut siblings: Vec<usize> = self
            .categories
            .iter()
            .enumerate()
            .filter(|(_, c)| c.parent_id == parent && c.id != moved_id)
            .map(|(i, _)| i)
            .collect();
        siblings.sort_by_key(|&i| self.categories[i].order);

        let Some(moved_pos) = self.categories.iter().position(|c| c.id == moved_id) else {
            return;
        };
        let insert_at = match after_id {
            Some(aid) => siblings
                .iter()
                .position(|&i| self.categories[i].id == aid)
                .map(|p| p + 1)
                .unwrap_or(siblings.len()),
            None => 0,
        };
        siblings.insert(insert_at, moved_pos);

        for (order, &idx) in siblings.iter().enumerate() {
            self.categories[idx].order = order;
        }
    }
}
