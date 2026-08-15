use super::{Direction, GalleryView, ScrollTarget};
use crate::{
    core::{
        hash::hash_path,
        image::{ImageId, compare_parents},
    },
    ui::gallery::{
        constant::{GRID_GAP, GRID_OUTER_MARGIN, GRID_OVERDRAW, GRID_TILE_MIN, MAX_COLS, MIN_COLS},
        library::Library,
    },
};
use gpui::{ListAlignment, ListOffset, ListState, Pixels, px};
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
    rows: Vec<Row>,
    groups: Vec<Group>,
    collapsed_groups: HashSet<GroupHash>,
    list_state: ListState,
    tile_size: f32,
    columns: usize,
    column_override: Option<usize>,
}

impl GroupedView {
    /// Create an empty grouped view
    pub fn new() -> Self {
        Self {
            ordered_indices: Vec::new(),
            rows: Vec::new(),
            groups: Vec::new(),
            collapsed_groups: HashSet::new(),
            list_state: ListState::new(0, ListAlignment::Top, px(GRID_OVERDRAW)).measure_all(),
            tile_size: GRID_TILE_MIN,
            columns: 1,
            column_override: None,
        }
    }

    /// Calculate columns and tile size
    pub fn update_layout(&mut self, width: Pixels) {
        let available = width.as_f32() - GRID_OUTER_MARGIN * 2.0;
        let columns = self.column_override.unwrap_or_else(|| {
            (((available + GRID_GAP) / (GRID_TILE_MIN + GRID_GAP)).floor() as usize).max(1)
        });
        let tile_size =
            ((available - columns.saturating_sub(1) as f32 * GRID_GAP) / columns as f32).max(30.0);

        if columns != self.columns {
            self.columns = columns;
            self.rebuild_rows();
        }
        self.tile_size = tile_size;
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
        self.rebuild_rows();
    }

    /// Return a grouped row
    pub fn row(&self, index: usize) -> Option<Row> {
        self.rows.get(index).cloned()
    }

    /// Return the number of grouped rows
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Return the grouped list state
    pub fn list_state(&self) -> &ListState {
        &self.list_state
    }

    /// Return the current tile size
    pub fn tile_size(&self) -> f32 {
        self.tile_size
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
        if self.collapsed_groups.remove(&hash) {
            self.rebuild_rows();
        }
    }

    /// Toggle one group
    pub fn toggle_group(&mut self, hash: GroupHash) {
        if !self.collapsed_groups.remove(&hash) {
            self.collapsed_groups.insert(hash);
        }
        self.rebuild_rows();
    }

    /// Collapse or expand every group
    pub fn toggle_all(&mut self) {
        if self.collapsed_groups.len() == self.groups.len() {
            self.collapsed_groups.clear();
        } else {
            self.collapsed_groups = self.groups.iter().map(|group| group.hash).collect();
        }
        self.rebuild_rows();
    }

    /// Return filtered image indices near the viewport
    pub fn visible_image_indices(&self, viewport_height: f32) -> Vec<usize> {
        if self.rows.is_empty() {
            return Vec::new();
        }

        let row_height = self.tile_size + GRID_GAP;
        let count = ((viewport_height + 2.0 * GRID_OVERDRAW) / row_height).ceil() as usize + 1;
        let len = self.rows.len();
        let anchor = self.list_state.logical_scroll_top().item_ix.min(len);
        let start = anchor.min(len.saturating_sub(count));
        let end = (start + count).min(len);

        self.rows[start..end]
            .iter()
            .filter_map(|row| match row {
                Row::Header(_) => None,
                Row::Tiles(range) => Some(self.ordered_indices[range.clone()].to_vec()),
            })
            .flatten()
            .collect()
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

    /// Rebuild visible headers and tile rows
    fn rebuild_rows(&mut self) {
        let old_rows = std::mem::take(&mut self.rows);

        for group in &self.groups {
            self.rows.push(Row::Header(group.hash));
            if !self.collapsed_groups.contains(&group.hash) {
                self.rows.extend(Row::chunk_tiles(
                    group.range.start,
                    group.range.len(),
                    self.columns,
                ));
            }
        }

        let unchanged_head = std::iter::zip(&old_rows, &self.rows)
            .take_while(|(a, b)| a == b)
            .count();
        let unchanged_tail = std::iter::zip(
            old_rows[unchanged_head..].iter().rev(),
            self.rows[unchanged_head..].iter().rev(),
        )
        .take_while(|(a, b)| a == b)
        .count();

        self.list_state.splice(
            unchanged_head..old_rows.len() - unchanged_tail,
            self.rows.len() - unchanged_head - unchanged_tail,
        );
    }

    /// Return image indices from expanded groups
    fn visible_indices(&self) -> Vec<usize> {
        self.groups
            .iter()
            .filter(|group| !self.collapsed_groups.contains(&group.hash))
            .flat_map(|group| self.ordered_indices[group.range.clone()].iter().copied())
            .collect()
    }
}

impl GalleryView for GroupedView {
    /// Find the adjacent image in expanded groups
    fn neighbor(
        &self,
        image_ids: &[ImageId],
        current: Option<&ImageId>,
        direction: Direction,
    ) -> Option<ImageId> {
        let visible = self.visible_indices();
        if visible.is_empty() {
            return None;
        }

        let current = current
            .and_then(|id| visible.iter().position(|index| &image_ids[*index] == id))
            .unwrap_or(0);
        let delta = match direction {
            Direction::Left => -1,
            Direction::Right => 1,
            Direction::Up => -(self.columns as isize),
            Direction::Down => self.columns as isize,
        };
        let next = (current as isize + delta).clamp(0, visible.len() as isize - 1) as usize;

        visible.get(next).map(|index| image_ids[*index].clone())
    }

    /// Scroll the grouped list to a target
    fn scroll_to(&mut self, image_ids: &[ImageId], target: ScrollTarget) {
        match target {
            ScrollTarget::Start => self.list_state.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(0.0),
            }),
            ScrollTarget::End => self.list_state.scroll_to_end(),
            ScrollTarget::Image(id) => {
                let Some(position) = self.image_position(image_ids, &id) else {
                    return;
                };
                let Some(row) = self.rows.iter().position(|row| match row {
                    Row::Header(_) => false,
                    Row::Tiles(range) => range.contains(&position),
                }) else {
                    return;
                };
                self.list_state.scroll_to(ListOffset {
                    item_ix: row,
                    offset_in_item: px(0.0),
                });
            }
        }
    }

    /// Return the first image in an expanded group
    fn first_image(&self, image_ids: &[ImageId]) -> Option<ImageId> {
        GroupedView::first_image(self, image_ids)
    }

    /// Enlarge grouped tiles
    fn zoom_in(&mut self) -> bool {
        let current = self.column_override.unwrap_or(self.columns);
        self.column_override = Some(current.saturating_sub(1).max(MIN_COLS));
        true
    }

    /// Shrink grouped tiles
    fn zoom_out(&mut self) -> bool {
        let current = self.column_override.unwrap_or(self.columns);
        self.column_override = Some((current + 1).min(MAX_COLS));
        true
    }

    /// Restore automatic grouped sizing
    fn zoom_reset(&mut self) -> bool {
        self.column_override = None;
        true
    }

    /// Collapse or expand every group
    fn toggle_groups(&mut self) -> bool {
        self.toggle_all();
        true
    }
}
