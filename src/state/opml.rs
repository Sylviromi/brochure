//! State for the OPML import/export file-path input screens.

use crate::models::TextInput;

/// Mutable state for OPMLImportPath and OPMLExportPath input screens.
#[derive(Default)]
pub struct OpmlState {
    /// File path typed by the user.
    pub path_input: TextInput,
}
