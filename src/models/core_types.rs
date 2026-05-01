use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::CategoryId;

/// A single RSS/Atom feed source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feed {
    pub title: String,
    pub url: String,
    /// Which category this feed belongs to. None = root / uncategorized.
    #[serde(default)]
    pub category_id: Option<CategoryId>,
    /// Display order among siblings (lower = first).
    #[serde(default)]
    pub order: usize,
    #[serde(skip, default)]
    pub unread_count: usize,
    #[serde(skip, default)]
    pub articles: Vec<Article>,
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
    pub title: String,
    pub description: String,
    pub link: String,
    pub is_read: bool,
    pub is_starred: bool,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub image_url: Option<String>,
    /// Name of the feed this article was fetched from (set at fetch time).
    #[serde(default)]
    pub source_feed: String,
}

fn default_true() -> bool {
    true
}

/// User-specific persistent data (read/starred state).
#[derive(Serialize, Deserialize, Default)]
pub struct UserData {
    pub read_links: HashSet<String>,
    pub starred_articles: Vec<Article>,
    /// When true, full article content is saved to disk (offline mode).
    #[serde(default)]
    pub save_article_content: bool,
    /// When true, all TUI borders use rounded corners.
    #[serde(default)]
    pub border_rounded: bool,
    /// When true (default), article content is fetched eagerly during feed refresh.
    /// When false, content is fetched lazily when the user opens an article.
    #[serde(default = "default_true")]
    pub eager_article_fetch: bool,
    /// When true (default), all feeds are fetched automatically on startup.
    #[serde(default = "default_true")]
    pub auto_fetch_on_start: bool,
}
