use super::CategoryId;

/// Application navigation states.
#[derive(PartialEq, Clone, Debug)]
pub enum AppState {
    FeedList,
    ArticleList,
    ArticleDetail,
    AddFeed,
    SettingsList,
    /// Browsing categories in the Saved tab sidebar.
    SavedCategoryList,
    OPMLExportPath,
    OPMLImportPath,
    ClearData,
    ClearArticleCache,
    FeedEditor,
    FeedEditorRename,
    /// Modal for saving an article to a category (or unsaving).
    CategoryPicker,
    /// Full-screen saved-category manager (from Settings).
    SavedCategoryEditor,
    /// Inline rename input inside the saved-category editor.
    SavedCategoryEditorRename,
}

/// Which tab is active in the tab bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Feeds,
    Saved,
    Settings,
}

impl Tab {
    pub fn next(self) -> Self {
        match self {
            Self::Feeds => Self::Saved,
            Self::Saved => Self::Settings,
            Self::Settings => Self::Feeds,
        }
    }

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
    ImportOpml,
    ExportOpml,
    ClearData,
    SaveArticleContent,
    ClearArticleCache,
    EagerArticleFetch,
    AutoFetchOnStart,
    BorderStyle,
}

impl SettingsItem {
    pub fn next(self) -> Self {
        match self {
            Self::ImportOpml => Self::ExportOpml,
            Self::ExportOpml => Self::ClearData,
            Self::ClearData => Self::SaveArticleContent,
            Self::SaveArticleContent => Self::ClearArticleCache,
            Self::ClearArticleCache => Self::EagerArticleFetch,
            Self::EagerArticleFetch => Self::AutoFetchOnStart,
            Self::AutoFetchOnStart => Self::BorderStyle,
            Self::BorderStyle => Self::ImportOpml,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::ImportOpml => Self::BorderStyle,
            Self::ExportOpml => Self::ImportOpml,
            Self::ClearData => Self::ExportOpml,
            Self::SaveArticleContent => Self::ClearData,
            Self::ClearArticleCache => Self::SaveArticleContent,
            Self::EagerArticleFetch => Self::ClearArticleCache,
            Self::AutoFetchOnStart => Self::EagerArticleFetch,
            Self::BorderStyle => Self::AutoFetchOnStart,
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
    /// User is confirming/editing the feed title.
    Title,
}

/// Identifies where an article was opened from, used by FullArticleFetched.
#[derive(Debug)]
pub enum FeedSource {
    /// A regular feed at the given index in app.feeds.
    Feed(usize),
    /// The article was opened from the Saved view.
    Saved,
}

/// Interaction mode inside the FeedEditor screen.
#[derive(Debug, Clone, PartialEq)]
pub enum FeedEditorMode {
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
        render_idx: usize,
    },
    /// Typing a name for a new category. None = root level, Some = subcategory.
    NewCategory {
        parent_id: Option<CategoryId>,
    },
}
