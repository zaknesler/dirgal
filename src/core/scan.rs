use crate::{
    core::{image::ImageEntry, pipeline},
    error::AppResult,
};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ImageScanner {
    pub roots: Vec<PathBuf>,
    pub thumb_dir: PathBuf,
    pub images: Vec<ImageEntry>,
    pub bookmarks: Vec<u64>,
}

impl ImageScanner {
    /// Scan the given roots and build an image scanner with the initial image list
    pub fn scan(roots: Vec<PathBuf>, thumb_dir: PathBuf) -> AppResult<Self> {
        let discovered = Self::discover(&roots, &thumb_dir)?;
        Ok(Self {
            roots,
            thumb_dir,
            images: discovered.images,
            bookmarks: discovered.bookmarks,
        })
    }

    /// Re-scan the current roots and replace the image list
    pub fn rescan(&mut self) -> AppResult<()> {
        let discovered = Self::discover(&self.roots, &self.thumb_dir)?;
        self.images = discovered.images;
        self.bookmarks = discovered.bookmarks;
        Ok(())
    }

    /// Generate thumbnails for any images that don't have one yet
    pub fn generate_thumbnails(&self) -> AppResult<()> {
        pipeline::generate_thumbnails(&self.images)
    }

    /// Collect files and conver to image entries
    fn discover(
        roots: &[PathBuf],
        thumb_dir: &std::path::Path,
    ) -> AppResult<pipeline::DiscoveredImages> {
        let files = pipeline::collect_files(roots)?;
        pipeline::build_image_entries(files, thumb_dir)
    }
}
