pub mod grid;
pub mod grouped;
pub mod list;
pub(crate) mod thumbnail;

use crate::core::image::ImageId;
use gpui_kit::gpui::{App, Modifiers};
pub use grid::GridView;
pub use grouped::{GroupHash, GroupMode, GroupedView};
pub use list::ListView;

#[derive(Clone, Copy)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

pub enum ScrollTarget {
    Start,
    End,
    Image(ImageId),
}

#[derive(Clone)]
pub enum GalleryViewEvent {
    SelectImage { id: ImageId, mode: SelectionMode },
    OpenImage(ImageId),
    VisibleImagesChanged(Vec<ImageId>),
}

#[derive(Clone, Copy)]
pub enum SelectionMode {
    Replace,
    Toggle,
    Extend,
}

impl SelectionMode {
    /// Convert click modifiers into a selection mode
    pub fn from_modifiers(modifiers: Modifiers, cx: &App) -> Self {
        // TODO: should toggling really need cmd/ctrl?
        if modifiers.secondary(cx) {
            Self::Toggle
        } else if modifiers.shift {
            Self::Extend
        } else {
            Self::Replace
        }
    }
}

pub trait GalleryView {
    /// Find the adjacent image in the given direction
    fn neighbor(
        &self,
        image_ids: &[ImageId],
        current: Option<&ImageId>,
        direction: Direction,
    ) -> Option<ImageId>;

    /// Find the image `delta` places from `current` in display order, wrapping at the ends
    fn step(&self, image_ids: &[ImageId], current: &ImageId, delta: isize) -> Option<ImageId> {
        if image_ids.is_empty() {
            return None;
        }

        let pos = self.position(image_ids, current).unwrap_or(0) as isize;
        let next = (pos + delta).rem_euclid(image_ids.len() as isize) as usize;

        image_ids.get(next).cloned()
    }

    /// Index of an image in display order
    fn position(&self, image_ids: &[ImageId], id: &ImageId) -> Option<usize> {
        image_ids.iter().position(|candidate| candidate == id)
    }

    /// Scroll to the requested target
    fn scroll_to(&mut self, image_ids: &[ImageId], target: ScrollTarget) -> bool;

    /// Return the first image available to the view
    fn first_image(&self, image_ids: &[ImageId]) -> Option<ImageId> {
        image_ids.first().cloned()
    }

    /// Increase thumbnail size
    fn zoom_in(&mut self) -> bool {
        false
    }

    /// Decrease thumbnail size
    fn zoom_out(&mut self) -> bool {
        false
    }

    /// Restore the default thumbnail size
    fn zoom_reset(&mut self) -> bool {
        false
    }

    /// Toggle all collapsible groups
    fn toggle_groups(&mut self) -> bool {
        false
    }
}
