use crate::models::{
    AddFeedStep, AppState, Article, Category, CategoryId, EditorPanel, Feed, FeedEditorMode,
    FeedTreeItem, ListScroll, SettingsItem, Tab, UserData, FAVORITES_URL,
};
use crate::storage::{article_cache_size, load_categories, load_feeds, load_user_data};
use ratatui::widgets::ListState;
use std::collections::HashSet;

/// Central application state passed to every frame draw and event handler.
pub struct App {
    pub state: AppState,
    pub feeds: Vec<Feed>,
    pub selected_feed: usize,
    pub selected_article: usize,
    pub status_msg: String,
    pub input: String,
    pub user_data: UserData,
    pub settings_selected: SettingsItem,
    pub scroll_offset: u16,
    /// Cached line count of the currently-viewed article for scroll capping.
    pub content_line_count: usize,
    /// Height (in terminal rows) of the article content viewport — set each draw frame.
    pub content_area_height: u16,
    /// Incremented on every Tick event; used to animate spinners in the UI.
    pub tick: usize,

    // ── Persistent list scroll states ────────────────────────────────────────
    /// Scroll state for the feed/category sidebar (FeedList tab).
    pub sidebar_list_state: ListState,
    /// Scroll state for the saved-categories sidebar (Saved tab).
    pub saved_sidebar_list_state: ListState,
    /// Scroll state for the article list panel.
    pub article_list_state: ListState,
    /// Scroll state for the editor feeds panel.
    pub editor_feeds_list_state: ListState,
    /// Scroll state for the editor categories panel.
    pub editor_cats_list_state: ListState,

    // ── Tab bar ──────────────────────────────────────────────────────────────
    pub selected_tab: Tab,

    // ── Saved context ────────────────────────────────────────────────────────
    /// Cursor into the flat saved-categories sidebar list (0 = "All Saved", 1+ = category).
    pub saved_sidebar_cursor: usize,
    /// Articles shown in ArticleList/ArticleDetail when browsing the Saved tab.
    pub saved_view_articles: Vec<Article>,
    /// True while ArticleList / ArticleDetail is showing saved articles.
    pub in_saved_context: bool,
    /// None = "All Saved" is selected; Some(id) = a specific category is selected.
    pub selected_saved_category: Option<u32>,

    // ── Category picker ──────────────────────────────────────────────────────
    /// Cursor row in the CategoryPicker modal (0..categories + 2).
    pub category_picker_cursor: usize,
    /// True when the "New category..." entry is active and user is typing.
    pub category_picker_new_mode: bool,
    /// Text buffer for the new-category name input inside the picker.
    pub category_picker_input: String,
    /// State to return to when the picker closes (ArticleList or ArticleDetail).
    pub category_picker_return_state: AppState,

    // ── Saved category editor ────────────────────────────────────────────────
    /// Scroll state for the SavedCategoryEditor list.
    pub saved_cat_editor_scroll: ListScroll,

    // ── Multi-step AddFeed ───────────────────────────────────────────────────
    pub add_feed_step: AddFeedStep,
    /// URL captured in step 0, used when saving in step 1.
    pub add_feed_url: String,
    /// Title fetched from the feed URL in the background (placeholder for step 1).
    pub add_feed_fetched_title: Option<String>,
    /// Where to return after AddFeed completes (SettingsList or FeedEditor).
    pub add_feed_return_state: AppState,
    /// Category to place the new feed in (set from cursor when adding via FeedEditor).
    pub add_feed_target_category: Option<CategoryId>,
    /// Order value to insert the new feed at (set from cursor when adding via FeedEditor).
    /// None means append at end.
    pub add_feed_target_order: Option<usize>,

    // ── OPML path input ──────────────────────────────────────────────────────
    /// File path typed by the user in OPMLExportPath / OPMLImportPath states.
    pub opml_path_input: String,

    // ── Fetch progress ───────────────────────────────────────────────────────
    /// Total feeds being fetched in the current batch.
    pub feeds_total: usize,
    /// Feeds still pending a result.
    pub feeds_pending: usize,
    /// True while a readability fetch for the selected article is in progress.
    pub article_fetching: bool,

    // ── Category tree ────────────────────────────────────────────────────────
    /// All category nodes (persisted to categories.json).
    pub categories: Vec<Category>,
    /// Categories collapsed in the sidebar view.
    pub sidebar_collapsed: HashSet<CategoryId>,
    /// Cursor into the flattened visible-tree list for the sidebar/FeedList.
    pub sidebar_cursor: usize,

    // ── Category context ─────────────────────────────────────────────────────
    /// Set to Some(id) when sidebar cursor rests on a category header; None when on a feed.
    pub selected_sidebar_category: Option<CategoryId>,
    /// (feeds_idx, article_idx) pairs for all articles in the selected category, sorted by date desc.
    pub category_view_articles: Vec<(usize, usize)>,
    /// True while ArticleList/ArticleDetail is showing the category-context article list.
    pub in_category_context: bool,

    // ── Cache ────────────────────────────────────────────────────────────────
    /// Byte size of articles.json; refreshed at startup and after cache clear.
    pub article_cache_size: u64,

    // ── Feed editor ──────────────────────────────────────────────────────────
    /// Cursor into the flattened visible-tree list inside the feed editor.
    pub editor_cursor: usize,
    /// Categories collapsed in the feed editor view (starts as a copy of sidebar_collapsed).
    pub editor_collapsed: HashSet<CategoryId>,
    /// Current interaction mode in the feed editor.
    pub editor_mode: FeedEditorMode,
    /// Text buffer for rename / new-category input in the feed editor.
    pub editor_input: String,
    /// Which panel has focus in the split editor (Categories = left, Feeds = right).
    pub editor_panel: EditorPanel,
    /// Cursor in the left (categories-only) panel of the editor.
    pub editor_cat_cursor: usize,
    /// Pending category delete: (id, total_feeds_to_delete). Set on [d], cleared on Esc or after confirm.
    pub editor_delete_cat: Option<(CategoryId, usize)>,

    // ── Status message animation ─────────────────────────────────────────────
    /// Value of `tick` when `status_msg` was last set — used to compute per-message scroll offset.
    pub status_msg_start_tick: usize,

    // ── Title auto-scroll animation ──────────────────────────────────────────
    /// Value of `tick` when the sidebar cursor last moved — used to scroll long feed titles.
    pub sidebar_title_start_tick: usize,
    /// Value of `tick` when the article selection last changed — used to scroll long article titles.
    pub article_title_start_tick: usize,
}

impl App {
    pub fn new() -> Self {
        let user_data = load_user_data();
        let feeds = load_feeds();
        let categories = load_categories();

        // Sync initial selected_feed with the first real (non-Favorites) feed in the visible tree.
        let initial_items = visible_tree_items(&categories, &feeds, &HashSet::new());
        let selected_feed = initial_items
            .iter()
            .find_map(|item| {
                if let FeedTreeItem::Feed { feeds_idx, .. } = item {
                    Some(*feeds_idx)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        let initial_editor_cursor = initial_items.iter()
            .position(|item| matches!(item, FeedTreeItem::Feed { .. }))
            .unwrap_or(0);

        Self {
            state: AppState::FeedList,
            feeds,
            selected_feed,
            selected_article: 0,
            status_msg: "Fetching feeds...".to_string(),
            input: String::new(),
            user_data,
            settings_selected: SettingsItem::ImportOpml,
            scroll_offset: 0,
            content_line_count: 0,
            content_area_height: 20,
            tick: 0,
            sidebar_list_state: ListState::default(),
            saved_sidebar_list_state: ListState::default(),
            article_list_state: ListState::default(),
            editor_feeds_list_state: ListState::default(),
            editor_cats_list_state: ListState::default(),
            selected_tab: Tab::Feeds,
            saved_sidebar_cursor: 0,
            saved_view_articles: Vec::new(),
            in_saved_context: false,
            selected_saved_category: None,
            category_picker_cursor: 0,
            category_picker_new_mode: false,
            category_picker_input: String::new(),
            category_picker_return_state: AppState::ArticleList,
            saved_cat_editor_scroll: ListScroll::default(),
            add_feed_step: AddFeedStep::Url,
            add_feed_url: String::new(),
            add_feed_fetched_title: None,
            add_feed_return_state: AppState::SettingsList,
            add_feed_target_category: None,
            add_feed_target_order: None,
            opml_path_input: String::new(),
            feeds_total: 0,
            feeds_pending: 0,
            article_fetching: false,
            categories,
            sidebar_collapsed: HashSet::new(),
            sidebar_cursor: 0,
            selected_sidebar_category: None,
            category_view_articles: Vec::new(),
            in_category_context: false,
            article_cache_size: article_cache_size(),
            editor_cursor: initial_editor_cursor,
            editor_collapsed: HashSet::new(),
            editor_mode: FeedEditorMode::Normal,
            editor_input: String::new(),
            editor_panel: EditorPanel::Feeds,
            editor_cat_cursor: 0,
            editor_delete_cat: None,
            status_msg_start_tick: 0,
            sidebar_title_start_tick: 0,
            article_title_start_tick: 0,
        }
    }

    /// Set the status bar message and reset the per-message scroll animation.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_msg = msg.into();
        self.status_msg_start_tick = self.tick;
    }

    // ── Tab switching ────────────────────────────────────────────────────────

    pub fn switch_tab_left(&mut self) {
        self.selected_tab = self.selected_tab.prev();
        self.apply_tab_state();
    }

    pub fn switch_tab_right(&mut self) {
        self.selected_tab = self.selected_tab.next();
        self.apply_tab_state();
    }

    fn apply_tab_state(&mut self) {
        self.in_saved_context = false;
        self.state = match self.selected_tab {
            Tab::Feeds => AppState::FeedList,
            Tab::Saved => AppState::SavedCategoryList,
            Tab::Settings => AppState::SettingsList,
        };
        if self.selected_tab == Tab::Saved {
            self.sync_saved_preview();
        }
    }

    // ── Standard navigation ──────────────────────────────────────────────────

    /// Advance selection forward (wraps around).
    pub fn next(&mut self) {
        match self.state {
            AppState::FeedList => {
                let items =
                    visible_tree_items(&self.categories, &self.feeds, &self.sidebar_collapsed);
                if items.is_empty() {
                    return;
                }

                self.sidebar_cursor = (self.sidebar_cursor + 1) % items.len();
                self.sidebar_title_start_tick = self.tick;
                match items.get(self.sidebar_cursor) {
                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                        self.selected_feed = *feeds_idx;
                        self.selected_article = 0;
                        self.clear_category_view();
                    }
                    Some(FeedTreeItem::Category { id, .. }) => {
                        self.populate_category_view(*id);
                    }
                    None => {}
                }
            }
            AppState::ArticleList => {
                let len = if self.in_category_context {
                    self.category_view_articles.len()
                } else if self.in_saved_context {
                    self.saved_view_articles.len()
                } else {
                    self.feeds
                        .get(self.selected_feed)
                        .map_or(0, |f| f.articles.len())
                };
                if len > 0 {
                    self.selected_article = (self.selected_article + 1) % len;
                    self.article_title_start_tick = self.tick;
                }
            }
            AppState::SettingsList => {
                self.settings_selected = self.settings_selected.next();
            }
            AppState::SavedCategoryList => {
                let count = 1 + self.user_data.saved_categories.len();
                self.saved_sidebar_cursor = (self.saved_sidebar_cursor + 1) % count;
                self.sidebar_title_start_tick = self.tick;
                self.sync_saved_preview();
            }
            AppState::FeedEditor => {
                if self.editor_panel == EditorPanel::Categories {
                    let cats = visible_cat_only_items(&self.categories, &self.feeds, &self.editor_collapsed);
                    if !cats.is_empty() {
                        let items = visible_tree_items(&self.categories, &self.feeds, &self.editor_collapsed);
                        let is_cat_moving = self.editor_moving_category(&items);
                        let wrap_len = if is_cat_moving { cats.len() + 1 } else { cats.len() };
                        self.editor_cat_cursor = (self.editor_cat_cursor + 1) % wrap_len;
                    }
                } else {
                    let items =
                        visible_tree_items(&self.categories, &self.feeds, &self.editor_collapsed);
                    if matches!(self.editor_mode, FeedEditorMode::Moving { .. }) {
                        // During a feed move: navigate ALL items so the cursor can land on a
                        // category header (dropping there = insert as first child of that category).
                        if !items.is_empty() {
                            self.editor_cursor = (self.editor_cursor + 1) % items.len();
                        }
                    } else {
                        // Normal: only Feed items (categories are non-interactive headers).
                        let feed_indices: Vec<usize> = items
                            .iter()
                            .enumerate()
                            .filter(|(_, item)| matches!(item, FeedTreeItem::Feed { .. }))
                            .map(|(i, _)| i)
                            .collect();
                        if !feed_indices.is_empty() {
                            let cur = feed_indices
                                .iter()
                                .position(|&i| i == self.editor_cursor)
                                .unwrap_or(0);
                            self.editor_cursor = feed_indices[(cur + 1) % feed_indices.len()];
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Advance selection backward (wraps around).
    pub fn previous(&mut self) {
        match self.state {
            AppState::FeedList => {
                let items =
                    visible_tree_items(&self.categories, &self.feeds, &self.sidebar_collapsed);
                if items.is_empty() {
                    return;
                }

                self.sidebar_cursor = self
                    .sidebar_cursor
                    .checked_sub(1)
                    .unwrap_or(items.len() - 1);
                self.sidebar_title_start_tick = self.tick;

                match items.get(self.sidebar_cursor) {
                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                        self.selected_feed = *feeds_idx;
                        self.selected_article = 0;
                        self.clear_category_view();
                    }
                    Some(FeedTreeItem::Category { id, .. }) => {
                        self.populate_category_view(*id);
                    }
                    None => {}
                }
            }
            AppState::ArticleList => {
                let len = if self.in_category_context {
                    self.category_view_articles.len()
                } else if self.in_saved_context {
                    self.saved_view_articles.len()
                } else {
                    self.feeds
                        .get(self.selected_feed)
                        .map_or(0, |f| f.articles.len())
                };
                if len > 0 {
                    self.selected_article = self.selected_article.checked_sub(1).unwrap_or(len - 1);
                    self.article_title_start_tick = self.tick;
                }
            }
            AppState::SettingsList => {
                self.settings_selected = self.settings_selected.prev();
            }
            AppState::SavedCategoryList => {
                let count = 1 + self.user_data.saved_categories.len();
                self.saved_sidebar_cursor = self
                    .saved_sidebar_cursor
                    .checked_sub(1)
                    .unwrap_or(count - 1);
                self.sidebar_title_start_tick = self.tick;
                self.sync_saved_preview();
            }
            AppState::FeedEditor => {
                if self.editor_panel == EditorPanel::Categories {
                    let cats = visible_cat_only_items(&self.categories, &self.feeds, &self.editor_collapsed);
                    if !cats.is_empty() {
                        let items = visible_tree_items(&self.categories, &self.feeds, &self.editor_collapsed);
                        let is_cat_moving = self.editor_moving_category(&items);
                        let wrap_len = if is_cat_moving { cats.len() + 1 } else { cats.len() };
                        self.editor_cat_cursor = self.editor_cat_cursor.checked_sub(1).unwrap_or(wrap_len - 1);
                    }
                } else {
                    let items =
                        visible_tree_items(&self.categories, &self.feeds, &self.editor_collapsed);
                    if matches!(self.editor_mode, FeedEditorMode::Moving { .. }) {
                        // During a feed move: navigate ALL items.
                        if !items.is_empty() {
                            self.editor_cursor =
                                self.editor_cursor.checked_sub(1).unwrap_or(items.len() - 1);
                        }
                    } else {
                        // Normal: only Feed items.
                        let feed_indices: Vec<usize> = items
                            .iter()
                            .enumerate()
                            .filter(|(_, item)| matches!(item, FeedTreeItem::Feed { .. }))
                            .map(|(i, _)| i)
                            .collect();
                        if !feed_indices.is_empty() {
                            let cur = feed_indices
                                .iter()
                                .position(|&i| i == self.editor_cursor)
                                .unwrap_or(0);
                            self.editor_cursor = feed_indices
                                [cur.checked_sub(1).unwrap_or(feed_indices.len() - 1)];
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Descend into the next layer of the navigation hierarchy.
    pub fn select(&mut self) {
        match self.state {
            AppState::FeedList => {
                let items =
                    visible_tree_items(&self.categories, &self.feeds, &self.sidebar_collapsed);
                match items.get(self.sidebar_cursor) {
                    Some(FeedTreeItem::Category { id, .. }) => {
                        self.populate_category_view(*id);
                        if !self.category_view_articles.is_empty() {
                            self.in_category_context = true;
                            self.selected_article = 0;
                            self.state = AppState::ArticleList;
                        }
                    }
                    Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
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
                let has_articles = if self.in_category_context {
                    !self.category_view_articles.is_empty()
                } else if self.in_saved_context {
                    !self.saved_view_articles.is_empty()
                } else {
                    self.feeds
                        .get(self.selected_feed)
                        .is_some_and(|f| !f.articles.is_empty())
                };
                if has_articles {
                    self.state = AppState::ArticleDetail;
                    self.scroll_offset = 0;
                    let content = if self.in_category_context {
                        let (fi, ai) = self.category_view_articles[self.selected_article];
                        self.feeds[fi].articles[ai].content.clone()
                    } else if self.in_saved_context {
                        self.saved_view_articles[self.selected_article].content.clone()
                    } else {
                        self.feeds[self.selected_feed].articles[self.selected_article]
                            .content
                            .clone()
                    };
                    self.content_line_count = content.lines().count().max(1);
                }
            }
            _ => {}
        }
    }

    /// Populate `category_view_articles` for the given category id and mark it as selected.
    pub fn populate_category_view(&mut self, cat_id: CategoryId) {
        self.selected_sidebar_category = Some(cat_id);
        self.category_view_articles.clear();

        // Collect (fi, ai, published_secs) avoiding simultaneous borrows.
        let feed_indices = feeds_in_category(cat_id, &self.categories, &self.feeds);
        let mut triples: Vec<(usize, usize, Option<i64>)> = Vec::new();
        for fi in feed_indices {
            for ai in 0..self.feeds[fi].articles.len() {
                triples.push((fi, ai, self.feeds[fi].articles[ai].published_secs));
            }
        }
        triples.sort_by(|a, b| match (a.2, b.2) {
            (Some(x), Some(y)) => y.cmp(&x),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        self.category_view_articles = triples.into_iter().map(|(fi, ai, _)| (fi, ai)).collect();
    }

    /// Clear the category context (cursor moved off a category onto a feed).
    pub fn clear_category_view(&mut self) {
        self.selected_sidebar_category = None;
        self.category_view_articles.clear();
        self.in_category_context = false;
    }

    /// Toggle the collapsed state of a category in the sidebar.
    pub fn toggle_category_collapse(&mut self, id: CategoryId) {
        if self.sidebar_collapsed.contains(&id) {
            self.sidebar_collapsed.remove(&id);
        } else {
            self.sidebar_collapsed.insert(id);
        }
        // Refresh the preview if still hovering over this category.
        if self.selected_sidebar_category == Some(id) {
            self.populate_category_view(id);
        }
    }

    /// Populate `saved_view_articles` from the currently selected sidebar entry.
    /// 0 = "All Saved", 1+ = category index.
    pub fn sync_saved_preview(&mut self) {
        if self.saved_sidebar_cursor == 0 {
            self.saved_view_articles = self
                .user_data
                .saved_articles
                .iter()
                .map(|s| {
                    let mut art = s.article.clone();
                    art.is_saved = true;
                    art
                })
                .collect();
            self.selected_saved_category = None;
            self.in_saved_context = !self.saved_view_articles.is_empty();
        } else {
            let cat_idx = self.saved_sidebar_cursor - 1;
            if let Some(cat) = self.user_data.saved_categories.get(cat_idx) {
                let cat_id = cat.id;
                self.selected_saved_category = Some(cat_id);
                self.saved_view_articles = self
                    .user_data
                    .saved_articles
                    .iter()
                    .filter(|s| s.category_id == cat_id)
                    .map(|s| {
                        let mut art = s.article.clone();
                        art.is_saved = true;
                        art
                    })
                    .collect();
                self.in_saved_context = !self.saved_view_articles.is_empty();
            } else {
                self.saved_view_articles.clear();
                self.in_saved_context = false;
            }
        }
    }

    /// True when currently moving a category — used to skip feeds during navigation.
    fn editor_moving_category(&self, items: &[FeedTreeItem]) -> bool {
        if let FeedEditorMode::Moving { origin_render_idx, .. } = &self.editor_mode {
            matches!(items.get(*origin_render_idx), Some(FeedTreeItem::Category { .. }))
        } else {
            false
        }
    }

    /// Ascend back up the navigation hierarchy.
    pub fn unselect(&mut self) {
        match self.state {
            AppState::ArticleList => {
                if self.in_category_context {
                    // Return to FeedList; keep category_view_articles populated for preview.
                    self.in_category_context = false;
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
                self.input.clear();
                self.add_feed_step = AddFeedStep::Url;
                self.add_feed_url.clear();
                self.add_feed_fetched_title = None;
                self.add_feed_target_category = None;
                self.state = self.add_feed_return_state.clone();
            }
            AppState::SettingsList => {
                self.settings_selected = SettingsItem::ImportOpml;
                self.selected_tab = Tab::Feeds;
                self.state = AppState::FeedList;
            }
            AppState::OPMLExportPath | AppState::OPMLImportPath => {
                self.opml_path_input.clear();
                self.state = AppState::SettingsList;
            }
            AppState::ClearData => self.state = AppState::SettingsList,
            AppState::SavedCategoryList => {
                self.in_saved_context = false;
                self.selected_tab = Tab::Feeds;
                self.state = AppState::FeedList;
            }
            AppState::FeedEditor => {
                self.editor_mode = FeedEditorMode::Normal;
                self.selected_tab = Tab::Feeds;
                self.state = AppState::FeedList;
            }
            AppState::FeedEditorRename => {
                self.editor_input.clear();
                self.editor_mode = FeedEditorMode::Normal;
                self.state = AppState::FeedEditor;
            }
            _ => {}
        }
    }
}

/// Compute the flattened, visible rows of the feed/category tree.
///
/// - Feeds with `url == FAVORITES_URL` are always excluded.
/// - Categories with their id in `collapsed` hide their children.
/// - Root-level categories are sorted by `category.order`, feeds within
///   each category by `feed.order`.
/// - Uncategorized feeds appear after all categories.
pub fn visible_tree_items(
    categories: &[Category],
    feeds: &[Feed],
    collapsed: &HashSet<CategoryId>,
) -> Vec<FeedTreeItem> {
    visible_tree_items_filtered(categories, feeds, collapsed, None)
}

/// Collects feed indices for all feeds directly or transitively under `cat_id`,
/// in tree order (sorted by order field within each level).
fn feeds_in_category(
    cat_id: CategoryId,
    categories: &[Category],
    feeds: &[Feed],
) -> Vec<usize> {
    let mut result = Vec::new();

    // Direct feeds in this category, sorted by order.
    let mut direct: Vec<(usize, &Feed)> = feeds
        .iter()
        .enumerate()
        .filter(|(_, f)| f.category_id == Some(cat_id) && f.url != FAVORITES_URL)
        .collect();
    direct.sort_by_key(|(_, f)| f.order);
    result.extend(direct.iter().map(|(i, _)| *i));

    // Subcategories, sorted by order.
    let mut subcats: Vec<&Category> = categories
        .iter()
        .filter(|c| c.parent_id == Some(cat_id))
        .collect();
    subcats.sort_by_key(|c| c.order);
    for subcat in subcats {
        result.extend(feeds_in_category(subcat.id, categories, feeds));
    }

    result
}

/// Returns only `FeedTreeItem::Category` rows from the visible tree.
/// Used by the left (categories-only) panel of the split editor.
pub fn visible_cat_only_items(
    categories: &[Category],
    feeds: &[Feed],
    collapsed: &HashSet<CategoryId>,
) -> Vec<FeedTreeItem> {
    visible_tree_items(categories, feeds, collapsed)
        .into_iter()
        .filter(|item| matches!(item, FeedTreeItem::Category { .. }))
        .collect()
}

fn visible_tree_items_filtered(
    categories: &[Category],
    feeds: &[Feed],
    collapsed: &HashSet<CategoryId>,
    feed_filter: Option<&HashSet<usize>>,
) -> Vec<FeedTreeItem> {
    let mut result = Vec::new();
    collect_tree_level(
        categories,
        feeds,
        collapsed,
        None,
        0,
        &mut result,
        feed_filter,
    );

    let mut uncategorized: Vec<(usize, &Feed)> = feeds
        .iter()
        .enumerate()
        .filter(|(idx, f)| {
            f.url != FAVORITES_URL
                && f.category_id.is_none()
                && feed_filter.is_none_or(|s| s.contains(idx))
        })
        .collect();
    uncategorized.sort_by_key(|(_, f)| f.order);
    for (feeds_idx, _) in uncategorized {
        result.push(FeedTreeItem::Feed {
            feeds_idx,
            depth: 0,
        });
    }

    result
}

fn collect_tree_level(
    categories: &[Category],
    feeds: &[Feed],
    collapsed: &HashSet<CategoryId>,
    parent_id: Option<CategoryId>,
    depth: u8,
    result: &mut Vec<FeedTreeItem>,
    feed_filter: Option<&HashSet<usize>>,
) {
    let mut cats: Vec<&Category> = categories
        .iter()
        .filter(|c| c.parent_id == parent_id)
        .collect();
    cats.sort_by_key(|c| c.order);

    for cat in cats {
        // When filtering, skip categories that have no visible descendants.
        if let Some(filter) = feed_filter
            && !category_has_visible_feeds(cat.id, categories, feeds, filter)
        {
            continue;
        }

        let is_collapsed = collapsed.contains(&cat.id);
        result.push(FeedTreeItem::Category {
            id: cat.id,
            depth,
            collapsed: is_collapsed,
        });

        if !is_collapsed {
            collect_tree_level(
                categories,
                feeds,
                collapsed,
                Some(cat.id),
                depth + 1,
                result,
                feed_filter,
            );

            let mut cat_feeds: Vec<(usize, &Feed)> = feeds
                .iter()
                .enumerate()
                .filter(|(idx, f)| {
                    f.url != FAVORITES_URL
                        && f.category_id == Some(cat.id)
                        && feed_filter.is_none_or(|s| s.contains(idx))
                })
                .collect();
            cat_feeds.sort_by_key(|(_, f)| f.order);
            for (feeds_idx, _) in cat_feeds {
                result.push(FeedTreeItem::Feed {
                    feeds_idx,
                    depth: depth + 1,
                });
            }
        }
    }
}

/// Returns true if `cat_id` has at least one descendant feed in `feed_filter`.
fn category_has_visible_feeds(
    cat_id: CategoryId,
    categories: &[Category],
    feeds: &[Feed],
    feed_filter: &HashSet<usize>,
) -> bool {
    feeds.iter().enumerate().any(|(i, f)| {
        f.url != FAVORITES_URL && f.category_id == Some(cat_id) && feed_filter.contains(&i)
    }) || categories
        .iter()
        .filter(|c| c.parent_id == Some(cat_id))
        .any(|c| category_has_visible_feeds(c.id, categories, feeds, feed_filter))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_article(title: &str) -> Article {
        Article {
            title: title.to_string(),
            description: String::new(),
            link: title.to_string(),
            is_read: false,
            is_saved: false,
            content: String::new(),
            image_url: None,
            source_feed: String::new(),
            published_secs: None,
        }
    }

    fn app_with_feed() -> App {
        let mut app = App::new();
        // Clear any disk-loaded state so tests are isolated
        app.feeds.clear();
        app.categories.clear();
        app.sidebar_cursor = 0;
        app.feeds.push(Feed {
            title: "Test Feed".to_string(),
            url: "https://example.com/feed.rss".to_string(),
            category_id: None,
            order: 0,
            unread_count: 0,
            articles: vec![mock_article("A"), mock_article("B")],
            fetched: false,
            fetch_error: None,
            feed_updated_secs: None,
                last_fetched_secs: None,
        });
        app
    }

    #[test]
    fn test_initial_state() {
        let app = App::new();
        assert_eq!(app.state, AppState::FeedList);
        assert_eq!(app.selected_tab, Tab::Feeds);
    }

    #[test]
    fn test_navigation_wrapping() {
        let mut app = app_with_feed();
        // sidebar_cursor=0 points to the test feed (no categories, no Favorites)
        app.select(); // FeedList -> ArticleList
        assert_eq!(app.state, AppState::ArticleList);
        assert_eq!(app.selected_feed, 0); // test feed is at feeds[0] (no virtual Favorites in tests)

        app.next(); // 0 -> 1
        assert_eq!(app.selected_article, 1);
        app.next(); // 1 -> 0 (wrap)
        assert_eq!(app.selected_article, 0);

        app.select(); // ArticleList -> ArticleDetail
        assert_eq!(app.state, AppState::ArticleDetail);
        assert_eq!(app.scroll_offset, 0);

        app.unselect(); // -> ArticleList
        assert_eq!(app.state, AppState::ArticleList);
        app.unselect(); // -> FeedList
        assert_eq!(app.state, AppState::FeedList);
    }

    #[test]
    fn test_tab_switching() {
        let mut app = App::new();
        assert_eq!(app.selected_tab, Tab::Feeds);
        app.switch_tab_right();
        assert_eq!(app.selected_tab, Tab::Saved);
        assert_eq!(app.state, AppState::SavedCategoryList);
        app.switch_tab_right();
        assert_eq!(app.selected_tab, Tab::Settings);
        assert_eq!(app.state, AppState::SettingsList);
        app.switch_tab_right(); // wraps back to Feeds
        assert_eq!(app.selected_tab, Tab::Feeds);
        assert_eq!(app.state, AppState::FeedList);
    }

    #[test]
    fn test_visible_tree_items_excludes_favorites() {
        let cats = vec![];
        let feeds = vec![
            Feed {
                title: "⭐ Favorites".into(),
                url: FAVORITES_URL.into(),
                category_id: None,
                order: 0,
                unread_count: 0,
                articles: vec![],
                fetched: false,
                fetch_error: None,
                feed_updated_secs: None,
                last_fetched_secs: None,
            },
            Feed {
                title: "News".into(),
                url: "https://news.example.com/feed".into(),
                category_id: None,
                order: 0,
                unread_count: 0,
                articles: vec![],
                fetched: false,
                fetch_error: None,
                feed_updated_secs: None,
                last_fetched_secs: None,
            },
        ];
        let collapsed = HashSet::new();
        let items = visible_tree_items(&cats, &feeds, &collapsed);
        // Only "News" visible; Favorites excluded by URL check
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], FeedTreeItem::Feed { feeds_idx: 1, .. }));
    }

    #[test]
    fn test_visible_tree_items_collapsed_hides_children() {
        let cats = vec![Category {
            id: 1,
            name: "Tech".into(),
            parent_id: None,
            order: 0,
        }];
        let feeds = vec![
            Feed {
                title: "⭐ Favorites".into(),
                url: FAVORITES_URL.into(),
                category_id: None,
                order: 0,
                unread_count: 0,
                articles: vec![],
                fetched: false,
                fetch_error: None,
                feed_updated_secs: None,
                last_fetched_secs: None,
            },
            Feed {
                title: "HN".into(),
                url: "https://news.ycombinator.com/rss".into(),
                category_id: Some(1),
                order: 0,
                unread_count: 0,
                articles: vec![],
                fetched: false,
                fetch_error: None,
                feed_updated_secs: None,
                last_fetched_secs: None,
            },
        ];
        let mut collapsed = HashSet::new();
        collapsed.insert(1u64);
        let items = visible_tree_items(&cats, &feeds, &collapsed);
        // Only the category header; feed hidden
        assert_eq!(items.len(), 1);
        assert!(matches!(
            items[0],
            FeedTreeItem::Category {
                id: 1,
                collapsed: true,
                ..
            }
        ));
    }
}
