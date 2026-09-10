use crate::{
    core::{
        image::{ContentHash, FoundFile, ImageEntry, ImageId},
        pipeline,
        store::Store,
    },
    error::AppResult,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanSource {
    /// Restored from last scan cache
    Cache,
    /// Scanned directly from the filesystem
    Disk,
}

#[derive(Debug, Clone)]
pub struct ImageScanner {
    pub roots: Vec<PathBuf>,
    pub thumb_dir: PathBuf,
    pub images: Vec<ImageEntry>,
    pub bookmarks: Vec<u64>,
    pub source: ScanSource,
}

impl ImageScanner {
    /// Build an image scanner from the last complete listings for all requested roots
    pub fn from_cache(roots: Vec<PathBuf>, thumb_dir: PathBuf) -> Option<Self> {
        let store = Store::load();
        let mut cached = store.cached_images(&roots)?;
        cached.sort_by(|a, b| crate::core::path::compare_paths(&a.0, &b.0));

        let images = cached
            .into_iter()
            .map(|(path, cached)| {
                let found = FoundFile {
                    path,
                    bytes: cached.size,
                    modified: cached.modified_time(),
                    created: cached.created_time(),
                };
                ImageEntry::new(
                    found,
                    &thumb_dir,
                    ContentHash(cached.hash),
                    cached.dimensions,
                )
            })
            .collect();

        Some(Self {
            roots,
            thumb_dir,
            images,
            bookmarks: store.bookmarks,
            source: ScanSource::Cache,
        })
    }

    /// Scan the given roots and build an image scanner with the initial image list
    pub fn scan(roots: Vec<PathBuf>, thumb_dir: PathBuf) -> AppResult<Self> {
        let discovered = Self::discover(&roots, &thumb_dir)?;
        Ok(Self {
            roots,
            thumb_dir,
            images: discovered.images,
            bookmarks: discovered.bookmarks,
            source: ScanSource::Disk,
        })
    }

    /// Re-scan the current roots and replace the image list
    pub fn rescan(&mut self) -> AppResult<()> {
        let discovered = Self::discover(&self.roots, &self.thumb_dir)?;
        self.images = discovered.images;
        self.bookmarks = discovered.bookmarks;
        self.source = ScanSource::Disk;
        Ok(())
    }

    /// Generate thumbnails for any images that don't have one yet
    pub fn generate_thumbnails(&self) -> AppResult<()> {
        pipeline::generate_thumbnails(&self.images)
    }

    /// Remove images at the given paths and return their content hashes
    pub fn remove_paths(&mut self, paths: &[PathBuf]) -> HashSet<ImageId> {
        let paths: HashSet<&Path> = paths.iter().map(PathBuf::as_path).collect();
        let mut removed_ids = HashSet::new();

        self.images.retain(|image| {
            if paths.contains(image.id.path()) {
                removed_ids.insert(image.id.clone());
                false
            } else {
                true
            }
        });

        removed_ids
    }

    /// Collect files and conver to image entries
    fn discover(
        roots: &[PathBuf],
        thumb_dir: &std::path::Path,
    ) -> AppResult<pipeline::DiscoveredImages> {
        let collected = pipeline::collect_files(roots)?;
        pipeline::build_image_entries(collected, thumb_dir)
    }
}
