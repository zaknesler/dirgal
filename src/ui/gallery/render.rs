use crate::{
    image::format_bytes,
    path::{group_segments, label_for},
    ui::{
        gallery::{Gallery, constant::*},
        model::*,
        *,
    },
    util::{self, file_manager_label},
};
use gpui::{
    AnyElement, App, Context, FocusHandle, Focusable, MouseDownEvent, ObjectFit, ScrollWheelEvent,
    SharedString, Window, div, img, list, prelude::*, px, rems, uniform_list,
};
use gpui_component::{
    ActiveTheme, Disableable, IconName, InteractiveElementExt, Sizable as _,
    breadcrumb::Breadcrumb,
    button::{Button, ButtonVariants as _, Toggle},
    h_flex,
    input::Input,
    menu::ContextMenuExt,
    scroll::{ScrollableElement, Scrollbar},
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
        let segments = group_segments(&self.roots, &group.path);
        let count = group.image_hashes.len();
        let is_collapsed = self.collapsed_groups.contains(&group_hash);

        h_flex()
            .id(("header", group_hash.0))
            .w_full()
            .items_center()
            .gap_2()
            .px(px(GRID_OUTER_MARGIN / 2.0))
            .pt(px(GRID_OUTER_MARGIN / 2.0))
            .when(!is_collapsed || is_last_row, |el| {
                el.pb(px(GRID_OUTER_MARGIN / 2.0))
            })
            .cursor_pointer()
            .group("header")
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_group(&group_hash, cx)))
            .child(
                Button::new(("chevron", group_hash.0))
                    .ghost()
                    .small()
                    .icon(if is_collapsed {
                        IconName::ChevronRight
                    } else {
                        IconName::ChevronDown
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
            .px(px(GRID_OUTER_MARGIN / 2.0))
            .gap(px(GRID_GAP))
            .when(is_only_row, |el| el.pt(px(GRID_OUTER_MARGIN / 2.0)))
            .when_else(
                is_last_row,
                |el| el.pb(px(GRID_OUTER_MARGIN / 2.0)),
                |el| el.pb(px(GRID_GAP)),
            )
            .children(
                hashes
                    .into_iter()
                    .map(|ref hash| self.render_tile(hash, cx)),
            )
            .into_any_element()
    }

    fn render_thumb(&self, hash: &ImageHash) -> AnyElement {
        let source = self.peek_thumb_path(hash);

        match source {
            Some(path) => img(path)
                .size_full()
                .overflow_hidden()
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => Self::render_thumb_placeholder().into_any_element(),
        }
    }

    /// Render a clickable tile with context menu and loading placeholder
    fn render_tile(&mut self, hash: &ImageHash, cx: &mut Context<Self>) -> AnyElement {
        let size = px(self.tile_size);
        let is_bookmarked = self.bookmarks.contains(hash);
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
            .relative()
            .border_3()
            .border_color(gpui::transparent_black())
            .hover(|el| {
                if is_selected {
                    el
                } else {
                    el.border_color(cx.theme().border)
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
            .map(|tile| tile.relative().child(self.render_thumb(&hash)))
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
    fn image_context_menu(
        menu: gpui_component::menu::PopupMenu,
        hash: ImageHash,
        is_bookmarked: bool,
        page: Page,
        src_path: &Path,
    ) -> gpui_component::menu::PopupMenu {
        menu.check_side(gpui_component::Side::Right)
            .menu_with_icon(
                if is_bookmarked {
                    "Unbookmark"
                } else {
                    "Bookmark"
                },
                if is_bookmarked {
                    IconName::HeartOff
                } else {
                    IconName::Heart
                },
                Box::new(actions::Bookmark::Thumb(hash)),
            )
            .menu_with_icon(
                "Copy full path",
                IconName::Copy,
                Box::new(actions::CopyPathToClipboard::Thumb(hash)),
            )
            .separator()
            .when(page == Page::Bookmarks, |menu| {
                menu.menu_with_icon(
                    "Reveal in gallery",
                    IconName::GalleryVerticalEnd,
                    Box::new(actions::RevealInGallery(hash)),
                )
            })
            .menu_with_icon(
                format!("Open in {}", file_manager_label().to_lowercase()),
                IconName::FolderOpen,
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
        TabBar::new("navigation")
            .w_full()
            .selected_index(self.page.into())
            .px_2()
            .rounded_none()
            .on_click(cx.listener(|this, selected_index, _, cx| {
                this.page = Page::from(*selected_index);
                this.refresh(cx);
            }))
            .children(Page::get_pages().iter().map(|page| {
                Tab::new().px_2().child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child(page.get_icon()),
                        )
                        .child(page.get_name()),
                )
            }))
    }

    /// Render the toolbar with search input, image counts, and zoom controls
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_groupable = self.is_groupable(cx);

        let count_label = match self.page {
            Page::Gallery if self.view == View::Grouped => format!(
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
                .gap_4()
                .items_center()
                .child(
                    div()
                        .min_w_0()
                        .max_w(px(400.))
                        .w_full()
                        .child(Input::new(&self.input).cleanable(true).flex_1()),
                )
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(count_label),
                )
        };

        let sort_ascending = self.sort.ascending;

        let controls = || {
            h_flex()
                .flex_none()
                .items_center()
                .gap_2()
                .child(
                    h_flex()
                        .items_center()
                        .gap_1()
                        .child(
                            Toggle::new(View::Grouped)
                                .icon(IconName::Folder)
                                .checked(self.view == View::Grouped)
                                .disabled(!is_groupable)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.view = View::Grouped;
                                    this.refresh(cx);
                                })),
                        )
                        .child(
                            Toggle::new(View::Grid)
                                .icon(IconName::LayoutDashboard)
                                .checked(self.view == View::Grid)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.view = View::Grid;
                                    this.refresh(cx);
                                })),
                        )
                        .child(
                            Toggle::new(View::List)
                                .icon(IconName::GalleryVerticalEnd)
                                .checked(self.view == View::List)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.view = View::List;
                                    this.refresh(cx);
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .gap_px()
                        .child(
                            div()
                                .w(px(150.))
                                .child(Select::new(&self.sort_select).small()),
                        )
                        .child(
                            Button::new("sort-direction")
                                .ghost()
                                .small()
                                .icon(if sort_ascending {
                                    IconName::SortAscending
                                } else {
                                    IconName::SortDescending
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_sort_direction(cx);
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .gap_px()
                        .child(
                            Button::new("grid-zoom-out")
                                .ghost()
                                .small()
                                .icon(IconName::Minus)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.zoom_grid_out(cx);
                                })),
                        )
                        .child(
                            Button::new("grid-zoom-in")
                                .ghost()
                                .small()
                                .icon(IconName::Plus)
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

    /// Render the placeholder shown when no images match
    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No images found.")
    }

    /// Render the lightbox footer with position, name, size, and bookmark toggle
    fn render_info_bar(&self, hash: &ImageHash, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = self.get_image_entry(hash).expect("image should exist");
        let name = label_for(&self.roots, &entry.src_path);
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
                .min_w_20()
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
                .text_ellipsis()
                .text_overflow(gpui::TextOverflow::TruncateMiddle(
                    SharedString::new_static("…"),
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
                        .icon(IconName::Copy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.copy_path_to_clipboard(&hash, cx);
                        })),
                )
                .child(
                    Button::new("bookmark")
                        .ghost()
                        .icon(if is_bookmarked {
                            IconName::HeartOff
                        } else {
                            IconName::Heart
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
                .py_2()
                .px_3()
                .rounded_lg()
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

    /// Render the full-size image with nav arrows, layering the thumb under it while it loads
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
                .icon(IconName::ChevronLeft)
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
                .icon(IconName::ChevronRight)
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
    fn render_lightbox(&self, hash: &ImageHash, cx: &mut Context<Self>) -> impl IntoElement {
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
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close_lightbox(cx);
            }))
            .on_any_mouse_down(cx.listener(|_, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
            }))
            .on_scroll_wheel(cx.listener(|_, _: &ScrollWheelEvent, _, cx| {
                cx.stop_propagation();
            }))
            .cursor_default()
            .child(self.render_lightbox_content(hash, cx))
            .child(self.render_info_bar(hash, cx))
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
                            let thumb = this.render_thumb(&hash);

                            items.push(
                                div()
                                    .id(image.hash.to_string())
                                    .px(px(GRID_OUTER_MARGIN / 2.))
                                    .pb(px(GRID_GAP))
                                    .child(
                                        h_flex()
                                            .px_4()
                                            .py_2()
                                            .border_1()
                                            .rounded_md()
                                            .overflow_hidden()
                                            .border_color(cx.theme().border)
                                            .cursor_pointer()
                                            .child(div().size(px(100.)).child(thumb))
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

        if (cols_changed || tile_size_changed) && !self.images.is_empty() {
            self.set_layout(columns, tile_size, cx);
        }

        // Queue thumbnails for the visible rows; state set here is picked up when rows render below
        self.enqueue_visible(window, cx);

        v_flex()
            .key_context(super::CONTEXT_GALLERY)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_prev))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_open_lightbox))
            .on_action(cx.listener(Self::on_toggle_grouped))
            .on_action(cx.listener(Self::on_close))
            .on_action(cx.listener(Self::on_zoom_in))
            .on_action(cx.listener(Self::on_zoom_out))
            .on_action(cx.listener(Self::on_zoom_reset))
            .on_action(cx.listener(Self::on_toggle_bookmark))
            .on_action(cx.listener(Self::on_copy_path_to_clipboard))
            .on_action(cx.listener(Self::on_open_in_finder))
            .on_action(cx.listener(Self::on_reveal_in_gallery))
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_jump_to_top))
            .on_action(cx.listener(Self::on_jump_to_bottom))
            .on_action(cx.listener(Self::on_prev_page))
            .on_action(cx.listener(Self::on_next_page))
            .on_action(cx.listener(Self::on_toggle_collapse))
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_tab_bar(cx))
            .child(self.render_header(cx))
            .map(|el| {
                if self.filtered_images.is_empty() {
                    el.child(self.render_empty(cx))
                } else if self.view == View::List {
                    el.child(self.render_list(cx))
                } else {
                    el.child(self.render_grid(cx))
                }
            })
            .when_some(self.lightbox, |el, hash| {
                el.child(self.render_lightbox(&hash, cx))
            })
    }
}
