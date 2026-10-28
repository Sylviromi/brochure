//! Sub-state structs extracted from `App` — one per feature area.
//! Each owns its own text input buffers and cursors.

pub mod add_feed;
pub mod category_picker;
pub mod feed_editor;
pub mod file_prompt;
pub mod freshrss;
pub mod link_hints;

pub use add_feed::AddFeedState;
pub use category_picker::CategoryPickerState;
pub use feed_editor::{EditorDrag, FeedEditorState};
pub use file_prompt::FilePromptState;
pub use freshrss::FreshRssSetupState;
pub use link_hints::LinkHints;
