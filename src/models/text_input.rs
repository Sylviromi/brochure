//! Shared text-input state with cursor tracking.

use crossterm::event::KeyCode;

/// Bundles a text buffer with a cursor position for keyboard text entry.
///
/// Replaces the duplicated `input: String` + `input_cursor: usize` pattern
/// across editable-state types.
#[derive(Clone, Default)]
pub struct TextInput {
    pub text: String,
    pub cursor: usize,
}

impl TextInput {
    /// Clear the text buffer and reset the cursor to 0.
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Handle a key event for cursor-aware text input.
    ///
    /// Supports Left/Right movement, Backspace (delete before cursor),
    /// and Char insertion at cursor. Pass `max_len: Some(n)` to cap
    /// input length (e.g. `Some(6)` for hex colors); `None` for unlimited.
    pub fn handle_key(&mut self, key: KeyCode, max_len: Option<usize>) {
        match key {
            KeyCode::Left if self.cursor > 0 => {
                self.cursor -= 1;
            }
            KeyCode::Right if self.cursor < self.text.chars().count() => {
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                let byte_idx = self
                    .text
                    .char_indices()
                    .nth(self.cursor - 1)
                    .map(|(i, _)| i)
                    .unwrap_or(self.text.len());
                self.text.remove(byte_idx);
                self.cursor -= 1;
            }
            KeyCode::Char(c) if max_len.is_none_or(|max| self.text.chars().count() < max) => {
                let byte_idx = self
                    .text
                    .char_indices()
                    .nth(self.cursor)
                    .map(|(i, _)| i)
                    .unwrap_or(self.text.len());
                self.text.insert(byte_idx, c);
                self.cursor += 1;
            }
            _ => {}
        }
    }
}
