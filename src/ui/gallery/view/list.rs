use super::{Direction, GalleryView, ScrollTarget};
use crate::core::image::ImageId;
use gpui::{ScrollStrategy, UniformListScrollHandle};
use std::ops::Range;

pub struct ListView {
    scroll_handle: UniformListScrollHandle,
    visible_range: Range<usize>,
    thumbnail_size: f32,
}

impl ListView {
    /// Create an empty list view
    pub fn new() -> Self {
        Self {
            scroll_handle: UniformListScrollHandle::new(),
            visible_range: 0..0,
            thumbnail_size: 80.0,
        }
    }

    /// Record the images requested by the virtualized list
    pub fn set_visible_range(&mut self, range: Range<usize>) -> bool {
        if self.visible_range == range {
            return false;
        }
        self.visible_range = range;
        true
    }

    /// Return the images requested by the virtualized list
    pub fn visible_range(&self, image_count: usize) -> Range<usize> {
        self.visible_range.start.min(image_count)..self.visible_range.end.min(image_count)
    }

    /// Return the list scroll handle
    pub fn scroll_handle(&self) -> &UniformListScrollHandle {
        &self.scroll_handle
    }

    /// Return the current thumbnail size
    pub fn thumbnail_size(&self) -> f32 {
        self.thumbnail_size
    }
}

impl GalleryView for ListView {
    /// Find the adjacent image in the list
    fn neighbor(
        &self,
        image_ids: &[ImageId],
        current: Option<&ImageId>,
        direction: Direction,
    ) -> Option<ImageId> {
        if image_ids.is_empty() {
            return None;
        }

        let current = current
            .and_then(|id| image_ids.iter().position(|candidate| candidate == id))
            .unwrap_or(0);
        let delta = match direction {
            Direction::Up | Direction::Left => -1,
            Direction::Down | Direction::Right => 1,
        };
        let next = (current as isize + delta).clamp(0, image_ids.len().saturating_sub(1) as isize)
            as usize;

        image_ids.get(next).cloned()
    }

    /// Scroll the list to a target
    fn scroll_to(&mut self, image_ids: &[ImageId], target: ScrollTarget) {
        if image_ids.is_empty() {
            return;
        }

        let position = match target {
            ScrollTarget::Start => Some((0, ScrollStrategy::Top)),
            ScrollTarget::End => Some((image_ids.len() - 1, ScrollStrategy::Bottom)),
            ScrollTarget::Image(id) => image_ids
                .iter()
                .position(|candidate| candidate == &id)
                .map(|index| (index, ScrollStrategy::Nearest)),
        };

        if let Some((index, strategy)) = position {
            self.scroll_handle.scroll_to_item(index, strategy);
        }
    }

    /// Enlarge list thumbnails
    fn zoom_in(&mut self) -> bool {
        self.thumbnail_size += 16.0;
        true
    }

    /// Shrink list thumbnails
    fn zoom_out(&mut self) -> bool {
        self.thumbnail_size = (self.thumbnail_size - 16.0).max(32.0);
        true
    }
}
