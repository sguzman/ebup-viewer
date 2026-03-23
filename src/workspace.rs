use std::path::{Path, PathBuf};
use tracing::trace;

/// Finds the workspace root relative to the current working directory.
///
/// It walks up the parent chain until it finds a directory containing either
/// `Cargo.lock` or `.git`. Falls back to the current working directory if no
/// marker is found.
pub fn workspace_root_from_cwd() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    if let Some(root) = find_workspace_root(&cwd) {
        trace!(cwd = ?cwd, root = ?root, "Resolved workspace root via repository marker");
        return Some(root);
    }
    trace!(
        cwd = ?cwd,
        "No repository marker found, using current directory as workspace root"
    );
    Some(cwd)
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if is_workspace_root_marker(dir) {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

fn is_workspace_root_marker(dir: &Path) -> bool {
    dir.join("Cargo.lock").is_file() || dir.join(".git").is_dir()
}
