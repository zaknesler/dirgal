pub mod grid;
pub mod grouped;
pub mod list;

use crate::core::image::ImageId;
pub use grid::GridView;
pub use grouped::{GroupHash, GroupedView, Row};
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

pub trait GalleryView {
    /// Find the adjacent image
    fn neighbor(
        &self,
        image_ids: &[ImageId],
        current: Option<&ImageId>,
        direction: Direction,
    ) -> Option<ImageId>;

    /// Scroll to the requested target
    fn scroll_to(&mut self, image_ids: &[ImageId], target: ScrollTarget);

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
