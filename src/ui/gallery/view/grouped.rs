use crate::{
    core::{
        hash::hash_path,
        image::{ImageId, compare_parents},
    },
    ui::gallery::library::Library,
};
use std::{
    collections::HashSet,
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct GroupHash(pub u64);

#[derive(Clone, PartialEq)]
pub enum Row {
    Header(GroupHash),
    Tiles(Range<usize>),
}

impl Row {
    /// Split a range of images into tile rows
    pub fn chunk_tiles(offset: usize, len: usize, columns: usize) -> impl Iterator<Item = Row> {
        (0..len).step_by(columns).map(move |start| {
            let end = (start + columns).min(len);
            Row::Tiles(offset + start..offset + end)
        })
    }
}

pub struct Group {
    pub hash: GroupHash,
    pub path: PathBuf,
    pub range: Range<usize>,
}

pub struct GroupedView {
    ordered_indices: Vec<usize>,
    groups: Vec<Group>,
    collapsed_groups: HashSet<GroupHash>,
}

impl GroupedView {
    /// Create an empty grouped view
    pub fn new() -> Self {
        Self {
            ordered_indices: Vec::new(),
            groups: Vec::new(),
            collapsed_groups: HashSet::new(),
        }
    }

    /// Rebuild grouped ordering from the filtered images
    pub fn rebuild(&mut self, image_ids: &[ImageId], library: &Library) {
        self.ordered_indices = (0..image_ids.len()).collect();
        self.ordered_indices.sort_by(|a, b| {
            match (library.get(&image_ids[*a]), library.get(&image_ids[*b])) {
                (Some(a), Some(b)) => compare_parents(a, b),
                _ => std::cmp::Ordering::Equal,
            }
        });
        self.rebuild_groups(image_ids, library);
    }

    /// Build rows for the current groups and column count
    pub fn rows(&self, columns: usize) -> Vec<Row> {
        let mut rows = Vec::new();

        for group in &self.groups {
            rows.push(Row::Header(group.hash));
            if !self.collapsed_groups.contains(&group.hash) {
                rows.extend(Row::chunk_tiles(
                    group.range.start,
                    group.range.len(),
                    columns,
                ));
            }
        }

        rows
    }

    /// Find a group by hash
    pub fn group(&self, hash: GroupHash) -> Option<&Group> {
        self.groups.iter().find(|group| group.hash == hash)
    }

    /// Return the number of groups
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Check whether a group is collapsed
    pub fn is_collapsed(&self, hash: GroupHash) -> bool {
        self.collapsed_groups.contains(&hash)
    }

    /// Return filtered image indices for a grouped range
    pub fn image_indices(&self, range: Range<usize>) -> &[usize] {
        &self.ordered_indices[range]
    }

    /// Find an image's position in grouped order
    pub fn image_position(&self, image_ids: &[ImageId], id: &ImageId) -> Option<usize> {
        self.ordered_indices
            .iter()
            .position(|index| &image_ids[*index] == id)
    }

    /// Return the first image in an expanded group
    pub fn first_image(&self, image_ids: &[ImageId]) -> Option<ImageId> {
        self.groups
            .iter()
            .find(|group| !self.collapsed_groups.contains(&group.hash))
            .and_then(|group| self.ordered_indices.get(group.range.start))
            .map(|index| image_ids[*index].clone())
    }

    /// Expand one group
    pub fn expand_group(&mut self, hash: GroupHash) {
        self.collapsed_groups.remove(&hash);
    }

    /// Toggle one group
    pub fn toggle_group(&mut self, hash: GroupHash) {
        if !self.collapsed_groups.remove(&hash) {
            self.collapsed_groups.insert(hash);
        }
    }

    /// Collapse or expand every group
    pub fn toggle_all(&mut self) {
        if self.collapsed_groups.len() == self.groups.len() {
            self.collapsed_groups.clear();
        } else {
            self.collapsed_groups = self.groups.iter().map(|group| group.hash).collect();
        }
    }

    /// Rebuild directory groups
    fn rebuild_groups(&mut self, image_ids: &[ImageId], library: &Library) {
        self.groups.clear();

        for (position, index) in self.ordered_indices.iter().copied().enumerate() {
            let id = &image_ids[index];
            let parent = library
                .get(id)
                .and_then(|entry| entry.id.path().parent())
                .unwrap_or(Path::new(""));

            match self.groups.last_mut() {
                Some(group) if group.path == parent => group.range.end = position + 1,
                _ => self.groups.push(Group {
                    hash: GroupHash(hash_path(parent)),
                    path: parent.to_path_buf(),
                    range: position..position + 1,
                }),
            }
        }
    }
}
