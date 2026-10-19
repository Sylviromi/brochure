//! State for the FreshRSS server setup wizard.

use crate::models::{FreshRssStep, TextInput};

/// All mutable state for the FreshRSS setup flow (server → username → API password).
pub struct FreshRssSetupState {
    /// Which step the wizard is on.
    pub step: FreshRssStep,
    /// Server address input.
    pub server_input: TextInput,
    /// Username input.
    pub username_input: TextInput,
    /// API password input (rendered masked).
    pub password_input: TextInput,
    /// Username of the connection being edited, or `None` when connecting fresh.
    /// While editing, a blank password keeps the one already stored for this user.
    pub editing_username: Option<String>,
}

impl Default for FreshRssSetupState {
    fn default() -> Self {
        Self {
            step: FreshRssStep::Server,
            server_input: TextInput::default(),
            username_input: TextInput::default(),
            password_input: TextInput::default(),
            editing_username: None,
        }
    }
}

impl FreshRssSetupState {
    /// Prefill server/username from an existing config (if any) and reset to step 0.
    /// Passing a `username` puts the wizard in edit mode.
    pub fn open(&mut self, base_url: Option<&str>, username: Option<&str>) {
        self.editing_username = username.map(str::to_string);
        self.step = FreshRssStep::Server;
        self.server_input.text = base_url.unwrap_or("https://").to_string();
        self.server_input.cursor = self.server_input.text.chars().count();
        self.username_input.text = username.unwrap_or("").to_string();
        self.username_input.cursor = self.username_input.text.chars().count();
        self.password_input.clear();
    }

    /// Clear the password buffer (call right after spawning the login task).
    pub fn clear_secret(&mut self) {
        self.password_input.clear();
    }
}
