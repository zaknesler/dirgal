// The zoom and scroll logic is ported from Zed's image_viewer crate (GPL-3.0-or-later):
// https://github.com/zed-industries/zed/blob/daec37bdc54d3985ff2a8175fd05b73d0444d569/crates/image_viewer/src/image_viewer.rs

use crate::assets::IconAsset;
use crate::core::{
    image::{ImageId, format_bytes},
    path::label_for,
    util,
};
use crate::ui::gallery::Gallery;
use crate::ui::{gallery::constant::*, model::*};
use gpui::{
    Bounds, ClickEvent, Context, DevicePixels, ObjectFit, PinchEvent, Pixels, Point,
    ScrollWheelEvent, SharedString, Size, Window, canvas, div, img, point, prelude::*, px, size,
};
use gpui_component::{
    ActiveTheme, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::ContextMenuExt,
    tag::Tag,
    v_flex,
};

pub struct Lightbox {
    /// Image being shown
    pub image_id: ImageId,
    /// Pixel dimensions of that image, if they could be read
    pub dimensions: Option<(u32, u32)>,
    /// Bounds of the image area, measured while rendering
    pub area_bounds: Option<Bounds<Pixels>>,
    /// Multiple of the fitted size the image is drawn at
    pub zoom: f32,
    /// How far the image is scrolled from the center of the area
    pub offset: Point<Pixels>,
}

impl Lightbox {
    pub fn new(image_id: ImageId, dimensions: Option<(u32, u32)>) -> Self {
        Self {
            image_id,
            dimensions,
            area_bounds: None,
            zoom: 1.0,
            offset: Point::default(),
        }
    }

    /// Bounds of the image as drawn, which is smaller than the area when it is letterboxed
    pub fn image_bounds(&self) -> Option<Bounds<Pixels>> {
        let area = self.area_bounds?;
        let image_size = self.scaled_size()?;

        // Center the image in the area, then shift it by however far it is scrolled
        let origin = area.center() - image_size.center() + self.offset;

        Some(Bounds {
            origin,
            size: image_size,
        })
    }

    /// Bounds of the image relative to its area, where children are positioned from
    pub fn image_bounds_in_area(&self) -> Option<Bounds<Pixels>> {
        let bounds = self.image_bounds()?;

        Some(Bounds {
            origin: bounds.origin - self.area_bounds?.origin,
            size: bounds.size,
        })
    }

    /// Size of the image as drawn, fitted to the area and then scaled by the zoom level
    fn scaled_size(&self) -> Option<Size<Pixels>> {
        let (width, height) = self.dimensions?;

        // Compute the fitted size using the area (measured when it's rendered) and image dimensions
        let bounds = self.area_bounds?;
        let image_size = size(DevicePixels(width as i32), DevicePixels(height as i32));
        let fitted = ObjectFit::Contain.get_bounds(bounds, image_size).size;

        Some(size(fitted.width * self.zoom, fitted.height * self.zoom))
    }

    /// Scroll the image by the given delta, and stop it from moving past its own edges
    fn scroll_by(&mut self, delta: Point<Pixels>) {
        self.offset += delta;
        self.clamp_offset();
    }

    /// Zoom by the given factor, optionally keeping the center position in place
    fn zoom_by(&mut self, factor: f32, center: Option<Point<Pixels>>) {
        let previous = self.zoom;
        self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);

        // Shift the offset so the centered point stays put while the image grows around it
        if let Some((center, area)) = center.zip(self.area_bounds) {
            let ratio = self.zoom / previous;
            let from_image = center - area.center() - self.offset;

            self.offset += from_image.map(|distance| distance * (1.0 - ratio));
        }

        self.clamp_offset();
    }

    /// Drop the zoom and scroll offset, fitting the image to its area again
    fn reset_zoom(&mut self) {
        self.zoom = 1.0;
        self.offset = Point::default();
    }

    /// Zoom until the shorter side of the image is flush with the area, then start at its top left
    fn fill_area(&mut self) {
        let Some(zoom) = self.fill_zoom() else {
            return;
        };

        self.zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        self.offset = self.scroll_slack();
    }

    /// Zoom level at which the image covers the area rather than fitting inside it
    fn fill_zoom(&self) -> Option<f32> {
        let (width, height) = self.dimensions?;
        let area = self.area_bounds?.size;

        let scale_x = f32::from(area.width) / width as f32;
        let scale_y = f32::from(area.height) / height as f32;

        // The zoom is a multiple of the fitted size, so this is how much larger covering it is
        Some(f32::max(scale_x, scale_y) / f32::min(scale_x, scale_y))
    }

    /// How far the image can be scrolled from center before an edge comes inside the area
    fn scroll_slack(&self) -> Point<Pixels> {
        let (Some(area), Some(image_size)) = (self.area_bounds, self.scaled_size()) else {
            return Point::default();
        };

        // Find the hidden part of the image that cannot be scrolled into view
        let hidden = size(
            (image_size.width - area.size.width).max(px(0.)),
            (image_size.height - area.size.height).max(px(0.)),
        );

        hidden.center()
    }

    /// Only the hidden part of the image can be scrolled into view
    fn clamp_offset(&mut self) {
        let slack = self.scroll_slack();

        self.offset = point(
            self.offset.x.clamp(-slack.x, slack.x),
            self.offset.y.clamp(-slack.y, slack.y),
        );
    }
}

impl Gallery {
    /// Image the lightbox is showing, if open
    pub fn lightbox_image_id(&self) -> Option<&ImageId> {
        self.lightbox.as_ref().map(|lightbox| &lightbox.image_id)
    }

    /// Store the measured image area, re-rendering only when it changes
    pub fn set_image_area_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        // Get the inner lightbox
        let Some(lightbox) = self.lightbox.as_mut() else {
            return;
        };

        // Only update the bounds if they have changed
        if lightbox.area_bounds == Some(bounds) {
            return;
        }

        lightbox.area_bounds = Some(bounds);
        cx.notify();
    }

    /// Whether a position is within the bounds of the image itself
    fn is_position_within_image(&self, position: Point<Pixels>) -> bool {
        self.lightbox
            .as_ref()
            .and_then(|lightbox| lightbox.image_bounds())
            .is_none_or(|bounds| bounds.contains(&position))
    }

    /// Zoom the lightbox image in a step
    pub fn zoom_lightbox_in(&mut self, cx: &mut Context<Self>) {
        self.zoom_lightbox_by(ZOOM_STEP, None, cx);
    }

    /// Zoom the lightbox image out a step
    pub fn zoom_lightbox_out(&mut self, cx: &mut Context<Self>) {
        self.zoom_lightbox_by(1.0 / ZOOM_STEP, None, cx);
    }

    /// Fit the lightbox image back to its area
    pub fn zoom_lightbox_reset(&mut self, cx: &mut Context<Self>) {
        let Some(lightbox) = self.lightbox.as_mut() else {
            return;
        };

        lightbox.reset_zoom();
        cx.notify();
    }

    /// Zoom the lightbox image until it fills its area, leaving the longer side to scroll
    pub fn zoom_lightbox_fill(&mut self, cx: &mut Context<Self>) {
        let Some(lightbox) = self.lightbox.as_mut() else {
            return;
        };

        lightbox.fill_area();
        cx.notify();
    }

    /// Zoom the lightbox image by a factor, optionally centered on a point
    fn zoom_lightbox_by(
        &mut self,
        factor: f32,
        center: Option<Point<Pixels>>,
        cx: &mut Context<Self>,
    ) {
        let Some(lightbox) = self.lightbox.as_mut() else {
            return;
        };

        lightbox.zoom_by(factor, center);
        cx.notify();
    }

    /// Pan around (or zoom) the open image
    fn on_image_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height());

        // Zoom when ctrl/cmd is pressed
        if event.modifiers.secondary() {
            let amount = f32::from(delta.y).abs() * ZOOM_PER_PIXEL;
            let factor = if delta.y > px(0.) {
                1.0 + amount
            } else {
                1.0 / (1.0 + amount)
            };

            self.zoom_lightbox_by(factor, Some(event.position), cx);
            return;
        }

        let Some(lightbox) = self.lightbox.as_mut() else {
            return;
        };

        lightbox.scroll_by(delta);
        cx.notify();
    }

    /// Pinch to zoom around the center of the gesture
    fn on_image_pinch(&mut self, event: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_lightbox_by(1.0 + event.delta, Some(event.position), cx);
    }

    /// Render the full-size image with nav arrows, the thumbnail will render beneath while it loads
    fn render_lightbox_content(&self, id: &ImageId, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = self.get_image_entry(id).expect("image should exist");
        let path = entry.id.clone_path();

        let thumb = match self.thumbs.get(&entry.content_hash) {
            Some(ThumbState::Ready(p)) if p.as_ref() != entry.id.path() => Some(p.clone()),
            _ => None,
        };

        let content_hash = entry.content_hash;
        let is_bookmarked = self.get_bookmark_index(&content_hash).is_some();
        let page = self.page;
        let src_path = entry.id.to_path_buf();

        let prev_button = |cx: &mut Context<'_, Self>| {
            Button::new("prev-arrow")
                .ghost()
                .large()
                .px_4()
                .py_8()
                .icon(IconAsset::ChevronLeft)
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.step(-1, cx);
                }))
        };

        let next_button = |cx: &mut Context<'_, Self>| {
            Button::new("next-arrow")
                .ghost()
                .large()
                .px_4()
                .py_8()
                .icon(IconAsset::ChevronRight)
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.step(1, cx);
                }))
        };

        let image_bounds = self
            .lightbox
            .as_ref()
            .and_then(|lightbox| lightbox.image_bounds_in_area());

        let image = || {
            div()
                .relative()
                .map(|el| match image_bounds {
                    // Place the image where the zoom and scroll offset put it
                    Some(bounds) => el
                        .absolute()
                        .left(bounds.origin.x)
                        .top(bounds.origin.y)
                        .w(bounds.size.width)
                        .h(bounds.size.height),
                    // Fill the area until it has been measured
                    None => el.size_full(),
                })
                .when_some(thumb, |el, thumb_path| {
                    el.child(
                        img(thumb_path)
                            .id("lightbox-thumb")
                            .absolute()
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .child(
                    img(path)
                        .id("lightbox-image")
                        .absolute()
                        .size_full()
                        .object_fit(ObjectFit::Contain),
                )
        };

        let image_view = |cx: &mut Context<'_, Self>| {
            let this = cx.entity();

            div()
                .id("image-area")
                .relative()
                .flex_1()
                .min_h_0()
                .size_full()
                .overflow_hidden()
                .on_click(cx.listener(|this, event: &ClickEvent, _, cx| {
                    // Clicks on the backdrop (not the image) should bubble up and close the lightbox
                    if !this.is_position_within_image(event.position()) {
                        return;
                    }

                    // TODO: Open context menu on when image is right clicked (not the entire overlay)
                    // if event.is_right_click() {
                    //     return;
                    // }

                    cx.stop_propagation();
                }))
                .on_scroll_wheel(cx.listener(Self::on_image_scroll_wheel))
                .on_pinch(cx.listener(Self::on_image_pinch))
                .context_menu(move |menu, _, _| {
                    super::view::thumbnail::ImageTile::context_menu(
                        menu,
                        content_hash,
                        is_bookmarked,
                        page,
                        &src_path,
                    )
                })
                .child(
                    canvas(
                        move |bounds, _, cx| {
                            // We need to do this weird thing here to get the bounds
                            this.update(cx, |this, cx| this.set_image_area_bounds(bounds, cx))
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .size_full(),
                )
                .child(image())
        };

        h_flex()
            .flex_1()
            .size_full()
            .min_w_0()
            .pt_4()
            .px_4()
            .gap_4()
            .child(prev_button(cx))
            .child(image_view(cx))
            .child(next_button(cx))
    }

    /// Render the fullscreen lightbox overlay with backdrop and info bar
    pub fn render_lightbox(&self, id: &ImageId, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .image_cache(super::cache::simple_lru_cache(
                super::CONTEXT_LIGHTBOX,
                LIGHTBOX_CACHE_ITEMS,
            ))
            .key_context(super::CONTEXT_LIGHTBOX)
            .id(super::CONTEXT_LIGHTBOX)
            .absolute()
            .inset_0()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(COLOR_BACKDROP))
            .occlude()
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_lightbox(cx);
            }))
            .cursor_default()
            .child(self.render_lightbox_content(id, cx))
            .child(self.render_info_bar(id, cx))
    }

    /// Render the lightbox footer with position, name, size, and bookmark toggle
    fn render_info_bar(&self, id: &ImageId, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = self.get_image_entry(id).expect("image should exist");
        let name = label_for(&self.library.roots, entry.id.path());
        let bytes = format_bytes(entry.bytes);

        let position = self.get_visible_position(id).map(|p| p + 1).unwrap_or(0);
        let counter = format!(
            "{} / {}",
            util::format_num(position),
            util::format_num(self.filtered_images.len())
        );

        let counter = || {
            Tag::secondary()
                .flex_none()
                .min_w_24()
                .p_2()
                .justify_center()
                .child(counter)
        };

        let name = || {
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_sm()
                .text_overflow(gpui::TextOverflow::TruncateMiddle(
                    SharedString::new_static(TRUNCATE_STR),
                ))
                .child(name)
        };

        let size = || {
            h_flex()
                .flex_none()
                .text_right()
                .text_color(cx.theme().muted_foreground)
                .child(bytes)
        };

        let is_bookmarked = self.get_bookmark_index(&entry.content_hash).is_some();
        let content_hash = entry.content_hash;
        let path = entry.id.to_path_buf();
        let actions = || {
            h_flex()
                .flex_none()
                .text_color(cx.theme().muted_foreground)
                .child(
                    Button::new("copy-path")
                        .ghost()
                        .icon(IconAsset::Copy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.copy_path_to_clipboard(&path, cx);
                        })),
                )
                .child(
                    Button::new("bookmark")
                        .ghost()
                        .icon(if is_bookmarked {
                            IconAsset::BookmarkOff
                        } else {
                            IconAsset::Bookmark
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_bookmark(&content_hash, cx);
                        })),
                )
        };

        h_flex().p_4().w_full().justify_center().child(
            h_flex()
                .id("info-bar")
                .min_w_0()
                .max_w(px(750.))
                .w_full()
                .items_center()
                .overflow_hidden()
                .justify_between()
                .gap_3()
                .p_1p5()
                .rounded_xl()
                .bg(cx.theme().background)
                .border_1()
                .border_color(cx.theme().border)
                .text_sm()
                .text_color(cx.theme().foreground)
                .cursor_default()
                .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                .child(counter())
                .child(name())
                .child(size())
                .child(actions()),
        )
    }
}
