//! Central application state and navigation logic.
//!
//! Owns the [`App`] struct (passed to every draw frame and input handler) and all
//! navigation methods: cursor movement, tab switching, tree traversal, and scroll management.

use crate::models::ThemeEditorState;
use crate::models::{
    AddFeedStep, AppState, Article, Category, CategoryId, EditorPanel, FAVORITES_URL, Feed,
    FeedEditorMode, FeedTreeItem, ListScroll, SettingsItem, Tab, TextScroll, UpdateInfo, UserData,
};
use crate::state::{AddFeedState, CategoryPickerState, FeedEditorState, OpmlState};
use crate::storage::{
    article_cache_size, load_categories, load_feeds, load_user_data, save_categories, save_feeds,
    save_user_data,
};
use crate::ui::theme::ColorTheme;
use limner::Alignment;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::collections::{HashMap, HashSet};

/// Build the active [`ColorTheme`] from persisted user preferences.
///
/// Uses the selected built-in slug, or finds the custom theme by `selected_custom_id`.
/// Falls back to Catppuccin Mocha on any error or missing theme.
pub(crate) fn resolve_theme(user_data: &UserData) -> ColorTheme {
    if user_data.selected_theme != "custom" {
        return ColorTheme::builtin(&user_data.selected_theme)
            .unwrap_or_else(ColorTheme::catppuccin_mocha);
    }
    if let Some(id) = user_data.selected_custom_id
        && let Some(ct) = user_data.custom_themes.iter().find(|t| t.id == id)
        && let Ok(t) = ColorTheme::from_custom_theme(ct)
    {
        return t;
    }
    ColorTheme::catppuccin_mocha()
}

/// Central application state passed to every frame draw and event handler.
pub struct App {
    pub state: AppState,
    pub feeds: Vec<Feed>,
    pub selected_feed: usize,
    pub selected_article: usize,
    pub status_msg: String,
    pub user_data: UserData,
    pub settings_selected: SettingsItem,
    pub article_scroll: TextScroll,
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

    /// When true, renders only the article full-screen with no chrome (zen reading mode).
    pub zen_mode: bool,

    // ── Category picker ──────────────────────────────────────────────────────
    /// State for the CategoryPicker modal.
    pub category_picker: CategoryPickerState,

    // ── Saved category editor ────────────────────────────────────────────────
    /// Scroll state for the SavedCategoryEditor list.
    pub saved_cat_editor_scroll: ListScroll,

    // ── Multi-step AddFeed ───────────────────────────────────────────────────
    /// State for the multi-step AddFeed wizard.
    pub add_feed: AddFeedState,

    // ── OPML path input ──────────────────────────────────────────────────────
    /// State for OPML import/export path input.
    pub opml: OpmlState,

    // ── Fetch progress ───────────────────────────────────────────────────────
    /// Total feeds being fetched in the current batch.
    pub feeds_total: usize,
    /// Feeds still pending a result.
    pub feeds_pending: usize,
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
    /// True when `category_view_articles` holds all-feeds content rather than a real category.
    pub in_all_feeds_context: bool,

    // ── Cache ────────────────────────────────────────────────────────────────
    /// Byte size of articles.json; refreshed at startup and after cache clear.
    pub article_cache_size: u64,

    // ── Feed editor ──────────────────────────────────────────────────────────
    /// State for the feed editor and saved-category editor.
    pub feed_editor: FeedEditorState,

    // ── Changelog tab ────────────────────────────────────────────────────────
    /// Scroll offset for the Changelog tab view.
    pub changelog_scroll: u16,
    /// Indices of collapsed changelog entries (0 = newest, reversed display order).
    pub changelog_collapsed: std::collections::HashSet<usize>,
    /// Currently selected changelog entry index.
    pub changelog_cursor: usize,

    // ── Status message animation ─────────────────────────────────────────────
    /// Latest version available on crates.io, if newer than the running version. `None` until the
    /// background check completes or if already up to date.
    pub update_available: Option<UpdateInfo>,
    /// Current scroll offset (in lines) for the update-available popup.
    pub update_popup_scroll: u16,
    /// Value of `tick` when `status_msg` was last set — used to compute per-message scroll offset.
    pub status_msg_start_tick: usize,

    // ── Article fetching ──────────────────────────────────────────────────────
    /// Whether a full-article readability fetch is in flight.
    pub article_fetching: bool,

    // ── Image cache ──────────────────────────────────────────────────────────
    /// Decoded image pixel data keyed by URL.
    pub image_cache: HashMap<String, limner::render_image::img_crate::DynamicImage>,
    /// URLs currently being downloaded.
    pub image_pending: HashSet<String>,

    // ── Article image rendering ──────────────────────────────────────────────
    /// Per-URL encoded terminal protocols (Kitty / Sixel / half-block).
    pub protocol_cache: HashMap<String, limner::render_image::Protocol>,
    /// Image picker (terminal protocol detection) — initialized in [`run()`].
    pub picker: Option<limner::render_image::Picker>,
    /// Image metadata from the latest markdown render.
    pub article_images: Vec<limner::ImageInfo>,
    /// Link metadata from the latest markdown render.
    pub article_links: Vec<limner::LinkInfo>,
    /// Content-area rect of the article detail panel.
    pub article_content_area: Rect,
    /// Current scroll offset of the article detail view.
    pub article_scroll_offset: u16,

    /// Body text alignment for article reading.
    pub body_alignment: Alignment,

    /// Width of the zen mode content area as a percentage (30–90).
    pub zen_width: u8,

    // ── Title auto-scroll animation ──────────────────────────────────────────
    /// Value of `tick` when the sidebar cursor last moved — used to scroll long feed titles.
    pub sidebar_title_start_tick: usize,
    /// Value of `tick` when the article selection last changed — used to scroll long article titles.
    pub article_title_start_tick: usize,

    // ── Theme ────────────────────────────────────────────────────────────────
    /// Active color theme — derived from `user_data.selected_theme` on startup.
    pub theme: ColorTheme,
    /// State for the theme editor (cursors and editing context).
    pub theme_editor: ThemeEditorState,
}

/// Sorts `(feed_idx, article_idx, published_secs)` triples newest-first.
/// Entries with no timestamp sort after those with one.
fn sort_articles_by_date(triples: &mut [(usize, usize, Option<i64>)]) {
    triples.sort_by(|a, b| match (a.2, b.2) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
}

/// Advance a cursor index, wrapping or clamping based on `wrap`. Returns `None` when
/// already at the end and not wrapping (caller should early-return).
pub(crate) fn cursor_next(cursor: usize, len: usize, wrap: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(if wrap {
        (cursor + 1) % len
    } else {
        (cursor + 1).min(len - 1)
    })
}

/// Retreat a cursor index, wrapping or clamping based on `wrap`. Returns `None` when
/// already at the start and not wrapping.
pub(crate) fn cursor_prev(cursor: usize, len: usize, wrap: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(if wrap {
        cursor.checked_sub(1).unwrap_or(len - 1)
    } else {
        cursor.saturating_sub(1)
    })
}

impl App {
    /// Construct a fresh `App`, loading feeds, categories, and user data from disk.
    pub fn new() -> Self {
        let user_data = load_user_data();
        let theme = resolve_theme(&user_data);
        let feeds = load_feeds();
        let categories = load_categories();

        // Sync initial selected_feed with the first real (non-Favorites) feed in the visible tree.
        let initial_sidebar = sidebar_tree_items(&categories, &feeds, &HashSet::new());
        let selected_feed = initial_sidebar
            .iter()
            .find_map(|item| {
                if let FeedTreeItem::Feed { feeds_idx, .. } = item {
                    Some(*feeds_idx)
                } else {
                    None
                }
            })
            .unwrap_or(0);

        // Load persisted sidebar and changelog collapse state before moving user_data.
        let initial_collapsed: HashSet<CategoryId> =
            user_data.sidebar_collapsed.iter().copied().collect();
        let initial_changelog_collapsed: HashSet<usize> =
            user_data.changelog_collapsed.iter().copied().collect();

        // Parse body alignment before user_data is moved into Self.
        let initial_body_alignment = {
            let s = user_data.body_alignment.to_lowercase();
            match s.as_str() {
                "center" => Alignment::Center,
                "right" => Alignment::Right,
                "justify" => Alignment::Justify,
                _ => Alignment::Left,
            }
        };

        // Clamp zen width to valid range: 30–90, default 66.
        let initial_zen_width = user_data.zen_width.clamp(30, 90);

        // Editor cursor uses the non-AllFeeds tree (editor never shows AllFeeds).
        let initial_editor_items = visible_tree_items(&categories, &feeds, &HashSet::new());
        let initial_editor_cursor = initial_editor_items
            .iter()
            .position(|item| matches!(item, FeedTreeItem::Feed { .. }))
            .unwrap_or(0);

        let mut app = Self {
            state: AppState::FeedList,
            feeds,
            selected_feed,
            selected_article: 0,
            status_msg: "Fetching feeds...".to_string(),
            user_data,
            settings_selected: SettingsItem::ImportOpml,
            article_scroll: TextScroll::default(),
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
            zen_mode: false,
            category_picker: CategoryPickerState::default(),
            saved_cat_editor_scroll: ListScroll::default(),
            add_feed: AddFeedState::default(),
            opml: OpmlState::default(),
            feeds_total: 0,
            feeds_pending: 0,
            categories,
            sidebar_collapsed: initial_collapsed,
            sidebar_cursor: 0,
            selected_sidebar_category: None,
            category_view_articles: Vec::new(),
            in_category_context: false,
            in_all_feeds_context: false,
            article_cache_size: article_cache_size(),
            feed_editor: FeedEditorState {
                cursor: initial_editor_cursor,
                collapsed: HashSet::new(),
                ..Default::default()
            },
            changelog_scroll: 0,
            changelog_collapsed: initial_changelog_collapsed,
            changelog_cursor: 0,
            article_fetching: false,
            update_available: None,
            update_popup_scroll: 0,
            status_msg_start_tick: 0,
            image_cache: HashMap::new(),
            image_pending: HashSet::new(),
            protocol_cache: HashMap::new(),
            picker: None,
            article_images: Vec::new(),
            article_links: Vec::new(),
            article_content_area: Rect::default(),
            article_scroll_offset: 0,
            body_alignment: initial_body_alignment,
            zen_width: initial_zen_width,
            sidebar_title_start_tick: 0,
            article_title_start_tick: 0,
            theme,
            theme_editor: ThemeEditorState::default(),
        };

        app.populate_all_feeds_view();
        app
    }

    /// Set the status bar message and reset the per-message scroll animation.
    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_msg = msg.into();
        self.status_msg_start_tick = self.tick;
    }

    /// Cycle body alignment to the next variant.
    pub fn cycle_alignment_next(&mut self) {
        self.body_alignment = match self.body_alignment {
            Alignment::Left => Alignment::Center,
            Alignment::Center => Alignment::Right,
            Alignment::Right => Alignment::Justify,
            Alignment::Justify => Alignment::Left,
        };
    }

    /// Cycle body alignment to the previous variant.
    pub fn cycle_alignment_prev(&mut self) {
        self.body_alignment = match self.body_alignment {
            Alignment::Left => Alignment::Justify,
            Alignment::Justify => Alignment::Right,
            Alignment::Right => Alignment::Center,
            Alignment::Center => Alignment::Left,
        };
    }

    /// Human-readable label for the current body alignment.
    pub fn alignment_label(&self) -> &'static str {
        match self.body_alignment {
            Alignment::Left => "Left",
            Alignment::Center => "Center",
            Alignment::Right => "Right",
            Alignment::Justify => "Justify",
        }
    }

    /// Cycle zen width to the next value (steps of 5%, stops at 90).
    pub fn cycle_zen_width_next(&mut self) {
        self.zen_width = match self.zen_width {
            30 => 35,
            35 => 40,
            40 => 45,
            45 => 50,
            50 => 55,
            55 => 60,
            60 => 65,
            65 => 66,
            66 => 70,
            70 => 75,
            75 => 80,
            80 => 85,
            85 => 90,
            _ => 90,
        };
    }

    /// Cycle zen width to the previous value (steps of 5%, stops at 30).
    pub fn cycle_zen_width_prev(&mut self) {
        self.zen_width = match self.zen_width {
            35 => 30,
            40 => 35,
            45 => 40,
            50 => 45,
            55 => 50,
            60 => 55,
            65 => 60,
            66 => 65,
            70 => 66,
            75 => 70,
            80 => 75,
            85 => 80,
            90 => 85,
            _ => 30,
        };
    }

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
            let items = sidebar_tree_items(&self.categories, &self.feeds, &self.sidebar_collapsed);
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
    }

    // ── Standard navigation ──────────────────────────────────────────────────

    /// Advance selection forward (wraps around).
    pub fn next(&mut self) {
        match self.state {
            AppState::FeedList => self.move_sidebar_cursor(true),
            AppState::ArticleList => self.move_article_cursor(true),
            AppState::SettingsList
                if (self.user_data.scroll_loop
                    || self.settings_selected != SettingsItem::Theme) =>
            {
                self.settings_selected = self.settings_selected.next();
            }
            AppState::SavedCategoryList => self.move_saved_cursor(true),
            AppState::FeedEditor => {
                if self.feed_editor.panel == EditorPanel::Categories {
                    let cats = visible_cat_only_items(
                        &self.categories,
                        &self.feeds,
                        &self.feed_editor.collapsed,
                    );
                    if !cats.is_empty() {
                        let items = visible_tree_items(
                            &self.categories,
                            &self.feeds,
                            &self.feed_editor.collapsed,
                        );
                        let is_cat_moving = self.editor_moving_category(&items);
                        let wrap_len = if is_cat_moving {
                            cats.len() + 1
                        } else {
                            cats.len()
                        };
                        if self.user_data.scroll_loop {
                            self.feed_editor.cat_cursor =
                                (self.feed_editor.cat_cursor + 1) % wrap_len;
                        } else {
                            self.feed_editor.cat_cursor =
                                (self.feed_editor.cat_cursor + 1).min(wrap_len - 1);
                        }
                    }
                } else {
                    let items = visible_tree_items(
                        &self.categories,
                        &self.feeds,
                        &self.feed_editor.collapsed,
                    );
                    if matches!(self.feed_editor.mode, FeedEditorMode::Moving { .. }) {
                        // During a feed move: navigate ALL items so the cursor can land on a
                        // category header (dropping there = insert as first child of that category).
                        if !items.is_empty() {
                            if self.user_data.scroll_loop {
                                self.feed_editor.cursor =
                                    (self.feed_editor.cursor + 1) % items.len();
                            } else {
                                self.feed_editor.cursor =
                                    (self.feed_editor.cursor + 1).min(items.len() - 1);
                            }
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
                                .position(|&i| i == self.feed_editor.cursor)
                                .unwrap_or(0);
                            let next_idx = if self.user_data.scroll_loop {
                                (cur + 1) % feed_indices.len()
                            } else {
                                (cur + 1).min(feed_indices.len() - 1)
                            };
                            self.feed_editor.cursor = feed_indices[next_idx];
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
            AppState::FeedList => self.move_sidebar_cursor(false),
            AppState::ArticleList => self.move_article_cursor(false),
            AppState::SettingsList
                if (self.user_data.scroll_loop
                    || self.settings_selected != SettingsItem::ImportOpml) =>
            {
                self.settings_selected = self.settings_selected.prev();
            }
            AppState::SavedCategoryList => self.move_saved_cursor(false),
            AppState::FeedEditor => {
                if self.feed_editor.panel == EditorPanel::Categories {
                    let cats = visible_cat_only_items(
                        &self.categories,
                        &self.feeds,
                        &self.feed_editor.collapsed,
                    );
                    if !cats.is_empty() {
                        let items = visible_tree_items(
                            &self.categories,
                            &self.feeds,
                            &self.feed_editor.collapsed,
                        );
                        let is_cat_moving = self.editor_moving_category(&items);
                        let wrap_len = if is_cat_moving {
                            cats.len() + 1
                        } else {
                            cats.len()
                        };
                        if self.user_data.scroll_loop {
                            self.feed_editor.cat_cursor = self
                                .feed_editor
                                .cat_cursor
                                .checked_sub(1)
                                .unwrap_or(wrap_len - 1);
                        } else {
                            self.feed_editor.cat_cursor =
                                self.feed_editor.cat_cursor.saturating_sub(1);
                        }
                    }
                } else {
                    let items = visible_tree_items(
                        &self.categories,
                        &self.feeds,
                        &self.feed_editor.collapsed,
                    );
                    if matches!(self.feed_editor.mode, FeedEditorMode::Moving { .. }) {
                        // During a feed move: navigate ALL items.
                        if !items.is_empty() {
                            if self.user_data.scroll_loop {
                                self.feed_editor.cursor = self
                                    .feed_editor
                                    .cursor
                                    .checked_sub(1)
                                    .unwrap_or(items.len() - 1);
                            } else {
                                self.feed_editor.cursor = self.feed_editor.cursor.saturating_sub(1);
                            }
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
                                .position(|&i| i == self.feed_editor.cursor)
                                .unwrap_or(0);
                            let prev_idx = if self.user_data.scroll_loop {
                                cur.checked_sub(1).unwrap_or(feed_indices.len() - 1)
                            } else {
                                cur.saturating_sub(1)
                            };
                            self.feed_editor.cursor = feed_indices[prev_idx];
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Move the sidebar (FeedList) cursor forward or backward one step.
    ///
    /// Rebuilds the visible tree, advances or retreats `sidebar_cursor` with wrap/clamp
    /// according to `scroll_loop`, then updates the article preview to match the new item.
    /// Returns early without resetting the tick if already at the boundary and not looping.
    fn move_sidebar_cursor(&mut self, forward: bool) {
        let items = sidebar_tree_items(&self.categories, &self.feeds, &self.sidebar_collapsed);
        if items.is_empty() {
            return;
        }
        let len = items.len();
        let loop_ = self.user_data.scroll_loop;
        self.sidebar_cursor = if forward {
            match cursor_next(self.sidebar_cursor, len, loop_) {
                Some(c) => c,
                None => return,
            }
        } else {
            match cursor_prev(self.sidebar_cursor, len, loop_) {
                Some(c) => c,
                None => return,
            }
        };
        self.sidebar_title_start_tick = self.tick;
        match items.get(self.sidebar_cursor) {
            Some(FeedTreeItem::AllFeeds) => self.populate_all_feeds_view(),
            Some(FeedTreeItem::Feed { feeds_idx, .. }) => {
                self.selected_feed = *feeds_idx;
                self.selected_article = 0;
                self.clear_category_view();
            }
            Some(FeedTreeItem::Category { id, .. }) => self.populate_category_view(*id),
            None => {}
        }
    }

    /// Move the article list cursor forward or backward one step.
    ///
    /// Determines the article count from whichever context is active (category, saved, or feed),
    /// then advances or retreats `selected_article`. Always resets the title animation tick,
    /// even when already at a boundary (clamp behaviour when not looping).
    pub(crate) fn move_article_cursor(&mut self, forward: bool) {
        let len = if self.in_category_context {
            self.category_view_articles.len()
        } else if self.in_saved_context {
            self.saved_view_articles.len()
        } else {
            self.feeds
                .get(self.selected_feed)
                .map_or(0, |f| f.articles.len())
        };
        if len == 0 {
            return;
        }
        let loop_ = self.user_data.scroll_loop;
        if forward {
            if loop_ {
                self.selected_article = (self.selected_article + 1) % len;
            } else {
                self.selected_article = (self.selected_article + 1).min(len - 1);
            }
        } else if loop_ {
            self.selected_article = self.selected_article.checked_sub(1).unwrap_or(len - 1);
        } else {
            self.selected_article = self.selected_article.saturating_sub(1);
        }
        self.article_title_start_tick = self.tick;
    }

    /// Move the saved-categories sidebar cursor forward or backward one step.
    ///
    /// Advances or retreats `saved_sidebar_cursor` with wrap/clamp according to `scroll_loop`,
    /// resets the sidebar title animation tick, and refreshes the saved article preview.
    /// Always resets the tick and calls `sync_saved_preview`, even at a boundary when clamping.
    fn move_saved_cursor(&mut self, forward: bool) {
        let count = 1 + self.user_data.saved_categories.len();
        let loop_ = self.user_data.scroll_loop;
        if forward {
            if loop_ {
                self.saved_sidebar_cursor = (self.saved_sidebar_cursor + 1) % count;
            } else {
                self.saved_sidebar_cursor = (self.saved_sidebar_cursor + 1).min(count - 1);
            }
        } else if loop_ {
            self.saved_sidebar_cursor = self
                .saved_sidebar_cursor
                .checked_sub(1)
                .unwrap_or(count - 1);
        } else {
            self.saved_sidebar_cursor = self.saved_sidebar_cursor.saturating_sub(1);
        }
        self.sidebar_title_start_tick = self.tick;
        self.sync_saved_preview();
    }

    /// Descend into the next layer of the navigation hierarchy.
    pub fn select(&mut self) {
        match self.state {
            AppState::FeedList => {
                let items =
                    sidebar_tree_items(&self.categories, &self.feeds, &self.sidebar_collapsed);
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
                    let content = if self.in_category_context {
                        let Some(&(fi, ai)) =
                            self.category_view_articles.get(self.selected_article)
                        else {
                            return;
                        };
                        let Some(feed) = self.feeds.get(fi) else {
                            return;
                        };
                        let Some(article) = feed.articles.get(ai) else {
                            return;
                        };
                        article.content.clone()
                    } else if self.in_saved_context {
                        let Some(saved) = self.saved_view_articles.get(self.selected_article)
                        else {
                            return;
                        };
                        saved.content.clone()
                    } else {
                        let Some(feed) = self.feeds.get(self.selected_feed) else {
                            return;
                        };
                        let Some(article) = feed.articles.get(self.selected_article) else {
                            return;
                        };
                        article.content.clone()
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
        sort_articles_by_date(&mut triples);
        self.category_view_articles = triples.into_iter().map(|(fi, ai, _)| (fi, ai)).collect();
        self.selected_article = self
            .selected_article
            .min(self.category_view_articles.len().saturating_sub(1));
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
        let mut triples: Vec<(usize, usize, Option<i64>)> = Vec::new();
        for (fi, feed) in self.feeds.iter().enumerate() {
            if feed.url == FAVORITES_URL {
                continue;
            }
            for ai in 0..feed.articles.len() {
                triples.push((fi, ai, feed.articles[ai].published_secs));
            }
        }
        sort_articles_by_date(&mut triples);
        self.category_view_articles = triples.into_iter().map(|(fi, ai, _)| (fi, ai)).collect();
        self.in_category_context = true;
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
    pub(super) fn persist_sidebar_collapsed(&mut self) {
        self.user_data.sidebar_collapsed = self.sidebar_collapsed.iter().copied().collect();
        let _ = save_user_data(&self.user_data);
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
        self.selected_article = self
            .selected_article
            .min(self.saved_view_articles.len().saturating_sub(1));
    }

    /// True when currently moving a category — used to skip feeds during navigation.
    fn editor_moving_category(&self, items: &[FeedTreeItem]) -> bool {
        if let FeedEditorMode::Moving {
            origin_render_idx, ..
        } = &self.feed_editor.mode
        {
            matches!(
                items.get(*origin_render_idx),
                Some(FeedTreeItem::Category { .. })
            )
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
                let items =
                    sidebar_tree_items(&self.categories, &self.feeds, &self.sidebar_collapsed);
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
            AppState::FeedEditor => {
                self.feed_editor.mode = FeedEditorMode::Normal;
                self.selected_tab = Tab::Feeds;
                self.state = AppState::FeedList;
            }
            AppState::FeedEditorRename => {
                self.feed_editor.input.clear();
                self.feed_editor.mode = FeedEditorMode::Normal;
                self.state = AppState::FeedEditor;
            }
            _ => {}
        }
    }

    // ── Feed editor data operations ──────────────────────────────────────────

    /// Clamp `editor_cursor` to the nearest Feed item in the full tree.
    pub(crate) fn clamp_editor_cursor_to_feed(&mut self) {
        let items = visible_tree_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);
        let feed_indices: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| matches!(item, FeedTreeItem::Feed { .. }))
            .map(|(i, _)| i)
            .collect();
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
        let items = visible_tree_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);
        if let Some(FeedTreeItem::Feed { feeds_idx, .. }) = items.get(self.feed_editor.cursor) {
            let idx = *feeds_idx;
            if idx > 0 {
                self.feeds.remove(idx);
                let _ = save_feeds(&self.feeds);
            }
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
        let children: Vec<CategoryId> = self
            .categories
            .iter()
            .filter(|c| c.parent_id == Some(cat_id))
            .map(|c| c.id)
            .collect();
        for child in children {
            self.delete_category_recursive(child);
        }
        self.feeds.retain(|f| f.category_id != Some(cat_id));
        self.categories.retain(|c| c.id != cat_id);
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
        let items = visible_tree_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);
        let src_id = match items.get(origin_full_idx) {
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
            let new_cats =
                visible_cat_only_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);
            return new_cats.iter().position(
                |item| matches!(item, FeedTreeItem::Category { id, .. } if *id == src_id),
            );
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

        let new_cats =
            visible_cat_only_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);
        new_cats
            .iter()
            .position(|item| matches!(item, FeedTreeItem::Category { id, .. } if *id == src_id))
    }

    /// Apply the pending feed move and return the new render-index of the moved feed.
    pub(crate) fn apply_feed_move(&mut self, origin: usize) -> Option<usize> {
        let dest = self.feed_editor.cursor;

        let items = visible_tree_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);
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

        let new_items =
            visible_tree_items(&self.categories, &self.feeds, &self.feed_editor.collapsed);
        new_items.iter().position(
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

        let moved_pos = self
            .categories
            .iter()
            .position(|c| c.id == moved_id)
            .unwrap();
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

/// Like [`visible_tree_items`] but prepends the virtual `AllFeeds` entry at index 0.
///
/// Use this for the main sidebar; use [`visible_tree_items`] for the feed editor and other
/// contexts where `AllFeeds` should not appear.
pub fn sidebar_tree_items(
    categories: &[Category],
    feeds: &[Feed],
    collapsed: &HashSet<CategoryId>,
) -> Vec<FeedTreeItem> {
    let mut items = visible_tree_items_filtered(categories, feeds, collapsed, None);
    items.insert(0, FeedTreeItem::AllFeeds);
    items
}

/// Collects feed indices for all feeds directly or transitively under `cat_id`,
/// in tree order (sorted by order field within each level).
pub(crate) fn feeds_in_category(
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

/// Like [`visible_tree_items`] but restricts feed rows to those whose index is in `feed_filter`.
/// Categories with no visible descendants under the filter are also omitted.
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

/// Recursively append one depth level of the category/feed tree to `result`.
///
/// Children of a collapsed category are skipped. Feed rows respect `feed_filter` when present.
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
        app.switch_tab_right();
        assert_eq!(app.selected_tab, Tab::Changelog);
        assert_eq!(app.state, AppState::Changelog);
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
