use crate::assets::IconAsset;
use crate::core::{image::format_bytes, path::label_for, util};
use crate::ui::gallery::Gallery;
use crate::ui::{gallery::constant::*, model::*};
use gpui::{Context, ObjectFit, div, img, prelude::*};
use gpui::{SharedString, px};
use gpui_component::{
    ActiveTheme, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::ContextMenuExt,
    scroll::ScrollableElement,
    tag::Tag,
    v_flex,
};

/// State of the open lightbox
pub struct Lightbox {
    /// Image being shown
    pub hash: ImageHash,
}

impl Lightbox {
    pub fn new(hash: ImageHash) -> Self {
        Self { hash }
    }
}

impl Gallery {
    /// Image the lightbox is showing, if open
    pub fn lightbox_hash(&self) -> Option<ImageHash> {
        self.lightbox.as_ref().map(|lightbox| lightbox.hash)
    }

    /// Render the full-size image with nav arrows, the thumbnail will render beneath while it loads
    fn render_lightbox_content(
        &self,
        hash: &ImageHash,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entry = self.get_image_entry(hash).expect("image should exist");
        let path = entry.src_path.clone();

        let thumb = match self.thumbs.get(hash) {
            Some(ThumbState::Ready(p)) if *p != entry.src_path => Some(p.clone()),
            _ => None,
        };

        let hash = *hash;
        let is_bookmarked = self.get_bookmark_index(&hash).is_some();
        let page = self.page;
        let src_path = entry.src_path.to_path_buf();

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

        let image_view = |cx: &mut Context<'_, Self>| {
            div()
                .id("image-area")
                .relative()
                .flex_1()
                .min_h_0()
                .size_full()
                .overflow_hidden()
                .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                .overflow_scrollbar()
                .context_menu(move |menu, _, _| {
                    Self::image_context_menu(menu, hash, is_bookmarked, page, &src_path)
                })
                .child(
                    div()
                        .size_full()
                        .relative()
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
                        ),
                )
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
    pub fn render_lightbox(&self, hash: &ImageHash, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(self.render_lightbox_content(hash, cx))
            .child(self.render_info_bar(hash, cx))
    }

    /// Render the lightbox footer with position, name, size, and bookmark toggle
    fn render_info_bar(&self, hash: &ImageHash, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = self.get_image_entry(hash).expect("image should exist");
        let name = label_for(&self.library.roots, &entry.src_path);
        let bytes = format_bytes(entry.bytes);

        let position = self.get_visible_position(hash).map(|p| p + 1).unwrap_or(0);
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

        let is_bookmarked = self.get_bookmark_index(hash).is_some();
        let hash = *hash;
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
                            this.copy_path_to_clipboard(&hash, cx);
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
                            this.toggle_bookmark(&hash, cx);
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
