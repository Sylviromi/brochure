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
    /// Whether this article has been saved to a category. Runtime flag; not persisted on Article.
    #[serde(default)]
    pub is_saved: bool,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub image_url: Option<String>,
    /// Name of the feed this article was fetched from (set at fetch time).
    #[serde(default)]
    pub source_feed: String,
    /// Unix timestamp (seconds) of when the article was published.
    #[serde(default)]
    pub published_secs: Option<i64>,
}

fn default_true() -> bool {
    true
}

/// A user-defined category for saved articles.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedCategory {
    pub id: u32,
    pub name: String,
}

/// An article saved by the user into a named category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedArticle {
    pub article: Article,
    pub category_id: u32,
}

/// User-specific persistent data (read/starred state).
#[derive(Serialize, Deserialize, Default)]
pub struct UserData {
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
    #[serde(default)]
    pub save_article_content: bool,
    #[serde(default)]
    pub border_rounded: bool,
    #[serde(default = "default_true")]
    pub eager_article_fetch: bool,
    #[serde(default = "default_true")]
    pub auto_fetch_on_start: bool,
}
