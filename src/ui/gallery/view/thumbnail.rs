use crate::{
    assets::IconAsset,
    core::{
        hash::hash_path,
        image::{ContentHash, ImageId},
        util::file_manager_label,
    },
    ui::{
        actions,
        gallery::{
            Gallery,
            constant::{COLOR_ACCENT, COLOR_ACCENT_HOVER, DEBUG},
            view::GalleryViewEvent,
        },
        model::{Page, ThumbnailFit},
    },
};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, EventEmitter, ObjectFit, div, img, prelude::*, px,
    rems,
};
use gpui_component::{
    ActiveTheme, InteractiveElementExt, Sizable as _, menu::ContextMenuExt, skeleton::Skeleton,
    spinner::Spinner, v_flex,
};
use std::path::Path;

pub struct Thumbnail;

impl Thumbnail {
    /// Render a thumbnail or loading placeholder
    pub fn render(gallery: &Gallery, id: &ImageId) -> AnyElement {
        let source = gallery.peek_thumb_path(id);
        let object_fit = match gallery.settings.thumbnail_fit {
            ThumbnailFit::Cover => ObjectFit::Cover,
            ThumbnailFit::Contain => ObjectFit::Contain,
        };

        match source {
            Some(path) => img(path)
                .aspect_square()
                .size_full()
                .object_fit(object_fit)
                .into_any_element(),
            None => Self::placeholder().into_any_element(),
        }
    }

    /// Render the thumbnail loading state
    fn placeholder() -> impl IntoElement {
        div()
            .size_full()
            .child(Skeleton::new().secondary().w_full().h_full())
            .child(
                v_flex()
                    .size_full()
                    .absolute()
                    .inset_0()
                    .items_center()
                    .justify_center()
                    .child(Spinner::new().large()),
            )
    }
}

pub struct ImageTile;

impl ImageTile {
    /// Render an interactive image tile
    pub fn render<V: EventEmitter<GalleryViewEvent>>(
        gallery: &Entity<Gallery>,
        id: &ImageId,
        tile_size: f32,
        cx: &mut Context<V>,
    ) -> AnyElement {
        let size = px(tile_size);
        let gallery_state = gallery.read(cx);
        let entry = gallery_state
            .get_image_entry(id)
            .expect("image should exist");
        let content_hash = entry.content_hash;
        let is_bookmarked = gallery_state.library.bookmarks.contains(&content_hash);
        let is_selected = gallery_state.selected_images.contains(id);
        let page = gallery_state.page;

        let src_path = entry.id.to_path_buf();
        let path_str = src_path.to_string_lossy().to_string();
        let tile_id = hash_path(id.path()) as usize;
        let thumbnail = Thumbnail::render(gallery_state, id);

        let click_id = id.clone();
        let open_id = id.clone();

        div()
            .key_context(crate::ui::CONTEXT_GALLERY)
            .id(tile_id)
            .flex_none()
            .size(size)
            .overflow_hidden()
            .aspect_square()
            .relative()
            .border_3()
            .border_color(gpui::transparent_black())
            .hover(|el| {
                if is_selected {
                    el.border_color(gpui::rgb(COLOR_ACCENT_HOVER))
                } else {
                    el.border_color(gpui::white())
                }
            })
            .when(is_selected, |el| el.border_color(gpui::rgb(COLOR_ACCENT)))
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
            .context_menu(move |menu, _, _| {
                Self::context_menu(menu, content_hash, is_bookmarked, page, &src_path)
            })
            .child(div().absolute().inset_0().aspect_square().child(thumbnail))
            .when(DEBUG, |el| {
                el.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .p_1p5()
                        .text_xs()
                        .line_height(rems(1.1))
                        .bg(cx.theme().background)
                        .text_color(cx.theme().foreground)
                        .child(path_str),
                )
            })
            .into_any_element()
    }

    /// Build the image context menu
    pub(crate) fn context_menu(
        menu: gpui_component::menu::PopupMenu,
        content_hash: ContentHash,
        is_bookmarked: bool,
        page: Page,
        src_path: &Path,
    ) -> gpui_component::menu::PopupMenu {
        menu.menu_with_icon_and_disabled(
            "Copy",
            IconAsset::ClipboardCopy,
            Box::new(actions::CopyImage::Path(src_path.to_path_buf())),
            true,
        )
        .menu_with_icon(
            "Trash",
            IconAsset::Trash,
            Box::new(actions::TrashFile::Path(src_path.to_path_buf())),
        )
        .menu_with_icon(
            "Delete",
            IconAsset::CircleX,
            Box::new(actions::DeleteFile::Path(src_path.to_path_buf())),
        )
        .separator()
        .menu_with_icon(
            if is_bookmarked {
                "Unbookmark"
            } else {
                "Bookmark"
            },
            if is_bookmarked {
                IconAsset::BookmarkOff
            } else {
                IconAsset::Bookmark
            },
            Box::new(actions::Bookmark::Hash(content_hash)),
        )
        .menu_with_icon(
            "Copy full path",
            IconAsset::NotepadText,
            Box::new(actions::CopyPathToClipboard::Path(src_path.to_path_buf())),
        )
        .separator()
        .when(page != Page::Gallery, |menu| {
            menu.menu_with_icon(
                "Reveal in gallery",
                IconAsset::Grid,
                Box::new(actions::RevealInGallery(content_hash)),
            )
        })
        .menu_with_icon(
            format!("Open in {}", file_manager_label().to_lowercase()),
            IconAsset::FolderOpen,
            Box::new(actions::OpenInFinder::Path(src_path.to_path_buf())),
        )
    }
}
