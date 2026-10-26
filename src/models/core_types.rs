//! Core domain types: Feed, Article, and user-persistent data structures.

use super::CategoryId;
use crate::models::theme::custom_theme::CustomTheme;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Where a feed's articles come from.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum FeedKind {
    /// Plain RSS/Atom URL fetched directly (existing behaviour).
    #[default]
    Local,
    /// Subscribed via a FreshRSS server (GReader API), synced side-by-side.
    FreshRss,
}

/// Connection settings for a single FreshRSS server.
///
/// The API password / auth token is never stored here — see `sync::secret`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreshRssConfig {
    /// Normalised API base, e.g. `https://rss.example.com/api/greader.php`.
    pub base_url: String,
    /// FreshRSS username.
    pub username: String,
    /// Unix seconds of the last successful sync, if any.
    #[serde(default)]
    pub last_sync_secs: Option<i64>,
    /// Sidebar category that holds everything synced from this server.
    /// `None` until the first sync creates (or adopts) it.
    #[serde(default)]
    pub category_id: Option<CategoryId>,
}

/// A single RSS/Atom feed source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feed {
    /// Display name of the feed.
    pub title: String,
    /// URL to the feed's RSS/Atom document.
    pub url: String,
    /// Which category this feed belongs to. None = root / uncategorized.
    #[serde(default)]
    pub category_id: Option<CategoryId>,
    /// Display order among siblings (lower = first).
    #[serde(default)]
    pub order: usize,
    /// Where this feed syncs from. Defaults to `Local` for old JSON.
    #[serde(default)]
    pub kind: FeedKind,
    /// FreshRSS server feed id (e.g. `feed/12`). Only set for `FreshRss` feeds.
    #[serde(default)]
    pub remote_id: Option<String>,
    /// Count of unread articles in this feed (runtime, not persisted).
    #[serde(skip, default)]
    pub unread_count: usize,
    /// Articles fetched from this feed (runtime, not persisted).
    #[serde(skip, default)]
    pub articles: Vec<Article>,
    /// Whether this feed has been fetched at least once (runtime, not persisted).
    #[serde(skip, default)]
    pub fetched: bool,
    /// Last fetch error message, if any.
    #[serde(skip, default)]
    pub fetch_error: Option<String>,
    /// Unix timestamp (seconds) from the feed's own `<updated>` / `<lastBuildDate>` field.
    #[serde(skip)]
    pub feed_updated_secs: Option<i64>,
    /// Unix timestamp (seconds) of our last successful fetch of this feed.
    #[serde(default)]
    pub last_fetched_secs: Option<i64>,
}

/// A single article entry from a feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    /// Article title.
    pub title: String,
    /// Brief description or summary from the feed (not persisted if content is saved).
    pub description: String,
    /// URL to the original article.
    pub link: String,
    /// Whether the user has marked this article as read.
    pub is_read: bool,
    /// Whether this article has been saved to a category. Runtime flag; not persisted on Article.
    #[serde(default)]
    pub is_saved: bool,
    /// Full article text (from RSS/Atom content or saved content field).
    #[serde(default)]
    pub content: String,
    /// Image URLs from MediaRSS attachments (media:content, media:thumbnail).
    /// Images embedded in article HTML are handled by html2md → markdown.
    #[serde(default)]
    pub images: Vec<String>,
    /// Name of the feed this article was fetched from (set at fetch time).
    #[serde(default)]
    pub source_feed: String,
    /// FreshRSS item id (e.g. `tag:google.com,2005:reader/item/...`). Only set for synced articles.
    #[serde(default)]
    pub remote_id: Option<String>,
    /// Unix timestamp (seconds) of when the article was published.
    #[serde(default)]
    pub published_secs: Option<i64>,
    /// Whether this article has been archived (was absent from the feed's latest fetch).
    /// Archived articles are shown in a separate section in the article list.
    #[serde(default)]
    pub is_archived: bool,
    /// Error message from the last failed full-content fetch.
    /// Rendered with centered/muted/italic styling. Never persisted to disk.
    #[serde(default, skip_serializing)]
    pub error: Option<String>,
}

/// Default serde function that returns `true`.
fn default_true() -> bool {
    true
}

/// Default serde function that returns `"catppuccin-mocha"`.
fn default_theme() -> String {
    String::from("catppuccin-mocha")
}

fn default_zen_width() -> u8 {
    66
}

/// Controls how long articles are kept after they disappear from a feed's latest fetch.
///
/// Articles not in the newest fetch are marked archived. Archived articles older than
/// the selected threshold are deleted. `Forever` keeps them indefinitely.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum ArchivePolicy {
    TwoDays,
    #[default]
    OneWeek,
    OneMonth,
    ThreeMonths,
    Forever,
}

impl ArchivePolicy {
    /// Returns the threshold duration in seconds, or `None` for `Forever`.
    pub fn threshold_secs(&self) -> Option<i64> {
        match self {
            ArchivePolicy::TwoDays => Some(2 * 24 * 3600),
            ArchivePolicy::OneWeek => Some(7 * 24 * 3600),
            ArchivePolicy::OneMonth => Some(30 * 24 * 3600),
            ArchivePolicy::ThreeMonths => Some(90 * 24 * 3600),
            ArchivePolicy::Forever => None,
        }
    }

    /// Human-readable label shown in the settings UI.
    pub fn label(&self) -> &'static str {
        match self {
            ArchivePolicy::TwoDays => "2 days",
            ArchivePolicy::OneWeek => "1 week",
            ArchivePolicy::OneMonth => "1 month",
            ArchivePolicy::ThreeMonths => "3 months",
            ArchivePolicy::Forever => "Forever",
        }
    }

    /// Returns the next policy in the cycle (wraps around).
    pub fn next(&self) -> ArchivePolicy {
        match self {
            ArchivePolicy::TwoDays => ArchivePolicy::OneWeek,
            ArchivePolicy::OneWeek => ArchivePolicy::OneMonth,
            ArchivePolicy::OneMonth => ArchivePolicy::ThreeMonths,
            ArchivePolicy::ThreeMonths => ArchivePolicy::Forever,
            ArchivePolicy::Forever => ArchivePolicy::TwoDays,
        }
    }

    /// Returns the previous policy in the cycle (wraps around).
    pub fn prev(&self) -> ArchivePolicy {
        match self {
            ArchivePolicy::TwoDays => ArchivePolicy::Forever,
            ArchivePolicy::OneWeek => ArchivePolicy::TwoDays,
            ArchivePolicy::OneMonth => ArchivePolicy::OneWeek,
            ArchivePolicy::ThreeMonths => ArchivePolicy::OneMonth,
            ArchivePolicy::Forever => ArchivePolicy::ThreeMonths,
        }
    }
}

/// Controls when the app automatically fetches feeds.
///
/// `OnStart`, `EveryHour`, and `EveryDay` all fetch on launch; interval-based
/// fetching is reserved for future implementation. `Never` skips all automatic fetching.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum FetchPolicy {
    #[default]
    OnStart,
    EveryHour,
    EveryDay,
    Never,
}

impl FetchPolicy {
    /// Human-readable label shown in the settings UI.
    pub fn label(&self) -> &'static str {
        match self {
            FetchPolicy::OnStart => "On Start",
            FetchPolicy::EveryHour => "Every Hour",
            FetchPolicy::EveryDay => "Every Day",
            FetchPolicy::Never => "Never",
        }
    }

    /// Returns the next policy in the cycle (wraps around).
    pub fn next(&self) -> FetchPolicy {
        match self {
            FetchPolicy::OnStart => FetchPolicy::EveryHour,
            FetchPolicy::EveryHour => FetchPolicy::EveryDay,
            FetchPolicy::EveryDay => FetchPolicy::Never,
            FetchPolicy::Never => FetchPolicy::OnStart,
        }
    }

    /// Returns the previous policy in the cycle (wraps around).
    pub fn prev(&self) -> FetchPolicy {
        match self {
            FetchPolicy::OnStart => FetchPolicy::Never,
            FetchPolicy::EveryHour => FetchPolicy::OnStart,
            FetchPolicy::EveryDay => FetchPolicy::EveryHour,
            FetchPolicy::Never => FetchPolicy::EveryDay,
        }
    }
}

/// A user-defined category for saved articles.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedCategory {
    /// Unique identifier for this category.
    pub id: u32,
    /// Display name of the category.
    pub name: String,
}

/// An article saved by the user into a named category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedArticle {
    /// The article that was saved.
    pub article: Article,
    /// ID of the category this article was saved to.
    pub category_id: u32,
}

/// A single release entry fetched from GitHub releases.
#[derive(Debug, Clone)]
pub struct ReleaseNote {
    /// The version string for this release (e.g. `"0.4.0"`).
    pub version: String,
    /// Human-readable release date (YYYY-MM-DD), empty if unavailable.
    pub date: String,
    /// Bullet-point highlights parsed from the release body, empty if unavailable.
    pub highlights: Vec<String>,
}

/// Information about available updates fetched from crates.io and GitHub releases.
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    /// All releases newer than the running version, ordered newest first.
    pub releases: Vec<ReleaseNote>,
}

/// User-specific persistent data (read/starred state).
///
/// Every field has a serde default, and [`Default`] is derived from those, so a
/// fresh install and a missing field in an old file always agree.
#[derive(Serialize, Deserialize)]
pub struct UserData {
    /// Set of article links marked as read by the user.
    #[serde(default)]
    pub read_links: HashSet<String>,
    /// Articles saved by the user into named categories.
    #[serde(default)]
    pub saved_articles: Vec<SavedArticle>,
    /// User-defined categories for organizing saved articles.
    #[serde(default)]
    pub saved_categories: Vec<SavedCategory>,
    /// Legacy field: populated when reading old user_data.json. Migrated on load, never re-written.
    #[serde(default, skip_serializing)]
    pub starred_articles: Vec<Article>,
    /// Whether to save full article content when fetching (vs. description only).
    #[serde(default)]
    pub cache_full_articles: bool,
    /// Whether to use rounded borders in the UI.
    #[serde(default)]
    pub border_rounded: bool,
    /// Legacy migration field: reads `auto_fetch_on_start` from old JSON but is never re-written.
    /// When `false`, the value is migrated to `FetchPolicy::Never` in `load_user_data`.
    #[serde(
        default = "default_true",
        skip_serializing,
        rename = "auto_fetch_on_start"
    )]
    pub legacy_auto_fetch_on_start: bool,
    /// Controls when the app automatically fetches feeds.
    #[serde(default)]
    pub fetch_policy: FetchPolicy,
    /// Policy for how long archived articles are kept before deletion.
    #[serde(default)]
    pub archive_policy: ArchivePolicy,
    /// Whether list navigation wraps around at the top/bottom (scrollbar loop).
    #[serde(default = "default_true")]
    pub scroll_loop: bool,
    /// Which category ids are collapsed in the sidebar (persisted).
    #[serde(default)]
    pub sidebar_collapsed: Vec<u64>,
    /// Body text alignment for article reading (left/center/right/justify).
    #[serde(default)]
    pub body_alignment: String,
    /// Width of the zen mode content area as a percentage (30–90, non-wrapping, default 66).
    #[serde(default = "default_zen_width")]
    pub zen_width: u8,
    /// Indices of collapsed changelog entries (reversed display order, persisted).
    #[serde(default)]
    pub changelog_collapsed: Vec<usize>,
    /// Slug of the active theme (e.g. `"catppuccin-mocha"`, `"gruvbox-dark"`, `"custom"`).
    #[serde(default = "default_theme")]
    pub selected_theme: String,
    /// ID of the active custom theme when `selected_theme == "custom"`.
    #[serde(default)]
    pub selected_custom_id: Option<u32>,
    /// All user-created custom themes.
    #[serde(default)]
    pub custom_themes: Vec<CustomTheme>,
    /// Legacy: inline TOML of a single custom theme. Migrated on load; never re-written.
    #[serde(default, skip_serializing)]
    pub custom_theme: Option<String>,
    /// Legacy: path to a custom `.toml` theme file. Migrated on load; never re-written.
    #[serde(default, skip_serializing)]
    pub custom_theme_path: Option<String>,
    /// FreshRSS server connection (no secrets — see `sync::secret`). None = local-only mode.
    #[serde(default)]
    pub freshrss: Option<FreshRssConfig>,
    /// Toast notification preferences.
    #[serde(default)]
    pub toasts: super::ToastSettings,
}

impl Default for UserData {
    fn default() -> Self {
        serde_json::from_str("{}").expect("every UserData field has a serde default")
    }
}

impl UserData {
    /// Id of the saved category called `name` (ignoring case), creating it when
    /// missing. Returns `(id, created)`.
    pub fn saved_category_for(&mut self, name: &str) -> (u32, bool) {
        if let Some(c) = self
            .saved_categories
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
        {
            return (c.id, false);
        }
        let id = self
            .saved_categories
            .iter()
            .map(|c| c.id)
            .max()
            .unwrap_or(0)
            + 1;
        self.saved_categories.push(SavedCategory {
            id,
            name: name.to_string(),
        });
        (id, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_default_matches_serde_defaults() {
        let data = UserData::default();
        assert_eq!(data.fetch_policy, FetchPolicy::OnStart);
        assert!(
            data.legacy_auto_fetch_on_start,
            "must not trigger the Never migration"
        );
        assert!(data.scroll_loop);
        assert_eq!(data.zen_width, 66);
        assert_eq!(data.selected_theme, "catppuccin-mocha");
    }
}
