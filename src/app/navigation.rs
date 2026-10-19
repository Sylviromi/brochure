//! Navigation: tab switching, cursor movement, drilling in/out of views, and the
//! article lists derived from the sidebar selection.

use super::{App, feeds_in_category, sidebar_tree_items, step_cursor};
use crate::models::{
    AddFeedStep, AppState, Article, CategoryId, FAVORITES_URL, FeedEditorMode, FeedTreeItem,
    SettingsItem, Tab,
};
use crate::storage::save_user_data;

impl App {
    // ── Tab switching ────────────────────────────────────────────────────────

    /// Switch to the previous tab (wraps from Feeds back to the last tab).
    pub fn switch_tab_left(&mut self) {
        self.selected_tab = self.selected_tab.prev();
        self.apply_tab_state();
    }

    /// Switch to the next tab (wraps from the last tab back to Feeds).
    pub fn switch_tab_right(&mut self) {
        self.selected_tab = self.selected_tab.next();
        self.apply_tab_state();
    }

    /// Synchronise `state` and saved-context flags to match `selected_tab` after a tab switch.
    fn apply_tab_state(&mut self) {
        self.clear_category_view();
        self.in_saved_context = false;
        self.state = match self.selected_tab {
            Tab::Feeds => AppState::FeedList,
            Tab::Saved => AppState::SavedCategoryList,
            Tab::Settings => AppState::SettingsList,
            Tab::Changelog => AppState::Changelog,
        };
        if self.selected_tab == Tab::Saved {
            self.sync_saved_preview();
        }
        if self.selected_tab == Tab::Feeds {
            self.sync_sidebar_preview();
        }
    }

    /// Jump straight to `tab` (mouse click on the tab bar).
    pub fn select_tab(&mut self, tab: Tab) {
        if self.selected_tab != tab {
            self.selected_tab = tab;
            self.apply_tab_state();
        }
    }

    /// Point the article preview at whatever the sidebar cursor rests on.
    pub(crate) fn sync_sidebar_preview(&mut self) {
        let items = self.sidebar_items();
        match items.get(self.sidebar_cursor) {
            Some(FeedTreeItem::AllFeeds) => self.populate_all_feeds_view(),
            Some(FeedTreeItem::Category { id, .. }) => self.populate_category_view(*id),
            Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                self.selected_feed = *feeds_idx;
                self.clear_category_view();
            }
            None => {}
        }
    }

    /// Rows of the main sidebar for the current collapse state.
    pub(crate) fn sidebar_items(&self) -> Vec<FeedTreeItem> {
        sidebar_tree_items(&self.categories, &self.feeds, &self.sidebar_collapsed)
    }

    /// Re-derive the category / all-feeds article list after feed contents changed,
    /// so stale `(feed_idx, article_idx)` pairs never survive to a render frame.
    pub(crate) fn refresh_article_views(&mut self) {
        if self.in_all_feeds_context {
            self.populate_all_feeds_view();
        } else if let Some(cat_id) = self.selected_sidebar_category {
            self.populate_category_view(cat_id);
        }
    }

    // ── Standard navigation ──────────────────────────────────────────────────

    /// Advance selection forward (wraps around).
    pub fn next(&mut self) {
        self.step(true);
    }

    /// Advance selection backward (wraps around).
    pub fn previous(&mut self) {
        self.step(false);
    }

    /// Move the focused list's cursor one step in the given direction.
    fn step(&mut self, forward: bool) {
        match self.state {
            AppState::FeedList => self.move_sidebar_cursor(forward),
            AppState::ArticleList => self.move_article_cursor(forward),
            AppState::SettingsList => {
                let at_edge = if forward {
                    SettingsItem::ALL.last()
                } else {
                    SettingsItem::ALL.first()
                } == Some(&self.settings_selected);
                if self.user_data.scroll_loop || !at_edge {
                    self.settings_selected = if forward {
                        self.settings_selected.next()
                    } else {
                        self.settings_selected.prev()
                    };
                }
            }
            AppState::SavedCategoryList => self.move_saved_cursor(forward),
            AppState::FeedEditor => self.editor_step(forward),
            _ => {}
        }
    }

    /// Move the sidebar (FeedList) cursor forward or backward one step.
    ///
    /// Wraps or clamps according to `scroll_loop`, then updates the article preview
    /// to match the new item.
    fn move_sidebar_cursor(&mut self, forward: bool) {
        let len = self.sidebar_items().len();
        if let Some(c) = step_cursor(
            self.sidebar_cursor,
            len,
            forward,
            self.user_data.scroll_loop,
        ) {
            self.set_sidebar_cursor(c);
        }
    }

    /// Place the sidebar cursor on row `idx` and update the article preview.
    pub(crate) fn set_sidebar_cursor(&mut self, idx: usize) {
        self.detached.sidebar = false;
        self.sidebar_cursor = idx;
        self.sidebar_title_start_tick = self.tick;
        self.selected_article = 0;
        self.sync_sidebar_preview();
    }

    /// Move the article list cursor forward or backward one step.
    ///
    /// Determines the article count from whichever context is active (category, saved, or feed),
    /// then advances or retreats `selected_article`. Always resets the title animation tick,
    /// even when already at a boundary (clamp behaviour when not looping).
    pub(crate) fn move_article_cursor(&mut self, forward: bool) {
        let len = self.article_count();
        if let Some(c) = step_cursor(
            self.selected_article,
            len,
            forward,
            self.user_data.scroll_loop,
        ) {
            self.set_article_cursor(c);
        }
    }

    /// Select article `idx` in the active list and restart its title animation.
    pub(crate) fn set_article_cursor(&mut self, idx: usize) {
        self.detached.article_list = false;
        self.selected_article = idx;
        self.article_title_start_tick = self.tick;
    }

    /// Scroll the open article by `delta` lines (negative = up), clamped to its content.
    pub fn scroll_article(&mut self, delta: i32) {
        let Some(link) = self.selected_article().map(|a| a.link.clone()) else {
            return;
        };
        let max = self
            .content_line_count
            .saturating_sub(self.content_area_height as usize)
            .min(u16::MAX as usize) as u16;
        self.article_scroll.scroll_by(&link, delta, max);
    }

    /// Lines moved by PageUp / PageDown in the article view.
    pub fn article_page(&self) -> i32 {
        i32::from(self.content_area_height.saturating_sub(1).max(1))
    }

    /// Number of articles in whichever list is active (category, saved, or feed).
    pub(crate) fn article_count(&self) -> usize {
        if self.in_category_context {
            self.category_view_articles.len()
        } else if self.in_saved_context {
            self.saved_view_articles.len()
        } else {
            self.feeds
                .get(self.selected_feed)
                .map_or(0, |f| f.articles.len())
        }
    }

    /// The article under the cursor in whichever list is active.
    pub fn selected_article(&self) -> Option<&Article> {
        if self.in_category_context {
            let &(fi, ai) = self.category_view_articles.get(self.selected_article)?;
            self.feeds.get(fi)?.articles.get(ai)
        } else if self.in_saved_context {
            self.saved_view_articles.get(self.selected_article)
        } else {
            self.feeds
                .get(self.selected_feed)?
                .articles
                .get(self.selected_article)
        }
    }

    /// Move the saved-categories sidebar cursor forward or backward one step.
    ///
    /// Advances or retreats `saved_sidebar_cursor` with wrap/clamp according to `scroll_loop`,
    /// resets the sidebar title animation tick, and refreshes the saved article preview.
    /// Always resets the tick and calls `sync_saved_preview`, even at a boundary when clamping.
    fn move_saved_cursor(&mut self, forward: bool) {
        let count = 1 + self.user_data.saved_categories.len();
        if let Some(c) = step_cursor(
            self.saved_sidebar_cursor,
            count,
            forward,
            self.user_data.scroll_loop,
        ) {
            self.set_saved_cursor(c);
        }
    }

    /// Place the saved-sidebar cursor on entry `idx` (0 = "All Saved") and refresh the preview.
    pub(crate) fn set_saved_cursor(&mut self, idx: usize) {
        self.detached.saved_sidebar = false;
        self.saved_sidebar_cursor = idx;
        self.sidebar_title_start_tick = self.tick;
        self.sync_saved_preview();
    }

    /// Descend into the next layer of the navigation hierarchy.
    pub fn select(&mut self) {
        match self.state {
            AppState::FeedList => {
                let items = self.sidebar_items();
                match items.get(self.sidebar_cursor) {
                    Some(FeedTreeItem::AllFeeds) => {
                        self.populate_all_feeds_view();
                        if !self.category_view_articles.is_empty() {
                            self.selected_article = 0;
                            self.state = AppState::ArticleList;
                        }
                    }
                    Some(FeedTreeItem::Category { id, .. }) => {
                        self.populate_category_view(*id);
                        if !self.category_view_articles.is_empty() {
                            self.in_category_context = true;
                            self.selected_article = 0;
                            self.state = AppState::ArticleList;
                        }
                    }
                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                        let feed = &self.feeds[*feeds_idx];
                        if feed.articles.is_empty()
                            && let Some(err) = &feed.fetch_error
                        {
                            let msg = format!("{}: {err}", feed.title);
                            self.selected_feed = *feeds_idx;
                            self.set_status(msg);
                            return;
                        }
                        self.selected_feed = *feeds_idx;
                        self.state = AppState::ArticleList;
                        self.selected_article = 0;
                    }
                    None => {}
                }
            }
            AppState::SavedCategoryList => {
                self.sync_saved_preview();
                if !self.saved_view_articles.is_empty() {
                    self.state = AppState::ArticleList;
                    self.selected_article = 0;
                }
            }
            AppState::ArticleList => {
                if let Some(article) = self.selected_article() {
                    self.content_line_count = article.content.lines().count().max(1);
                    self.state = AppState::ArticleDetail;
                }
            }
            _ => {}
        }
    }

    /// Populate `category_view_articles` for the given category id and mark it as selected.
    pub fn populate_category_view(&mut self, cat_id: CategoryId) {
        self.selected_sidebar_category = Some(cat_id);
        self.category_view_articles.clear();

        let feed_indices = feeds_in_category(cat_id, &self.categories, &self.feeds);
        self.set_category_view(feed_indices);
    }

    /// Clear the category context (cursor moved off a category onto a feed).
    pub fn clear_category_view(&mut self) {
        self.selected_sidebar_category = None;
        self.category_view_articles.clear();
        self.in_category_context = false;
        self.in_all_feeds_context = false;
    }

    /// Populate `category_view_articles` with every article across all non-favorites feeds,
    /// sorted newest-first. Sets `in_category_context` and `in_all_feeds_context`.
    pub fn populate_all_feeds_view(&mut self) {
        self.selected_sidebar_category = None;
        self.in_all_feeds_context = true;
        let feed_indices: Vec<usize> = (0..self.feeds.len())
            .filter(|&fi| self.feeds[fi].url != FAVORITES_URL)
            .collect();
        self.set_category_view(feed_indices);
        self.in_category_context = true;
    }

    /// Fill `category_view_articles` with every article of `feed_indices`, newest first
    /// (undated articles last), and clamp the article cursor to the new length.
    fn set_category_view(&mut self, feed_indices: Vec<usize>) {
        let feeds = &self.feeds;
        let mut pairs: Vec<(usize, usize)> = feed_indices
            .into_iter()
            .flat_map(|fi| (0..feeds[fi].articles.len()).map(move |ai| (fi, ai)))
            .collect();
        // `Reverse(Option)` puts `None` first, so sort on (has_date, date) descending.
        pairs.sort_by_key(|&(fi, ai)| {
            let ts = feeds[fi].articles[ai].published_secs;
            std::cmp::Reverse((ts.is_some(), ts))
        });
        self.category_view_articles = pairs;
        self.selected_article = self
            .selected_article
            .min(self.category_view_articles.len().saturating_sub(1));
    }

    /// Toggle the collapsed state of a category in the sidebar.
    pub fn toggle_category_collapse(&mut self, id: CategoryId) {
        if self.sidebar_collapsed.contains(&id) {
            self.sidebar_collapsed.remove(&id);
        } else {
            self.sidebar_collapsed.insert(id);
        }
        self.persist_sidebar_collapsed();
        // Refresh the preview if still hovering over this category.
        if self.selected_sidebar_category == Some(id) {
            self.populate_category_view(id);
        }
    }

    /// Sync `sidebar_collapsed` to `user_data` and persist to disk.
    pub(crate) fn persist_sidebar_collapsed(&mut self) {
        self.user_data.sidebar_collapsed = self.sidebar_collapsed.iter().copied().collect();
        let _ = save_user_data(&self.user_data);
    }

    /// Populate `saved_view_articles` from the currently selected sidebar entry.
    /// 0 = "All Saved", 1+ = category index.
    pub fn sync_saved_preview(&mut self) {
        // 0 = "All Saved"; a cursor past the last category shows nothing.
        let filter = match self.saved_sidebar_cursor {
            0 => Some(None),
            n => self
                .user_data
                .saved_categories
                .get(n - 1)
                .map(|c| Some(c.id)),
        };
        if let Some(category) = filter {
            self.selected_saved_category = category;
        }
        self.saved_view_articles = match filter {
            Some(category) => self
                .user_data
                .saved_articles
                .iter()
                .filter(|s| category.is_none_or(|id| s.category_id == id))
                .map(|s| Article {
                    is_saved: true,
                    ..s.article.clone()
                })
                .collect(),
            None => Vec::new(),
        };
        self.in_saved_context = !self.saved_view_articles.is_empty();
        self.selected_article = self
            .selected_article
            .min(self.saved_view_articles.len().saturating_sub(1));
    }

    /// Ascend back up the navigation hierarchy.
    pub fn unselect(&mut self) {
        match self.state {
            AppState::ArticleList => {
                if self.in_category_context {
                    // Return to FeedList; keep category_view_articles populated for preview.
                    // in_category_context is left intact so the article-list preview renders
                    // from category_view_articles rather than falling back to feeds[selected_feed].
                    // `move_sidebar_cursor` / `clear_category_view` will clear it when the user
                    // navigates to a different item in the sidebar.
                    self.state = AppState::FeedList;
                } else if self.in_saved_context {
                    self.in_saved_context = false;
                    self.saved_view_articles.clear();
                    self.state = AppState::SavedCategoryList;
                } else {
                    self.state = AppState::FeedList;
                }
            }
            AppState::ArticleDetail => self.state = AppState::ArticleList,
            AppState::AddFeed => {
                self.add_feed.url_input.clear();
                self.add_feed.step = AddFeedStep::Url;
                self.add_feed.url.clear();
                self.add_feed.fetched_title = None;
                self.add_feed.target_category = None;
                self.state = self.add_feed.return_state.clone();
            }
            AppState::SettingsList => {
                self.settings_selected = SettingsItem::ImportOpml;
                self.selected_tab = Tab::Feeds;
                self.state = AppState::FeedList;
            }
            AppState::OPMLExportPath | AppState::OPMLImportPath => {
                self.opml.path_input.clear();
                self.state = AppState::SettingsList;
            }
            AppState::ClearData => self.state = AppState::SettingsList,
            AppState::SavedCategoryList => {
                self.in_saved_context = false;
                self.selected_tab = Tab::Feeds;
                self.state = AppState::FeedList;
                self.sync_sidebar_preview();
            }
            AppState::FeedEditor => {
                self.feed_editor.mode = FeedEditorMode::Normal;
                self.selected_tab = Tab::Feeds;
                self.state = AppState::FeedList;
                // Feeds may have been deleted or moved: re-anchor the sidebar.
                let len = self.sidebar_items().len();
                self.sidebar_cursor = self.sidebar_cursor.min(len.saturating_sub(1));
                self.sync_sidebar_preview();
            }
            AppState::FeedEditorRename => {
                self.feed_editor.input.clear();
                self.feed_editor.mode = FeedEditorMode::Normal;
                self.state = AppState::FeedEditor;
            }
            _ => {}
        }
    }
}
