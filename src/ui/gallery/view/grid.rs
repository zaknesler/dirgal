use super::{Direction, GalleryView, GalleryViewEvent, ScrollTarget};
use crate::{
    core::image::ImageId,
    ui::gallery::{
        Gallery,
        constant::{
            GRID_CACHE_ITEMS, GRID_GAP, GRID_OUTER_MARGIN, GRID_TILE_MIN, MAX_COLS, MIN_COLS,
        },
    },
};
use gpui::{
    Context, EventEmitter, Pixels, Render, ScrollStrategy, UniformListScrollHandle, WeakEntity,
    Window, div, prelude::*, px, uniform_list,
};
use gpui_component::{h_flex, scroll::Scrollbar};
use std::ops::Range;

pub struct GridView {
    gallery: WeakEntity<Gallery>,
    scroll_handle: UniformListScrollHandle,
    visible_rows: Range<usize>,
    tile_size: f32,
    columns: usize,
    column_override: Option<usize>,
}

impl GridView {
    /// Create an empty grid view
    pub fn new(gallery: WeakEntity<Gallery>, cx: &mut Context<Self>) -> Self {
        if let Some(parent) = gallery.upgrade() {
            cx.observe(&parent, |_, _, cx| cx.notify()).detach();
        }

        Self {
            gallery,
            scroll_handle: UniformListScrollHandle::new(),
            visible_rows: 0..0,
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

        self.columns = columns;
        self.tile_size =
            ((available - columns.saturating_sub(1) as f32 * GRID_GAP) / columns as f32).max(30.0);
    }

    /// Return the number of rows for an image count
    pub fn row_count(&self, image_count: usize) -> usize {
        image_count.div_ceil(self.columns)
    }

    /// Return the image range for a row
    pub fn row_range(&self, row: usize, image_count: usize) -> std::ops::Range<usize> {
        let start = row * self.columns;
        start..(start + self.columns).min(image_count)
    }

    /// Record the rows requested by the virtualized list
    pub fn set_visible_rows(&mut self, rows: Range<usize>) -> bool {
        if self.visible_rows == rows {
            return false;
        }

        self.visible_rows = rows;
        true
    }

    /// Return the image range requested by the virtualized list
    pub fn visible_image_range(&self, image_count: usize) -> Range<usize> {
        let start = (self.visible_rows.start * self.columns).min(image_count);
        let end = (self.visible_rows.end * self.columns).min(image_count);
        start..end
    }
}

impl Render for GridView {
    /// Render the virtualized flat grid
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_layout(window.viewport_size().width);
        let gallery = self.gallery.upgrade().expect("gallery should exist");
        let image_count = gallery.read(cx).filtered_images.len();
        let visible = self.visible_image_range(image_count);
        let image_ids = gallery.read(cx).filtered_images[visible].to_vec();
        cx.emit(GalleryViewEvent::VisibleImagesChanged(image_ids));

        let row_count = self.row_count(image_count);
        let tile_size = self.tile_size;
        let scroll_handle = self.scroll_handle.clone();

        div()
            .image_cache(crate::ui::cache::simple_lru_cache(
                crate::ui::CONTEXT_GRID,
                GRID_CACHE_ITEMS,
            ))
            .flex_1()
            .min_h_0()
            .relative()
            .child(
                uniform_list(
                    "grid",
                    row_count,
                    cx.processor(move |view, range: Range<usize>, _, cx| {
                        let gallery = view.gallery.upgrade().expect("gallery should exist");

                        // Keep track of which rows are visible (mainly for thumbnail queue)
                        if view.set_visible_rows(range.clone()) {
                            cx.notify();
                        }

                        let image_count = gallery.read(cx).filtered_images.len();

                        let mut rows = Vec::new();
                        for row in range {
                            let range = view.row_range(row, image_count);
                            let image_ids = gallery.read(cx).filtered_images[range].to_vec();

                            rows.push(
                                h_flex()
                                    .w_full()
                                    .gap(px(GRID_GAP))
                                    .pb(px(GRID_GAP))
                                    .px(px(GRID_OUTER_MARGIN))
                                    .children(image_ids.iter().map(|id| {
                                        super::thumbnail::ImageTile::render(
                                            &gallery, id, tile_size, cx,
                                        )
                                    }))
                                    .into_any_element(),
                            );
                        }

                        rows
                    }),
                )
                .track_scroll(&scroll_handle)
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(Scrollbar::vertical(&scroll_handle)),
            )
    }
}

impl EventEmitter<GalleryViewEvent> for GridView {}

impl GalleryView for GridView {
    /// Find the adjacent image in the grid
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
            Direction::Left => -1,
            Direction::Right => 1,
            Direction::Up => -(self.columns as isize),
            Direction::Down => self.columns as isize,
        };
        let next = (current as isize + delta).clamp(0, image_ids.len().saturating_sub(1) as isize)
            as usize;

        image_ids.get(next).cloned()
    }

    /// Scroll the grid to a target
    fn scroll_to(&mut self, image_ids: &[ImageId], target: ScrollTarget) -> bool {
        if image_ids.is_empty() {
            return false;
        }

        let position = match target {
            ScrollTarget::Start => Some((0, ScrollStrategy::Top)),
            ScrollTarget::End => {
                Some((self.row_count(image_ids.len()) - 1, ScrollStrategy::Bottom))
            }
            ScrollTarget::Image(id) => image_ids
                .iter()
                .position(|candidate| candidate == &id)
                .map(|index| (index / self.columns, ScrollStrategy::Nearest)),
        };

        if let Some((index, strategy)) = position {
            self.scroll_handle.scroll_to_item(index, strategy);
            true
        } else {
            false
        }
    }

    /// Enlarge tiles by removing a column
    fn zoom_in(&mut self) -> bool {
        let current = self.column_override.unwrap_or(self.columns);
        if current <= MIN_COLS {
            return false;
        }
        self.column_override = Some(current - 1);
        true
    }

    /// Shrink tiles by adding a column
    fn zoom_out(&mut self) -> bool {
        let current = self.column_override.unwrap_or(self.columns);
        if current >= MAX_COLS {
            return false;
        }
        self.column_override = Some(current + 1);
        true
    }

    /// Restore automatic column sizing
    fn zoom_reset(&mut self) -> bool {
        self.column_override.take().is_some()
    }
}
