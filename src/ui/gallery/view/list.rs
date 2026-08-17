use super::{Direction, GalleryView, GalleryViewEvent, ScrollTarget};
use crate::{
    core::{hash::hash_path, image::ImageId},
    ui::gallery::{
        Gallery,
        constant::{GRID_CACHE_ITEMS, GRID_OUTER_MARGIN, TRUNCATE_STR},
    },
};
use gpui::{
    ClickEvent, Context, EventEmitter, Render, ScrollStrategy, SharedString,
    UniformListScrollHandle, WeakEntity, Window, div, prelude::*, px, uniform_list,
};
use gpui_component::{ActiveTheme, InteractiveElementExt, h_flex, scroll::Scrollbar, v_flex};
use std::ops::Range;

const DEFAULT_THUMBNAIL_SIZE: f32 = 80.0;
const MIN_THUMBNAIL_SIZE: f32 = 32.0;
const THUMBNAIL_ZOOM_STEP: f32 = 16.0;

pub struct ListView {
    gallery: WeakEntity<Gallery>,
    scroll_handle: UniformListScrollHandle,
    visible_range: Range<usize>,
    thumbnail_size: f32,
}

impl ListView {
    /// Create an empty list view
    pub fn new(gallery: WeakEntity<Gallery>, cx: &mut Context<Self>) -> Self {
        if let Some(parent) = gallery.upgrade() {
            cx.observe(&parent, |_, _, cx| cx.notify()).detach();
        }

        Self {
            gallery,
            scroll_handle: UniformListScrollHandle::new(),
            visible_range: 0..0,
            thumbnail_size: DEFAULT_THUMBNAIL_SIZE,
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
}

impl Render for ListView {
    /// Render the virtualized image list
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let gallery = self.gallery.upgrade().expect("gallery should exist");

        let image_count = gallery.read(cx).filtered_images.len();
        let visible = self.visible_range(image_count);
        let image_ids = gallery.read(cx).filtered_images[visible].to_vec();
        cx.emit(GalleryViewEvent::VisibleImagesChanged(image_ids));

        let thumbnail_size = self.thumbnail_size;
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
                    "list",
                    image_count,
                    cx.processor(move |view, range: Range<usize>, _, cx| {
                        let gallery = view.gallery.upgrade().expect("gallery should exist");

                        if view.set_visible_range(range.clone()) {
                            cx.notify();
                        }

                        let image_ids = gallery.read(cx).filtered_images[range.clone()].to_vec();

                        let mut items = Vec::new();
                        for (index, id) in range.zip(image_ids) {
                            let gallery_state = gallery.read(cx);
                            let image = gallery_state
                                .get_image_entry(&id)
                                .expect("image should exist");
                            let path = image.id.path().to_path_buf();
                            let thumb = super::thumbnail::Thumbnail::render(gallery_state, &id);
                            let click_id = id.clone();
                            let open_id = id.clone();

                            items.push(
                                h_flex()
                                    .id(hash_path(&path) as usize)
                                    .px(px(GRID_OUTER_MARGIN))
                                    .py_2()
                                    .w_full()
                                    .gap_4()
                                    .when(index != 0, |el| el.border_t_1())
                                    .items_center()
                                    .rounded_md()
                                    .overflow_hidden()
                                    .border_color(cx.theme().border)
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |_, event: &ClickEvent, _, cx| {
                                        cx.stop_propagation();
                                        cx.emit(GalleryViewEvent::SelectImage {
                                            id: click_id.clone(),
                                            mode: event.modifiers().into(),
                                        });
                                    }))
                                    .on_double_click(cx.listener(move |_, _, _, cx| {
                                        cx.stop_propagation();
                                        cx.emit(GalleryViewEvent::OpenImage(open_id.clone()));
                                    }))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .overflow_hidden()
                                            .size(px(thumbnail_size))
                                            .child(thumb),
                                    )
                                    .child(
                                        v_flex()
                                            .justify_center()
                                            .flex_1()
                                            .overflow_hidden()
                                            .text_overflow(gpui::TextOverflow::TruncateMiddle(
                                                SharedString::new_static(TRUNCATE_STR),
                                            ))
                                            .child(path.to_string_lossy().to_string()),
                                    ),
                            );
                        }

                        items
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

impl EventEmitter<GalleryViewEvent> for ListView {}

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
    fn scroll_to(&mut self, image_ids: &[ImageId], target: ScrollTarget) -> bool {
        if image_ids.is_empty() {
            return false;
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
            true
        } else {
            false
        }
    }

    /// Enlarge list thumbnails
    fn zoom_in(&mut self) -> bool {
        self.thumbnail_size += THUMBNAIL_ZOOM_STEP;
        true
    }

    /// Shrink list thumbnails
    fn zoom_out(&mut self) -> bool {
        let size = (self.thumbnail_size - THUMBNAIL_ZOOM_STEP).max(MIN_THUMBNAIL_SIZE);
        if size == self.thumbnail_size {
            return false;
        }
        self.thumbnail_size = size;
        true
    }

    /// Restore the default list thumbnail size
    fn zoom_reset(&mut self) -> bool {
        if self.thumbnail_size == DEFAULT_THUMBNAIL_SIZE {
            return false;
        }
        self.thumbnail_size = DEFAULT_THUMBNAIL_SIZE;
        true
    }
}
