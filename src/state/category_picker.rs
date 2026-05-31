//! State for the CategoryPicker modal (save-to-category flow).

use crate::models::{AppState, TextInput};

/// All mutable state for the CategoryPicker modal.
pub struct CategoryPickerState {
    /// Cursor row in the picker list (0..categories + 2).
    pub cursor: usize,
    /// True when the "New category..." entry is active and user is typing.
    pub new_mode: bool,
    /// Text buffer for the new-category name input inside the picker.
    pub input: TextInput,
    /// State to return to when the picker closes (ArticleList or ArticleDetail).
    pub return_state: AppState,
}

impl Default for CategoryPickerState {
    fn default() -> Self {
        Self {
            cursor: 0,
            new_mode: false,
            input: TextInput::default(),
            return_state: AppState::ArticleList,
        }
    }
}
