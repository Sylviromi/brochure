use serde::{Deserialize, Serialize};

mod core_types;
mod events;
mod navigation;
pub mod feed;

pub use core_types::*;
pub use events::*;
pub use navigation::*;

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
        /// 0 = sibling after cursor, +1 = child of cursor, -1 = sibling of cursor's parent.
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
