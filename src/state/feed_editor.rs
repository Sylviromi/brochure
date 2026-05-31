//! State for the feed editor and saved-category editor screens.

use crate::models::{CategoryId, EditorPanel, FeedEditorMode, TextInput};
use std::collections::HashSet;

/// All mutable state for the feed/category editor and saved-category editor.
pub struct FeedEditorState {
    /// Cursor into the flattened visible-tree list inside the feed editor.
    pub cursor: usize,
    /// Categories collapsed in the feed editor view.
    pub collapsed: HashSet<CategoryId>,
    /// Current interaction mode in the feed editor.
    pub mode: FeedEditorMode,
    /// Text buffer for rename / new-category input in the feed editor and saved-category editor.
    pub input: TextInput,
    /// Which panel has focus in the split editor (Categories = left, Feeds = right).
    pub panel: EditorPanel,
    /// Cursor in the left (categories-only) panel of the editor.
    pub cat_cursor: usize,
    /// Pending category delete: (id, total_feeds_to_delete). Set on [d], cleared on Esc or after confirm.
    pub delete_cat: Option<(CategoryId, usize)>,
}

impl Default for FeedEditorState {
    fn default() -> Self {
        Self {
            cursor: 0,
            collapsed: HashSet::new(),
            mode: FeedEditorMode::Normal,
            input: TextInput::default(),
            panel: EditorPanel::Feeds,
            cat_cursor: 0,
            delete_cat: None,
        }
    }
}
