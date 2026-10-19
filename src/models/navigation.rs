//! Application navigation states, tabs, and editor modes.

use super::CategoryId;

/// Application navigation states.
#[derive(PartialEq, Clone, Debug)]
pub enum AppState {
    /// Viewing the feed list in the Feeds tab.
    FeedList,
    /// Viewing the list of articles in the selected feed.
    ArticleList,
    /// Viewing the full content of a selected article.
    ArticleDetail,
    /// Entering a URL to add a new feed.
    AddFeed,
    /// Viewing the settings menu.
    SettingsList,
    /// Browsing categories in the Saved tab sidebar.
    SavedCategoryList,
    /// Prompting for OPML export file path.
    ExportPath,
    /// Prompting for OPML import file path.
    ImportPath,
    /// Confirmation dialog to clear all data.
    ClearData,
    /// Confirmation dialog to clear cached article content.
    ClearArticleCache,
    /// Full-screen feed editor (rearranging and organizing feeds/categories).
    FeedEditor,
    /// Inline rename input inside the feed editor.
    FeedEditorRename,
    /// Modal for saving an article to a category (or unsaving).
    CategoryPicker,
    /// Full-screen saved-category manager (from Settings).
    SavedCategoryEditor,
    /// Inline rename input inside the saved-category editor.
    SavedCategoryEditorRename,
    /// Confirmation dialog before deleting a saved category.
    SavedCategoryEditorDeleteConfirm,
    /// Text-input state for creating a new saved category in the editor.
    SavedCategoryEditorNew,
    /// Viewing the changelog and about screen.
    Changelog,
    /// Full-screen theme editor (browse all themes, manage custom ones).
    ThemeEditor,
    /// Clone-from picker popup when creating a new custom theme.
    ThemeEditorNew,
    /// Full-screen color-slot editor for a custom theme.
    ThemeEditorColorEdit,
    /// Inline hex-value text input within the color-slot editor.
    ThemeEditorHexInput,
    /// Text input for renaming a custom theme.
    ThemeEditorRename,
    /// Text input for the export file path.
    ThemeEditorExport,
    /// Text input for the import file path.
    ThemeEditorImport,
    /// Multi-step FreshRSS server setup (server → username → API password).
    FreshRssSetup,
}

/// Which tab is active in the tab bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Feeds,
    Saved,
    Settings,
    /// The changelog and about screen.
    Changelog,
}

impl Tab {
    /// Cycle to the next tab (wraps around).
    pub fn next(self) -> Self {
        match self {
            Self::Feeds => Self::Saved,
            Self::Saved => Self::Settings,
            Self::Settings => Self::Changelog,
            Self::Changelog => Self::Feeds,
        }
    }

    /// Cycle to the previous tab (wraps around).
    pub fn prev(self) -> Self {
        match self {
            Self::Feeds => Self::Changelog,
            Self::Saved => Self::Feeds,
            Self::Settings => Self::Saved,
            Self::Changelog => Self::Settings,
        }
    }
}

/// Which item is selected in the Settings menu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SettingsItem {
    /// Import feeds from an OPML file.
    ImportOpml,
    /// Export feeds to an OPML file.
    ExportOpml,
    /// Import saved articles from a starred JSON export.
    ImportSaved,
    /// Export saved articles as starred JSON.
    ExportSaved,
    /// Clear all user data and feeds.
    ClearData,
    /// Toggle whether to save full article content when fetching.
    CacheFullArticles,
    /// Clear cached article content and fetch fresh on demand.
    ClearArticleCache,
    /// Toggle automatic feed fetching on app startup.
    AutoFetchOnStart,
    /// Cycle archive policy for how long archived articles are kept.
    ArchivePolicy,
    /// Toggle whether list navigation wraps around at the top/bottom.
    ScrollLoop,
    /// Toggle rounded UI borders.
    BorderStyle,
    /// Cycle body text alignment for reading.
    BodyAlignment,
    /// Cycle zen mode content width (30–90%, no wrap).
    ZenWidth,
    /// Open the theme picker popup.
    Theme,
    /// Connect a FreshRSS server (Enter: sync now / open setup; Left: disconnect).
    FreshRss,
}

impl SettingsItem {
    /// Short user-facing description for this setting.
    pub fn description(self) -> &'static str {
        match self {
            Self::ImportOpml => "Import feeds from an OPML file on disk",
            Self::ExportOpml => "Export your feed list to an OPML file",
            Self::ImportSaved => {
                "Import saved articles from a starred-items export (FreshRSS, Inoreader, …)"
            }
            Self::ExportSaved => {
                "Export saved articles as starred-items JSON that other RSS readers can import"
            }
            Self::ClearData => "Remove all feeds, categories, and preferences",
            Self::CacheFullArticles => {
                "Save full article content to cache during fetch instead of just the description"
            }
            Self::ClearArticleCache => "Delete cached article content and read history",
            Self::AutoFetchOnStart => "When and how often to check for new articles",
            Self::ArchivePolicy => "How long to keep articles after they leave the feed",
            Self::ScrollLoop => "Wrap around at the top and bottom of lists",
            Self::BorderStyle => "Use rounded or straight border corners",
            Self::BodyAlignment => {
                "Body text alignment for reading (Left / Center / Right / Justify)"
            }
            Self::ZenWidth => "Width of the zen reading mode as a percentage of the terminal",
            Self::Theme => "Browse built-in themes and create custom ones",
            Self::FreshRss => "Connect a FreshRSS server to sync subscriptions and read state",
        }
    }

    /// Every item in on-screen order (top to bottom).
    pub const ALL: [Self; 15] = [
        Self::ImportOpml,
        Self::ExportOpml,
        Self::ImportSaved,
        Self::ExportSaved,
        Self::ClearData,
        Self::CacheFullArticles,
        Self::ClearArticleCache,
        Self::AutoFetchOnStart,
        Self::ArchivePolicy,
        Self::FreshRss,
        Self::ScrollLoop,
        Self::BorderStyle,
        Self::BodyAlignment,
        Self::ZenWidth,
        Self::Theme,
    ];

    fn position(self) -> usize {
        Self::ALL.iter().position(|&i| i == self).unwrap_or(0)
    }

    /// Move to the next settings item (wraps around).
    pub fn next(self) -> Self {
        Self::ALL[(self.position() + 1) % Self::ALL.len()]
    }

    /// Move to the previous settings item (wraps around).
    pub fn prev(self) -> Self {
        Self::ALL[(self.position() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Identifies where an article lives in the app, used by the full-article fetch event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FeedSource {
    /// A regular feed at the given index in app.feeds.
    Feed(usize),
    /// The article was opened from the Saved articles view.
    Saved,
}

/// Which panel has focus in the feed editor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditorPanel {
    /// Left panel: category manager (add/rename/delete categories).
    Categories,
    /// Right panel: combined feed+category tree (move feeds/categories).
    Feeds,
}

/// Which step the FreshRSS setup wizard is on.
#[derive(Debug, Clone, PartialEq)]
pub enum FreshRssStep {
    /// User is typing the server address.
    Server,
    /// User is typing the FreshRSS username.
    Username,
    /// User is typing the API password (masked).
    Password,
}

/// Which step the multi-step AddFeed flow is on.
#[derive(Debug, Clone, PartialEq)]
pub enum AddFeedStep {
    /// User is typing the feed URL.
    Url,
    /// User is confirming or editing the auto-detected feed title.
    Title,
}

/// Interaction mode inside the FeedEditor screen.
#[derive(Debug, Clone, PartialEq)]
pub enum FeedEditorMode {
    /// Standard browsing/selection mode.
    Normal,
    /// Item at this render-list index is being dragged.
    Moving {
        /// Row of the moved item in the editor tree (`visible_tree_items`).
        origin_render_idx: usize,
        /// Cursor position before entering move mode — restored on Esc.
        original_cursor: usize,
        /// Depth offset relative to cursor's depth (categories only).
        depth_delta: i8,
    },
    /// Renaming the item at this render-list index.
    Renaming {
        /// Index of the item being renamed.
        render_idx: usize,
    },
    /// Typing a name for a new category. None = root level, Some = subcategory.
    NewCategory {
        /// Parent category ID (None for root level).
        parent_id: Option<CategoryId>,
    },
    /// Editing the URL of the feed at this render-list index.
    EditingUrl {
        /// Index of the feed item being edited.
        render_idx: usize,
    },
}
