//! Domain models: core types (Feed, Article), navigation states, events, and helper structs.

mod core_types;
mod events;
pub mod feed;
mod navigation;
pub mod scroll;
pub(crate) mod text_input;
pub(crate) mod theme;
mod tree;

pub use core_types::*;
pub use events::*;
pub use navigation::*;
pub use scroll::{ListScroll, TextScroll};
pub(crate) use text_input::TextInput;
pub use theme::{CustomTheme, CustomThemeColors, ThemeEditorState};
pub use tree::{Category, CategoryId, FeedTreeItem};

// ── Constants ─────────────────────────────────────────────────────────────────

/// URL used to identify the virtual Favorites feed (never persisted).
pub const FAVORITES_URL: &str = "internal:favorites";

/// Articles shorter than this are considered stubs and trigger a readability fetch.
pub const CONTENT_STUB_MAX_LEN: usize = 500;
