use crate::ui::{gallery::Gallery, model::*, *};
use crate::{assets::IconAsset, core::util};
use gpui::{App, Context, FocusHandle, Focusable, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Icon, Root,
    button::{Button, ButtonVariants as _, Toggle, ToggleGroup, ToggleVariants},
    h_flex,
    input::Input,
    select::Select,
    tab::{Tab, TabBar},
    v_flex,
};

impl Gallery {
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
                util::format_num(self.grouped_view.read(cx).group_count())
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
                                    this.zoom_view_out(cx);
                                })),
                        )
                        .child(
                            Button::new("grid-zoom-in")
                                .ghost()
                                .icon(IconAsset::Plus)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.zoom_view_in(cx);
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
        let should_hide = self.selected_images.is_empty() || self.lightbox.is_some();
        let num_selected = self.selected_images.len();
        let selected_hashes = self.selected_content_hashes();
        let all_bookmarked = !selected_hashes.is_empty()
            && selected_hashes
                .iter()
                .all(|hash| self.library.bookmarks.contains(hash));
        let selection_label = format!(
            "{} {} selected",
            num_selected,
            if num_selected == 1 { "image" } else { "images" }
        );

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
                    .child(selection_label)
                    .child(
                        h_flex()
                            .flex_none()
                            .gap_1()
                            .child(
                                Button::new("bulk-copy-path")
                                    .ghost()
                                    .icon(IconAsset::Copy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.copy_selected_paths_to_clipboard(cx);
                                    })),
                            )
                            .child(
                                Button::new("bulk-bookmark")
                                    .ghost()
                                    .icon(if all_bookmarked {
                                        IconAsset::BookmarkOff
                                    } else {
                                        IconAsset::Bookmark
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.toggle_selected_bookmarks(cx);
                                    })),
                            )
                            .child(
                                Button::new("bulk-trash")
                                    .ghost()
                                    .icon(IconAsset::Trash)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.on_trash_file(
                                            &actions::TrashFile::Current,
                                            window,
                                            cx,
                                        );
                                    })),
                            )
                            .child(
                                Button::new("bulk-delete")
                                    .ghost()
                                    .icon(IconAsset::CircleX)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.on_delete_file(
                                            &actions::DeleteFile::Current,
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    ),
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
}

impl Focusable for Gallery {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                } else {
                    match self.settings.view {
                        View::Grid => el.child(self.grid_view.clone()),
                        View::Grouped => el.child(self.grouped_view.clone()),
                        View::List => el.child(self.list_view.clone()),
                    }
                }
            })
            .when_some(self.lightbox_image_id().cloned(), |el, id| {
                el.child(self.render_lightbox(&id, cx))
            })
            .child(self.render_floating_actions(cx))
            .children(notif_layer)
            .children(dialog_layer)
    }
}
