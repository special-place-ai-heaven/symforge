//! The `working_directory` reroute report shared by the MCP edit tools and the
//! embedded edit route.

use std::path::Path;

/// The suffix an edit appends when the caller supplied `working_directory`:
/// whether the edit was rerouted, where it wrote, and the indexed copy's path.
/// Empty when no `working_directory` was supplied.
pub(crate) fn format_reroute_suffix(
    working_directory: Option<&Path>,
    rerouted: bool,
    target_path: &Path,
    indexed_path: &Path,
) -> String {
    let Some(working_directory) = working_directory else {
        return String::new();
    };
    format!(
        "\nworking_directory: {}\nrerouted: {}\nwrote_to: {}\nindexed_path: {}",
        working_directory.display(),
        rerouted,
        target_path.display(),
        indexed_path.display(),
    )
}
