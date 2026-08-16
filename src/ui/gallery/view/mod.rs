pub mod grid;
pub mod grouped;
pub mod list;
pub(crate) mod thumbnail;

use crate::core::image::ImageId;
use gpui::Modifiers;
pub use grid::GridView;
pub use grouped::{GroupHash, GroupedView};
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

impl From<Modifiers> for SelectionMode {
    /// Convert click modifiers into a selection mode
    fn from(modifiers: Modifiers) -> Self {
        // TODO: should toggling really need cmd/ctrl?
        if modifiers.secondary() {
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
