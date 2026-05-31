//! Theme editor UI state: cursors, input buffers, and editing context.

use crate::models::TextInput;

/// State for the theme editor: cursors and editing context.
#[derive(Default)]
pub struct ThemeEditorState {
    /// Cursor in the theme editor list (builtins first, then custom themes).
    pub cursor: usize,
    /// Cursor in the color-slot list when editing a custom theme.
    pub color_cursor: usize,
    /// Cursor in the clone-from picker when creating a new custom theme.
    pub clone_cursor: usize,
    /// ID of the custom theme currently being edited or renamed.
    pub editing_id: Option<u32>,
    /// Text buffer for theme name (rename) and file path (export/import).
    pub path_input: TextInput,
    /// Text buffer for hex color entry.
    pub hex_input: TextInput,
}
