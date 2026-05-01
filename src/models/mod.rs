use serde::{Deserialize, Serialize};

mod core_types;
mod events;
mod navigation;
pub mod feed;
pub mod scroll;

pub use core_types::*;
pub use events::*;
pub use navigation::*;
pub use scroll::{ListScroll, TextScroll};

// ── Constants ─────────────────────────────────────────────────────────────────

/// URL used to identify the virtual Favorites feed (never persisted).
pub const FAVORITES_URL: &str = "internal:favorites";

/// Articles shorter than this are considered stubs and trigger a readability fetch.
pub const CONTENT_STUB_MAX_LEN: usize = 500;

// ── Category tree ─────────────────────────────────────────────────────────────

/// Stable identifier for a category node.
pub type CategoryId = u64;

/// A category node in the feed tree (persisted to categories.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Category {
    pub id: CategoryId,
    pub name: String,
    /// None = root level. Some(id) = nested under that category.
    #[serde(default)]
    pub parent_id: Option<CategoryId>,
    /// Display order among siblings (lower = first).
    #[serde(default)]
    pub order: usize,
}

/// One visible row in the flattened category/feed tree.
#[derive(Debug, Clone)]
pub enum FeedTreeItem {
    Category {
        id: CategoryId,
        depth: u8,
        collapsed: bool,
    },
    Feed {
        feeds_idx: usize,
        depth: u8,
    },
}

