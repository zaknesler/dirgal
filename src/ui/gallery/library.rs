use crate::core::image::ImageEntry;
use crate::ui::{model::*, state};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Library {
    pub roots: Vec<PathBuf>,
    pub images: Vec<ImageEntry>,
    pub image_index: HashMap<ImageHash, usize>,
    pub duplicates: Vec<ImageEntry>,
    pub duplicate_index: HashMap<ImageHash, usize>,
    pub bookmarks: Vec<ImageHash>,
}

impl Library {
    /// Empty library used before the initial state snapshot has been loaded
    pub fn empty() -> Self {
        Self {
            roots: Vec::new(),
            images: Vec::new(),
            image_index: HashMap::new(),
            duplicates: Vec::new(),
            duplicate_index: HashMap::new(),
            bookmarks: Vec::new(),
        }
    }

    /// Create a new library
    pub fn from_state(state: state::AppState, sort: Sort) -> Self {
        let (images, duplicates) =
            crate::core::image::deduplicate_and_sort(state.scanner.images, sort);

        let image_index = images
            .iter()
            .enumerate()
            .map(|(i, e)| (ImageHash(e.hash), i))
            .collect();

        let duplicate_index = duplicates
            .iter()
            .enumerate()
            .map(|(i, e)| (ImageHash(e.hash), i))
            .collect();

        let bookmarks = crate::core::image::resolve_bookmarks(&state.config.bookmarks, &images);

        Self {
            roots: state.scanner.roots,
            images,
            image_index,
            duplicates,
            duplicate_index,
            bookmarks,
        }
    }

    /// Re-sort images in place and rebuild the index and bookmarks to match the new order
    pub fn resort(&mut self, sort: Sort, bookmarks: &[u64]) {
        self.images
            .sort_by(|a, b| crate::core::image::compare_key(a, b, sort));

        self.image_index = self
            .images
            .iter()
            .enumerate()
            .map(|(i, e)| (ImageHash(e.hash), i))
            .collect();

        self.bookmarks = crate::core::image::resolve_bookmarks(bookmarks, &self.images);
    }
}
