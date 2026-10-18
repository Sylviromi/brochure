//! Central application state.
//!
//! Owns the [`App`] struct (passed to every draw frame and input handler). Behaviour is
//! split by concern:
//! - `navigation` — tab switching, cursor movement, drilling in/out of views
//! - `editor` — feed editor cursor and move/delete operations
//! - `tree` — flattening categories/feeds into visible rows
//! - `freshrss` — FreshRSS refresh, sync, and read/star push

mod editor;
mod freshrss;
mod navigation;
mod tree;

pub(crate) use tree::feed_row_indices;
pub(crate) use tree::feeds_in_category;
pub use tree::{sidebar_tree_items, visible_cat_only_items, visible_tree_items};

use crate::models::Category;
use crate::models::Feed;
use crate::models::{
    AppState, Article, CategoryId, FeedTreeItem, HitAreas, ListScroll, SettingsItem, Tab,
    TextScroll, ThemeEditorState, UpdateInfo, UserData,
};
use crate::state::{
    AddFeedState, CategoryPickerState, FeedEditorState, FreshRssSetupState, OpmlState,
};
use crate::storage::{article_cache_size, load_categories, load_feeds, load_user_data};
use crate::ui::theme::ColorTheme;
use limner::Alignment;
use limner::render_image::SlicedProtocol;
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
    pub changelog_collapsed: HashSet<usize>,
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
    pub protocol_cache: HashMap<String, SlicedProtocol>,
    /// Image picker (terminal protocol detection) — initialized in [`run()`].
    pub picker: Option<limner::render_image::Picker>,
    /// Image metadata from the latest markdown render.
    pub article_images: Vec<limner::ImageInfo>,
    /// Cached markdown render of the article on screen (see `ArticleRender`).
    pub article_render: Option<crate::ui::content::ArticleRender>,
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

    // ── FreshRSS sync ────────────────────────────────────────────────────
    /// Cached GReader `Auth` token (runtime only, never persisted).
    pub freshrss_auth: Option<String>,
    /// State for the FreshRSS server setup wizard.
    pub freshrss_setup: FreshRssSetupState,

    // ── Mouse ────────────────────────────────────────────────────────────
    /// Clickable regions recorded during the last draw.
    pub hit: HitAreas,
}

/// Move a cursor one step forward or backward within `len` rows, wrapping or
/// clamping per `wrap`. Returns `None` for an empty list.
pub(crate) fn step_cursor(cursor: usize, len: usize, forward: bool, wrap: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match (forward, wrap) {
        (true, true) => (cursor + 1) % len,
        (true, false) => (cursor + 1).min(len - 1),
        (false, true) => cursor.checked_sub(1).unwrap_or(len - 1),
        (false, false) => cursor.saturating_sub(1),
    })
}

/// Advance a cursor index, wrapping or clamping based on `wrap`.
pub(crate) fn cursor_next(cursor: usize, len: usize, wrap: bool) -> Option<usize> {
    step_cursor(cursor, len, true, wrap)
}

/// Retreat a cursor index, wrapping or clamping based on `wrap`.
pub(crate) fn cursor_prev(cursor: usize, len: usize, wrap: bool) -> Option<usize> {
    step_cursor(cursor, len, false, wrap)
}

/// Ordered list of all valid zen mode width percentages.
const ZEN_WIDTHS: &[u8] = &[30, 35, 40, 45, 50, 55, 60, 65, 66, 70, 75, 80, 85, 90];

const ALIGNMENTS: &[Alignment] = &[
    Alignment::Left,
    Alignment::Center,
    Alignment::Right,
    Alignment::Justify,
];

impl App {
    /// Construct a fresh `App`, loading feeds, categories, and user data from disk.
    pub fn new() -> Self {
        let user_data = load_user_data();
        let theme = resolve_theme(&user_data);
        let feeds = load_feeds();
        let categories = load_categories();

        // Editor cursor and initial selected_feed both start on the first visible feed.
        let initial_editor_items = visible_tree_items(&categories, &feeds, &HashSet::new());
        let initial_editor_cursor = feed_row_indices(&initial_editor_items)
            .first()
            .copied()
            .unwrap_or(0);
        let selected_feed = match initial_editor_items.get(initial_editor_cursor) {
            Some(FeedTreeItem::Feed { feeds_idx, .. }) => *feeds_idx,
            _ => 0,
        };

        // Load persisted sidebar and changelog collapse state before moving user_data.
        let initial_collapsed: HashSet<CategoryId> =
            user_data.sidebar_collapsed.iter().copied().collect();
        let initial_changelog_collapsed: HashSet<usize> =
            user_data.changelog_collapsed.iter().copied().collect();

        // Parse body alignment before user_data is moved into Self.
        let initial_body_alignment = {
            match user_data.body_alignment.to_lowercase().as_str() {
                "center" => Alignment::Center,
                "right" => Alignment::Right,
                "justify" => Alignment::Justify,
                _ => Alignment::Left,
            }
        };

        // Clamp zen width to valid range: 30–90, default 66.
        let initial_zen_width = user_data.zen_width.clamp(30, 90);

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
            article_render: None,
            article_content_area: Rect::default(),
            article_scroll_offset: 0,
            body_alignment: initial_body_alignment,
            zen_width: initial_zen_width,
            sidebar_title_start_tick: 0,
            article_title_start_tick: 0,
            theme,
            theme_editor: ThemeEditorState::default(),
            freshrss_auth: None,
            freshrss_setup: FreshRssSetupState::default(),
            hit: HitAreas::default(),
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
        if let Some(pos) = ALIGNMENTS.iter().position(|&a| a == self.body_alignment) {
            self.body_alignment = ALIGNMENTS[(pos + 1) % ALIGNMENTS.len()];
        }
    }

    /// Cycle body alignment to the previous variant.
    pub fn cycle_alignment_prev(&mut self) {
        if let Some(pos) = ALIGNMENTS.iter().position(|&a| a == self.body_alignment) {
            self.body_alignment = ALIGNMENTS[(pos + ALIGNMENTS.len() - 1) % ALIGNMENTS.len()];
        }
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
        let pos = ZEN_WIDTHS
            .iter()
            .position(|&w| w == self.zen_width)
            .unwrap_or(ZEN_WIDTHS.len() - 1);
        self.zen_width = ZEN_WIDTHS[(pos + 1).min(ZEN_WIDTHS.len() - 1)];
    }

    /// Cycle zen width to the previous value (steps of 5%, stops at 30).
    pub fn cycle_zen_width_prev(&mut self) {
        let pos = ZEN_WIDTHS
            .iter()
            .position(|&w| w == self.zen_width)
            .unwrap_or(0);
        self.zen_width = ZEN_WIDTHS[pos.saturating_sub(1)];
    }
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
    fn step_cursor_wraps_and_clamps() {
        assert_eq!(step_cursor(2, 3, true, true), Some(0));
        assert_eq!(step_cursor(2, 3, true, false), Some(2));
        assert_eq!(step_cursor(0, 3, false, true), Some(2));
        assert_eq!(step_cursor(0, 3, false, false), Some(0));
        assert_eq!(step_cursor(0, 0, true, true), None);
    }
}
