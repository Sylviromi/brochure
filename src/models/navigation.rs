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
    OPMLExportPath,
    /// Prompting for OPML import file path.
    OPMLImportPath,
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
}

/// Which tab is active in the tab bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Feeds,
    Saved,
    Settings,
}

impl Tab {
    /// Cycle to the next tab (wraps around).
    pub fn next(self) -> Self {
        match self {
            Self::Feeds => Self::Saved,
            Self::Saved => Self::Settings,
            Self::Settings => Self::Feeds,
        }
    }

    /// Cycle to the previous tab (wraps around).
    pub fn prev(self) -> Self {
        match self {
            Self::Feeds => Self::Settings,
            Self::Saved => Self::Feeds,
            Self::Settings => Self::Saved,
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
    /// Clear all user data and feeds.
    ClearData,
    /// Toggle whether to save full article content when fetching.
    SaveArticleContent,
    /// Clear cached article content and fetch fresh on demand.
    ClearArticleCache,
    /// Toggle eager fetching of full article content.
    EagerArticleFetch,
    /// Toggle automatic feed fetching on app startup.
    AutoFetchOnStart,
    /// Cycle archive policy for how long archived articles are kept.
    ArchivePolicy,
    /// Toggle whether list navigation wraps around at the top/bottom.
    ScrollLoop,
    /// Toggle rounded UI borders.
    BorderStyle,
}

impl SettingsItem {
    /// Move to the next settings item (wraps around).
    pub fn next(self) -> Self {
        match self {
            Self::ImportOpml => Self::ExportOpml,
            Self::ExportOpml => Self::ClearData,
            Self::ClearData => Self::SaveArticleContent,
            Self::SaveArticleContent => Self::ClearArticleCache,
            Self::ClearArticleCache => Self::EagerArticleFetch,
            Self::EagerArticleFetch => Self::AutoFetchOnStart,
            Self::AutoFetchOnStart => Self::ArchivePolicy,
            Self::ArchivePolicy => Self::ScrollLoop,
            Self::ScrollLoop => Self::BorderStyle,
            Self::BorderStyle => Self::ImportOpml,
        }
    }

    /// Move to the previous settings item (wraps around).
    pub fn prev(self) -> Self {
        match self {
            Self::ImportOpml => Self::BorderStyle,
            Self::ExportOpml => Self::ImportOpml,
            Self::ClearData => Self::ExportOpml,
            Self::SaveArticleContent => Self::ClearData,
            Self::ClearArticleCache => Self::SaveArticleContent,
            Self::EagerArticleFetch => Self::ClearArticleCache,
            Self::AutoFetchOnStart => Self::EagerArticleFetch,
            Self::ArchivePolicy => Self::AutoFetchOnStart,
            Self::ScrollLoop => Self::ArchivePolicy,
            Self::BorderStyle => Self::ScrollLoop,
        }
    }
}

/// Which panel has focus in the feed editor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditorPanel {
    /// Left panel: category manager (add/rename/delete categories).
    Categories,
    /// Right panel: combined feed+category tree (move feeds/categories).
    Feeds,
}

/// Which step the multi-step AddFeed flow is on.
#[derive(Debug, Clone, PartialEq)]
pub enum AddFeedStep {
    /// User is typing the feed URL.
    Url,
    /// User is confirming or editing the auto-detected feed title.
    Title,
}

/// Identifies where an article was opened from, used by FullArticleFetched.
#[derive(Debug)]
pub enum FeedSource {
    /// A regular feed at the given index in app.feeds.
    Feed(usize),
    /// The article was opened from the Saved articles view.
    Saved,
}

/// Interaction mode inside the FeedEditor screen.
#[derive(Debug, Clone, PartialEq)]
pub enum FeedEditorMode {
    /// Standard browsing/selection mode.
    Normal,
    /// Item at this render-list index is being dragged.
    Moving {
        /// Index in the effective tree (all-collapsed for category moves).
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
