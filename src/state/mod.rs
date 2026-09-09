//! Sub-state structs extracted from `App` — one per feature area.
//! Each owns its own text input buffers and cursors.

pub mod add_feed;
pub mod category_picker;
pub mod feed_editor;
pub mod freshrss;
pub mod opml;

pub use add_feed::AddFeedState;
pub use category_picker::CategoryPickerState;
pub use feed_editor::FeedEditorState;
pub use freshrss::FreshRssSetupState;
pub use opml::OpmlState;
