//! State for the multi-step "Add Feed" wizard.

use crate::models::{AddFeedStep, AppState, CategoryId, TextInput};

/// All mutable state for the AddFeed wizard flow.
pub struct AddFeedState {
    /// Which step the wizard is on.
    pub step: AddFeedStep,
    /// The URL entered in step 0 (carried into step 1 for saving).
    pub url: String,
    /// Title fetched from the feed URL in the background (placeholder shown in step 1).
    pub fetched_title: Option<String>,
    /// Where to return after the wizard completes (SettingsList or FeedEditor).
    pub return_state: AppState,
    /// Category to place the new feed in (set from cursor when adding via FeedEditor).
    pub target_category: Option<CategoryId>,
    /// Order value to insert the new feed at (None = append at end).
    pub target_order: Option<usize>,
    /// Text buffer for URL input (step 0) and title input (step 1).
    pub url_input: TextInput,
}

impl Default for AddFeedState {
    fn default() -> Self {
        Self {
            step: AddFeedStep::Url,
            url: String::new(),
            fetched_title: None,
            return_state: AppState::SettingsList,
            target_category: None,
            target_order: None,
            url_input: TextInput::default(),
        }
    }
}
