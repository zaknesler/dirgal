use crate::hash::hash_path_mtime;
use std::collections::HashSet;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use gpui::Img;
use humansize::{BINARY, FormatSizeOptions, format_size};
use ignore::WalkBuilder;

pub const THUMB_PX: u32 = 336;

/// Number of bytes a source image must be to not warrant a thumbnail
pub const SMALL_FILE_BYTES: u64 = 32 * 1024;

#[derive(Clone)]
pub struct ImageEntry {
    pub hash: u64,
    pub bytes: u64,
    pub src_path: Arc<Path>,
    pub thumb_path: Arc<Path>,
}

pub struct FoundFile {
    path: PathBuf,
    bytes: u64,
    mtime: u64,
}

impl ImageEntry {
    pub fn new(file: FoundFile, thumb_dir: &Path) -> Self {
        let hash = hash_path_mtime(&file.path, file.mtime);
        let thumb = thumb_dir.join(format!("{:016x}.png", hash));

        Self {
            hash,
            src_path: Arc::from(file.path),
            bytes: file.bytes,
            thumb_path: Arc::from(thumb),
        }
    }

    // #[tracing::instrument
    pub async fn generate_thumbnail(&self) -> Result<(), String> {
        let src = &self.src_path;
        let dst = &self.thumb_path;

        if dst.exists() {
            return Ok(());
        }

        let image = image::open(src).map_err(|e| {
            tracing::warn!(src = %src.display(), error = %e, "failed to open image");
            e.to_string()
        })?;

        let thumb = if image.width() <= THUMB_PX && image.height() <= THUMB_PX {
            image
        } else {
            image.thumbnail(THUMB_PX, THUMB_PX)
        };

        let tmp = dst.with_extension("tmp");

        thumb
            .save_with_format(&tmp, image::ImageFormat::Png)
            .map_err(|e| {
                tracing::warn!(dst = %dst.display(), error = %e, "failed to save thumbnail");
                e.to_string()
            })?;

        fs::rename(&tmp, dst).map_err(|e| {
            tracing::warn!(dst = %dst.display(), error = %e, "failed to rename thumbnail into place");
            e.to_string()
        })
    }
}

#[tracing::instrument(skip(roots, thumb_dir), fields(roots = roots.len()))]
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
                let (bytes, mtime) = entry_stats(&entry);
                found.push(FoundFile {
                    path: entry.into_path(),
                    bytes,
                    mtime,
                });
            }
        }
    }

    found.sort_by(|a, b| {
        (a.path.parent(), a.path.file_name()).cmp(&(b.path.parent(), b.path.file_name()))
    });

    tracing::debug!(count = found.len(), "found image files");

    found
        .into_iter()
        .map(|f| ImageEntry::new(f, thumb_dir))
        .collect()
}

fn walk_images(root: &Path) -> Vec<ignore::DirEntry> {
    WalkBuilder::new(root)
        .build()
        .flatten()
        .filter(|e| e.file_type().is_some_and(|ft| ft.is_file()) && is_image(e.path()))
        .collect()
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| Img::extensions().contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn entry_stats(entry: &ignore::DirEntry) -> (u64, u64) {
    entry
        .metadata()
        .ok()
        .map(|m| (m.len(), mtime_nanos(&m)))
        .unwrap_or((0, 0))
}

fn mtime_nanos(metadata: &Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

pub fn format_bytes(bytes: u64) -> String {
    format_size(bytes, FormatSizeOptions::from(BINARY).decimal_places(1))
}
