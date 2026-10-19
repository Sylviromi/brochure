//! State for the import/export file-path input screens.

use crate::models::TextInput;

/// What the import/export path prompt reads or writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileFormat {
    /// Feed list as OPML.
    #[default]
    Opml,
    /// Saved articles as Google Reader-style starred JSON.
    Saved,
}

impl FileFormat {
    /// What the file holds, for prompt titles and status messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::Opml => "OPML",
            Self::Saved => "Saved Articles",
        }
    }

    /// Suggested file name for exports.
    pub fn default_file_name(self) -> &'static str {
        match self {
            Self::Opml => "export.opml",
            Self::Saved => "brochure-starred.json",
        }
    }
}

/// Mutable state for the `ImportPath` and `ExportPath` input screens.
#[derive(Default)]
pub struct FilePromptState {
    /// File path typed by the user.
    pub path_input: TextInput,
    /// Which kind of file is being imported or exported.
    pub format: FileFormat,
}
