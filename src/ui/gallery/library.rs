use crate::core::image::{ContentHash, ImageEntry, ImageId};
use crate::ui::{model::*, state};
use std::collections::HashMap;
use std::path::PathBuf;

pub struct Library {
    pub roots: Vec<PathBuf>,
    pub images: Vec<ImageEntry>,
    pub image_index: HashMap<ImageId, usize>,
    pub duplicates: Vec<ImageEntry>,
    pub duplicate_index: HashMap<ImageId, usize>,
    pub primary_content_index: HashMap<ContentHash, usize>,
    pub bookmarks: Vec<ContentHash>,
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
            primary_content_index: HashMap::new(),
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
            .map(|(i, e)| (e.id.clone(), i))
            .collect();

        let duplicate_index = duplicates
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        let primary_content_index = images
            .iter()
            .enumerate()
            .map(|(i, e)| (e.content_hash, i))
            .collect();

        let bookmarks = crate::core::image::resolve_bookmarks(&state.scanner.bookmarks, &images);

        Self {
            roots: state.scanner.roots,
            images,
            image_index,
            duplicates,
            duplicate_index,
            primary_content_index,
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
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        self.primary_content_index = self
            .images
            .iter()
            .enumerate()
            .map(|(i, e)| (e.content_hash, i))
            .collect();

        self.bookmarks = crate::core::image::resolve_bookmarks(bookmarks, &self.images);
    }

    pub fn get(&self, id: &ImageId) -> Option<&ImageEntry> {
        // TODO: make this more resilient? it shouldn't just assume it's in the dupe index
        if let Some(index) = self.image_index.get(id) {
            self.images.get(*index)
        } else {
            self.duplicate_index
                .get(id)
                .and_then(|index| self.duplicates.get(*index))
        }
    }

    pub fn primary_for_content(&self, hash: &ContentHash) -> Option<&ImageEntry> {
        self.primary_content_index
            .get(hash)
            .and_then(|index| self.images.get(*index))
    }

    pub fn contains_id(&self, id: &ImageId) -> bool {
        self.image_index.contains_key(id) || self.duplicate_index.contains_key(id)
    }

    pub fn contains_content(&self, hash: &ContentHash) -> bool {
        self.primary_content_index.contains_key(hash)
    }
}
