use crate::ui::{
    gallery::{Gallery, constant::*},
    model::*,
    *,
};
use crate::{
    assets::IconAsset,
    core::{
        path::group_segments,
        util::{self, file_manager_label},
    },
};
use gpui::{
    AnyElement, App, Context, FocusHandle, Focusable, ObjectFit, SharedString, Window, div, img,
    list, prelude::*, px, rems, uniform_list,
};
use gpui_component::{
    ActiveTheme, Icon, InteractiveElementExt, Root, Sizable as _,
    breadcrumb::Breadcrumb,
    button::{Button, ButtonVariants as _, Toggle, ToggleGroup, ToggleVariants},
    h_flex,
    input::Input,
    menu::ContextMenuExt,
    scroll::Scrollbar,
    select::Select,
    skeleton::Skeleton,
    spinner::Spinner,
    tab::{Tab, TabBar},
    tag::Tag,
    v_flex,
};
use std::path::Path;

impl Gallery {
    /// Render a single list row, either a group header or a row of tiles
    fn render_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(index).cloned() else {
            return div().into_any_element();
        };

        match row {
            Row::Header(group_hash) => self.render_header_row(group_hash, index, cx),
            Row::Tiles(range) => self.render_tile_row(range, index, cx),
        }
    }

    /// Render a collapsible group header with breadcrumb path and image count
    fn render_header_row(
        &mut self,
        group_hash: GroupHash,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_last_row = index == self.rows.len() - 1;

        let group = self
            .groups
            .iter()
            .find(|g| g.hash == group_hash)
            .expect("group should exist");
        let segments = group_segments(&self.library.roots, &group.path);
        let count = group.image_hashes.len();
        let is_collapsed = self.collapsed_groups.contains(&group_hash);

        h_flex()
            .id(("header", group_hash.0))
            .w_full()
            .items_center()
            .gap_2()
            .px(px(GRID_OUTER_MARGIN))
            .pt(px(GRID_OUTER_MARGIN))
            .when(!is_collapsed || is_last_row, |el| {
                el.pb(px(GRID_OUTER_MARGIN))
            })
            .cursor_pointer()
            .group("header")
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_group(&group_hash, cx)))
            .child(
                Button::new(("chevron", group_hash.0))
                    .ghost()
                    .small()
                    .icon(if is_collapsed {
                        IconAsset::ChevronRight
                    } else {
                        IconAsset::ChevronDown
                    })
                    .text_color(cx.theme().muted_foreground)
                    .group_hover("header", |el| el.text_color(cx.theme().foreground))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_group(&group_hash, cx);
                    })),
            )
            .child(
                h_flex()
                    .items_center()
                    .flex_none()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(Breadcrumb::new().children(segments)),
            )
            .child(
                Tag::new()
                    .small()
                    .child(util::format_num(count).to_string()),
            )
            .into_any_element()
    }

    /// Render one row of thumbnail tiles for a slice of the filtered images
    fn render_tile_row(
        &mut self,
        range: std::ops::Range<usize>,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_only_row = index == 0;
        let is_last_row = index == self.rows.len() - 1;

        let hashes = self.filtered_images[range].to_vec();

        h_flex()
            .w_full()
            .px(px(GRID_OUTER_MARGIN))
            .gap(px(GRID_GAP))
            .when(is_only_row, |el| el.pt(px(GRID_OUTER_MARGIN)))
            .when_else(
                is_last_row,
                |el| el.pb(px(GRID_OUTER_MARGIN)),
                |el| el.pb(px(GRID_GAP)),
            )
            .children(
                hashes
                    .into_iter()
                    .map(|ref hash| self.render_tile(hash, cx)),
            )
            .into_any_element()
    }

    fn render_thumb(&self, hash: &ImageHash, _: &mut Context<Self>) -> AnyElement {
        let source = self.peek_thumb_path(hash);

        let object_fit = match self.settings.thumbnail_fit {
            ThumbnailFit::Cover => ObjectFit::Cover,
            ThumbnailFit::Contain => ObjectFit::Contain,
        };

        match source {
            Some(path) => img(path)
                .aspect_square()
                .size_full()
                .object_fit(object_fit)
                .into_any_element(),
            None => Self::render_thumb_placeholder().into_any_element(),
        }
    }

    /// Render a clickable tile with context menu and loading placeholder
    fn render_tile(&mut self, hash: &ImageHash, cx: &mut Context<Self>) -> AnyElement {
        let size = px(self.tile_size);
        let is_bookmarked = self.library.bookmarks.contains(hash);
        let is_selected = self.selected_hashes.contains(hash);
        let page = self.page;

        let src_path = self
            .get_image_entry(hash)
            .map(|e| e.src_path.to_path_buf())
            .expect("image should exist");
        let path_str = src_path.to_string_lossy().to_string();

        let hash = *hash;

        div()
            .key_context(super::CONTEXT_GALLERY)
            .id(hash.0 as usize)
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
            .on_click(cx.listener(move |this, event, window, cx| {
                cx.stop_propagation();
                Self::on_thumb_click_event(this, &hash, event, window, cx);
            }))
            .on_double_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.open_lightbox(&hash, cx)
            }))
            .context_menu(move |menu, _, _| {
                Self::image_context_menu(menu, hash, is_bookmarked, page, &src_path)
            })
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .aspect_square()
                    .child(self.render_thumb(&hash, cx)),
            )
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

    /// Build the right-click menu for an image in the grid or lightbox
    pub(super) fn image_context_menu(
        menu: gpui_component::menu::PopupMenu,
        hash: ImageHash,
        is_bookmarked: bool,
        page: Page,
        src_path: &Path,
    ) -> gpui_component::menu::PopupMenu {
        menu.check_side(gpui_component::Side::Right)
            .menu_with_icon_and_disabled(
                "Copy",
                IconAsset::ClipboardCopy,
                Box::new(actions::CopyImage::Path(src_path.to_path_buf())),
                true,
            )
            .menu_with_icon(
                "Trash",
                IconAsset::Recycle,
                Box::new(actions::TrashFile::Path(src_path.to_path_buf())),
            )
            .menu_with_icon(
                "Delete",
                IconAsset::Trash,
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
                Box::new(actions::Bookmark::Hash(hash)),
            )
            .menu_with_icon(
                "Copy full path",
                IconAsset::NotepadText,
                Box::new(actions::CopyPathToClipboard::Hash(hash)),
            )
            .separator()
            .when(page != Page::Gallery, |menu| {
                menu.menu_with_icon(
                    "Reveal in gallery",
                    IconAsset::Grid,
                    Box::new(actions::RevealInGallery(hash)),
                )
            })
            .menu_with_icon(
                format!("Open in {}", file_manager_label().to_lowercase()),
                IconAsset::FolderOpen,
                Box::new(actions::OpenInFinder::Path(src_path.to_path_buf())),
            )
    }

    /// Skeleton with a spinner shown while a thumbnail loads
    fn render_thumb_placeholder() -> impl IntoElement {
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

    /// Render the page navigation tabs
    fn render_tab_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let page_index = self.page.index();

        TabBar::new("navigation")
            .w_full()
            .selected_index(page_index)
            .px_2()
            .on_click(cx.listener(|this, selected_index, _, cx| {
                let (page, _, _) = Page::ALL[*selected_index];
                this.set_page(page, cx);
            }))
            .children(Page::ALL.iter().map(|(_, name, icon)| {
                Tab::new().px_2().child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child(Icon::new(*icon)),
                        )
                        .child(*name),
                )
            }))
    }

    /// Render the toolbar with search input, image counts, and zoom controls
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count_label = match self.page {
            Page::Gallery if self.settings.view == View::Grouped => format!(
                "{} images in {} folders",
                util::format_num(self.filtered_images.len()),
                util::format_num(self.groups.len())
            ),
            Page::Gallery => format!("{} images", util::format_num(self.filtered_images.len())),
            Page::Bookmarks => format!(
                "{} bookmarked images",
                util::format_num(self.filtered_images.len())
            ),
            Page::Duplicates => format!(
                "{} duplicate images",
                util::format_num(self.filtered_images.len())
            ),
        };

        let search = || {
            h_flex()
                .flex_1()
                .gap_2()
                .items_center()
                .w_full()
                .child(
                    Input::new(&self.input)
                        .cleanable(true)
                        .flex_1()
                        .min_w_0()
                        .max_w(px(400.)),
                )
                .child(
                    Button::new("refresh")
                        .ghost()
                        .icon(IconAsset::Refresh)
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.refresh_library(cx);
                        })),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(count_label),
                )
        };

        let sort_ascending = self.settings.sort().ascending;

        let controls = || {
            h_flex()
                .flex_none()
                .items_center()
                .gap_2()
                .child(
                    ToggleGroup::new("view-toggle")
                        .segmented()
                        .outline()
                        .children(View::ALL.iter().map(|(view, _, icon)| {
                            Toggle::new(*view)
                                .icon(*icon)
                                .checked(self.settings.view == *view)
                        }))
                        .on_click(cx.listener(|this, checked: &Vec<bool>, window, cx| {
                            cx.stop_propagation();

                            // Treat as radio button
                            let clicked = checked
                                .iter()
                                .enumerate()
                                .filter(|(_, checked)| **checked)
                                .map(|(index, _)| View::ALL[index].0)
                                .find(|view| *view != this.settings.view);

                            if let Some(view) = clicked {
                                this.set_view(view, window, cx);
                            }
                        })),
                )
                .child(
                    ToggleGroup::new("thumbnail-fit-toggle")
                        .segmented()
                        .outline()
                        .children(ThumbnailFit::ALL.iter().map(|(fit, _, icon)| {
                            Toggle::new(*fit)
                                .icon(*icon)
                                .checked(self.settings.thumbnail_fit == *fit)
                        }))
                        .on_click(cx.listener(|this, checked: &Vec<bool>, window, cx| {
                            cx.stop_propagation();

                            // Treat as radio button
                            let clicked = checked
                                .iter()
                                .enumerate()
                                .filter(|(_, checked)| **checked)
                                .map(|(index, _)| ThumbnailFit::ALL[index].0)
                                .find(|fit| *fit != this.settings.thumbnail_fit);

                            if let Some(fit) = clicked {
                                this.set_thumbnail_fit(fit, window, cx);
                            }
                        })),
                )
                .child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .gap_1()
                        .child(div().w(px(175.)).child(Select::new(&self.sort_select)))
                        .child(
                            Button::new("sort-direction")
                                .ghost()
                                .icon(if sort_ascending {
                                    IconAsset::SortAscending
                                } else {
                                    IconAsset::SortDescending
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.toggle_sort_direction(window, cx);
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .gap_1()
                        .child(
                            Button::new("grid-zoom-out")
                                .ghost()
                                .icon(IconAsset::Minus)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.zoom_grid_out(cx);
                                })),
                        )
                        .child(
                            Button::new("grid-zoom-in")
                                .ghost()
                                .icon(IconAsset::Plus)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.zoom_grid_in(cx);
                                })),
                        ),
                )
        };

        h_flex()
            .gap_4()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(search())
            .child(controls())
    }

    fn render_floating_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let should_hide = self.selected_hashes.is_empty() || self.lightbox.is_some();
        let num_selected = self.selected_hashes.len();

        h_flex()
            .when(should_hide, |el| el.hidden())
            .occlude()
            .p_4()
            .w_full()
            .absolute()
            .bottom_4()
            .justify_center()
            .shadow_2xl()
            .child(
                h_flex()
                    .id("info-bar")
                    .min_w_0()
                    .max_w(px(750.))
                    .w_full()
                    .items_center()
                    .overflow_hidden()
                    .justify_between()
                    .gap_3()
                    .py_2()
                    .px_3()
                    .rounded_xl()
                    .bg(cx.theme().background.opacity(0.875))
                    .border_1()
                    .border_color(cx.theme().border)
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .cursor_default()
                    .on_click(cx.listener(|_, _, _, cx| cx.stop_propagation()))
                    .child(format!("{} image(s) selected", num_selected)),
            )
    }

    /// Render the placeholder shown when no images match
    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No images found.")
    }

    /// Render a virtualized image list with its scrollbar
    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let total_count = self.filtered_images.len();

        div()
            .image_cache(super::cache::simple_lru_cache(
                super::CONTEXT_GRID,
                GRID_CACHE_ITEMS,
            ))
            .flex_1()
            .min_h_0()
            .relative()
            .child(
                uniform_list(
                    "list",
                    total_count,
                    cx.processor(move |this, range, _, cx| {
                        let mut items = Vec::new();
                        for index in range {
                            let hash = this.filtered_images[index];
                            let image = this.get_image_entry(&hash).expect("image should exist");
                            let thumb = this.render_thumb(&hash, cx);

                            items.push(
                                h_flex()
                                    .id(image.hash.to_string())
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
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .overflow_hidden()
                                            .size(px(80.))
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
                                            .child(image.src_path.to_string_lossy().to_string()),
                                    ),
                            );
                        }
                        items
                    }),
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .child(Scrollbar::vertical(&self.grid)),
            )
    }

    /// Render a virtualized image grid with its scrollbar
    fn render_grid(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .image_cache(super::cache::simple_lru_cache(
                super::CONTEXT_GRID,
                GRID_CACHE_ITEMS,
            ))
            .flex_1()
            .min_h_0()
            .relative()
            .child(
                list(
                    self.grid.clone(),
                    cx.processor(|this, index, _, cx| this.render_row(index, cx)),
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .child(Scrollbar::vertical(&self.grid)),
            )
    }
}

impl Focusable for Gallery {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (columns, tile_size) = self.get_grid_layout(window);

        let cols_changed = columns != self.num_columns;

        // Check if tile size has changed by more than a sub-pixel threshold
        let tile_size_changed = (tile_size - self.tile_size).abs() > 0.5;

        if (cols_changed || tile_size_changed) && !self.library.images.is_empty() {
            self.set_layout(columns, tile_size, cx);
        }

        // Queue thumbnails for the visible rows; state set here is picked up when rows render below
        self.enqueue_visible(window, cx);

        let notif_layer = Root::render_notification_layer(window, cx);
        let dialog_layer = Root::render_dialog_layer(window, cx);

        v_flex()
            .key_context(super::CONTEXT_GALLERY)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_apply_preset))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_down))
            .on_action(cx.listener(Self::on_left))
            .on_action(cx.listener(Self::on_right))
            .on_action(cx.listener(Self::on_open_lightbox))
            .on_action(cx.listener(Self::on_togle_view))
            .on_action(cx.listener(Self::on_toggle_thumbnail_fit))
            .on_action(cx.listener(Self::on_close))
            .on_action(cx.listener(Self::on_minimize))
            .on_action(cx.listener(Self::on_zoom_in))
            .on_action(cx.listener(Self::on_zoom_out))
            .on_action(cx.listener(Self::on_zoom_reset))
            .on_action(cx.listener(Self::on_zoom_fill))
            .on_action(cx.listener(Self::on_toggle_bookmark))
            .on_action(cx.listener(Self::on_copy_path_to_clipboard))
            .on_action(cx.listener(Self::on_copy_image))
            .on_action(cx.listener(Self::on_trash_file))
            .on_action(cx.listener(Self::on_delete_file))
            .on_action(cx.listener(Self::on_open_in_finder))
            .on_action(cx.listener(Self::on_reveal_in_gallery))
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_jump_to_top))
            .on_action(cx.listener(Self::on_jump_to_bottom))
            .on_action(cx.listener(Self::on_prev_page))
            .on_action(cx.listener(Self::on_next_page))
            .on_action(cx.listener(Self::on_toggle_collapse))
            .on_action(cx.listener(Self::on_refresh))
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_tab_bar(cx))
            .child(self.render_header(cx))
            .map(|el| {
                if self.filtered_images.is_empty() {
                    el.child(self.render_empty(cx))
                } else if self.settings.view == View::List {
                    el.child(self.render_list(cx))
                } else {
                    el.child(self.render_grid(cx))
                }
            })
            .when_some(self.lightbox_hash(), |el, hash| {
                el.child(self.render_lightbox(&hash, cx))
            })
            .child(self.render_floating_actions(cx))
            .children(notif_layer)
            .children(dialog_layer)
    }
}
