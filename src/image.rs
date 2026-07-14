use crate::{
    error::AppResult,
    hash::{hash_content, hash_path},
};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

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
    path: PathBuf,
    bytes: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
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

/// Collect all image files in the given roots
pub fn collect_images(roots: &[PathBuf], thumb_dir: &Path) -> Vec<ImageEntry> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut found: Vec<FoundFile> = Vec::new();

    for root in roots {
        if !root.is_dir() {
            tracing::warn!(root = %root.display(), "not a directory, skipping");
            continue;
        }

        for entry in walk_images(root) {
            if seen.insert(entry.path().to_path_buf()) {
                let (bytes, modified, created) = entry_stats(&entry);
                found.push(FoundFile {
                    path: entry.into_path(),
                    bytes,
                    modified,
                    created,
                });
            }
        }
    }

    found.sort_by(|a, b| crate::path::compare_paths(&a.path, &b.path));

    found
        .into_iter()
        .map(|f| ImageEntry::new(f, thumb_dir))
        .collect()
}

/// Walk the given root directory recursively and collect all image files
fn walk_images(root: &Path) -> Vec<ignore::DirEntry> {
    WalkBuilder::new(root)
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()) && is_image(e.path()))
        .collect()
}

/// Check whether the given path is an image file
fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| Img::extensions().contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Get the stats of the given entry (size, modified, created)
fn entry_stats(entry: &ignore::DirEntry) -> (u64, Option<SystemTime>, Option<SystemTime>) {
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
