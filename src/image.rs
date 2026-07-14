use crate::{
    error::AppResult,
    hash::{hash_content, hash_path},
    ui::model::{ImageHash, Sort, SortKey},
};
use std::{
    cmp::Ordering,
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use gpui::Img;
use humansize::{BINARY, FormatSizeOptions, format_size};
use ignore::WalkBuilder;

pub const THUMB_PX: u32 = 336;

/// Number of bytes a source image must be to not warrant a thumbnail (32 KB)
pub const SMALL_FILE_BYTES: u64 = 32 * 1024;

#[derive(Debug, Clone)]
pub struct ImageEntry {
    pub hash: u64,
    pub bytes: u64,
    #[allow(unused)]
    pub modified: Option<SystemTime>,
    #[allow(unused)]
    pub created: Option<SystemTime>,
    pub src_path: Arc<Path>,
    pub thumb_path: Arc<Path>,
    pub thumb_exists: bool,
}

pub struct FoundFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
}

impl ImageEntry {
    pub fn new(file: FoundFile, thumb_dir: &Path) -> Self {
        let hash = hash_content(&file.path).unwrap_or_else(|e| {
            tracing::warn!(path = %file.path.display(), error = %e, "hash_content failed, falling back to hash_path");
            hash_path(&file.path)
        });
        let thumb = thumb_dir.join(format!("{:016x}.png", hash));
        let thumb_exists = thumb.exists();

        Self {
            hash,
            bytes: file.bytes,
            modified: file.modified,
            created: file.created,
            src_path: Arc::from(file.path),
            thumb_path: Arc::from(thumb),
            thumb_exists,
        }
    }

    /// Generate and save the thumbnail in the thumbnail directory
    pub fn generate_thumbnail(&self) -> AppResult<()> {
        let src = &self.src_path;
        let dst = &self.thumb_path;

        if dst.exists() {
            return Ok(());
        }

        let image = image::ImageReader::open(src)?
            .with_guessed_format()?
            .decode()?;

        let image_already_small = image.width() <= THUMB_PX && image.height() <= THUMB_PX;
        let thumb = if image_already_small {
            image
        } else {
            image.thumbnail(THUMB_PX, THUMB_PX)
        };

        let tmp = dst.with_extension("tmp");
        thumb.save_with_format(&tmp, image::ImageFormat::Png)?;
        fs::rename(&tmp, dst)?;
        Ok(())
    }
}

/// Construct a walker that will recursively walk the given root directory
pub fn build_root_walker(root: &Path) -> ignore::Walk {
    WalkBuilder::new(root).build()
}

/// Check whether the given path is an image file
pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| Img::extensions().contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Get the stats of the given entry (size, modified, created)
pub fn entry_stats(entry: &ignore::DirEntry) -> (u64, Option<SystemTime>, Option<SystemTime>) {
    entry
        .metadata()
        .ok()
        .map(|m| (m.len(), m.modified().ok(), m.created().ok()))
        .unwrap_or((0, None, None))
}

/// Format the given number of bytes as a human-readable string
pub fn format_bytes(bytes: u64) -> String {
    format_size(bytes, FormatSizeOptions::from(BINARY).decimal_places(1))
}

/// Deduplicate by content hash keeping the last, then sort by the active sort key
pub fn deduplicate_and_sort(images: Vec<ImageEntry>, sort: Sort) -> Vec<ImageEntry> {
    let mut seen = HashSet::new();
    let mut images: Vec<ImageEntry> = images
        .into_iter()
        .rev()
        .filter(|e| seen.insert(e.hash))
        .collect();

    images.sort_by(|a, b| compare_key(a, b, sort));
    images
}

/// Compare by parent directory alone so same directory images stay contiguous
pub fn compare_parents(a: &ImageEntry, b: &ImageEntry) -> Ordering {
    let parent_a = a.src_path.parent().unwrap_or(Path::new(""));
    let parent_b = b.src_path.parent().unwrap_or(Path::new(""));
    crate::path::compare_paths(parent_a, parent_b)
}

/// Compare two images by the active sort key falling back to path for a stable order
pub fn compare_key(a: &ImageEntry, b: &ImageEntry, sort: Sort) -> Ordering {
    let ord = match sort.key {
        SortKey::Name => crate::path::compare_paths(&a.src_path, &b.src_path),
        SortKey::Modified => a.modified.cmp(&b.modified),
        SortKey::Created => a.created.cmp(&b.created),
        SortKey::Size => a.bytes.cmp(&b.bytes),
    }
    .then_with(|| crate::path::compare_paths(&a.src_path, &b.src_path));

    if sort.ascending { ord } else { ord.reverse() }
}

/// Resolve configured bookmark hashes against loaded images, dropping unknowns
pub fn resolve_bookmarks(hashes: &[u64], images: &[ImageEntry]) -> Vec<ImageHash> {
    let known = hashes.iter().copied().collect::<HashSet<u64>>();

    images
        .iter()
        .filter(|e| known.contains(&e.hash))
        .map(|e| ImageHash(e.hash))
        .collect()
}

/// Whether grouping would produce anything beyond a single fake "(root)" group
pub fn compute_groupable(images: &[ImageEntry], roots: &[PathBuf]) -> bool {
    let mut parents: HashSet<&Path> = HashSet::new();
    for entry in images {
        parents.insert(entry.src_path.parent().unwrap_or(Path::new("")));
    }

    let single_root = roots.len() == 1;
    let single_parent = parents.len() == 1;
    let parent_is_root = parents.contains(roots[0].as_path());

    !single_root || !single_parent || !parent_is_root
}
