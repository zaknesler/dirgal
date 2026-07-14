use gpui::SharedString;
use std::path::{Path, PathBuf};

/// Find the deepest root that contains the given path
pub fn matching_root<'a>(roots: &'a [PathBuf], path: &Path) -> Option<&'a PathBuf> {
    roots
        .iter()
        .filter(|r| path.starts_with(r))
        .max_by_key(|r| r.as_os_str().len())
}

/// Get the label for the given path, relative to the deepest root that contains it
pub fn label_for(roots: &[PathBuf], path: &Path) -> SharedString {
    let rel = match matching_root(roots, path) {
        Some(root) => path.strip_prefix(root).unwrap_or(path),
        None => path,
    };

    rel.to_string_lossy().into_owned().into()
}

/// Split out the path segments from the given parent path
pub fn group_segments(roots: &[PathBuf], parent: &Path) -> Vec<SharedString> {
    let Some(root) = matching_root(roots, parent) else {
        return parent
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into())
            .collect();
    };

    let rel = parent.strip_prefix(root).unwrap_or(parent);
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string());

    let mut segments = Vec::new();
    if roots.len() > 1 {
        segments.push(root_name.into());
    }
    segments.extend(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy().into()),
    );

    if segments.is_empty() {
        segments.push("(root)".into());
    }

    segments
}

/// Compare two paths using natural ordering
pub fn compare_paths(a: &Path, b: &Path) -> std::cmp::Ordering {
    natord::compare(&a.to_string_lossy(), &b.to_string_lossy())
}

/// Compare by parent directory first, then full path, so files sharing a
/// directory sort contiguously (flat sort interleaves parent/child dirs)
pub fn compare_paths_grouped(a: &Path, b: &Path) -> std::cmp::Ordering {
    let parent_a = a.parent().unwrap_or(Path::new(""));
    let parent_b = b.parent().unwrap_or(Path::new(""));
    compare_paths(parent_a, parent_b).then_with(|| compare_paths(a, b))
}
