use gpui::SharedString;
use std::path::{Path, PathBuf};

/// Expand all root directories for the given paths
pub fn get_roots(paths: Option<Vec<String>>) -> Vec<PathBuf> {
    paths
        .unwrap_or_else(|| vec![".".to_string()])
        .into_iter()
        .map(|path| {
            let path = PathBuf::from(path);
            std::fs::canonicalize(&path).unwrap_or(path)
        })
        .collect()
}

/// Get the thumbnail cache directory for the current OS
pub fn get_thumbnail_dir() -> PathBuf {
    let thumb_dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(env!("CARGO_PKG_NAME"))
        .join("thumbnails");

    if let Err(e) = std::fs::create_dir_all(&thumb_dir) {
        tracing::warn!(dir = %thumb_dir.display(), error = %e, "could not create thumbnail cache, using local directory");
        return PathBuf::from("thumbnails");
    }

    thumb_dir
}

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

/// Extract the first valid `YYYY-MM-DD` date in a path
pub fn extract_date_from_path(path: &Path) -> Option<(u32, u32, u32)> {
    static DATE_RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"(\d{4})-(\d{2})-(\d{2})").unwrap());

    let text = path.to_string_lossy();

    // Find first valid date of all likely candidates
    DATE_RE.captures_iter(&text).find_map(|caps| {
        let year: u32 = caps[1].parse().ok()?;
        let month: u32 = caps[2].parse().ok()?;
        let day: u32 = caps[3].parse().ok()?;

        let valid =
            (1970..=2069).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day);

        valid.then_some((year, month, day))
    })
}
