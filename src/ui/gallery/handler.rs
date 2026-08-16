use crate::ui::{
    gallery::view::{Direction, GalleryViewEvent, GroupHash, ScrollTarget, SelectionMode},
    model::*,
    *,
};
use crate::{
    core::{
        hash::hash_path,
        image::ImageId,
        util::{self},
    },
    ui::gallery::Gallery,
};
use gpui::{Context, Entity, Window};
use gpui_component::WindowExt;
use gpui_component::{
    input::{InputEvent, InputState},
    select::{SelectEvent, SelectState},
};
use std::path::Path;

impl Gallery {
    /// React to a sort-field selection from the dropdown
    pub fn on_sort(
        &mut self,
        _: &Entity<SelectState<Vec<String>>>,
        event: &SelectEvent<Vec<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let SelectEvent::Confirm(Some(label)) = event else {
            return;
        };
        let Some(&(key, _)) = SortKey::ALL.iter().find(|(_, l)| *l == label.as_str()) else {
            return;
        };
        let sort = Sort {
            key,
            ..self.settings.sort()
        };
        self.set_sort(sort, window, cx);
    }

    /// Refresh the library
    pub fn on_refresh(
        &mut self,
        _: &actions::Refresh,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_library(cx);
    }

    pub fn on_apply_preset(
        &mut self,
        actions::ApplyPreset(key): &actions::ApplyPreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = self.state.read(cx).config.clone();

        // Slot 0 is the reset: bare config settings with no preset layered on
        let new = match key {
            0 => config.settings,
            _ => match config.presets.get(key) {
                Some(preset) => preset.with_defaults(config.settings),
                None => {
                    tracing::debug!(slot = key, "no preset bound to slot");
                    return;
                }
            },
        };

        self.apply_settings(new, window, cx);
    }

    /// Re-filter the gallery as the search input changes
    pub fn on_input_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change | InputEvent::PressEnter { .. } => {
                cx.stop_propagation();
                self.clear_selection(cx);
                self.reflow(cx);
            }
            _ => {}
        };
    }

    /// Handle an interaction emitted by a gallery view
    pub fn on_view_event(&mut self, event: &GalleryViewEvent, cx: &mut Context<Self>) {
        match event {
            GalleryViewEvent::SelectImage { id, mode } => match mode {
                SelectionMode::Toggle if self.selected_images.contains(id) => {
                    self.remove_image_from_selection(id, cx);
                }
                SelectionMode::Toggle => {
                    self.add_image_to_selection(id, cx);
                }
                SelectionMode::Extend => {
                    self.add_images_until_selection(id, cx);
                }
                SelectionMode::Replace => self.select_single_image(id, cx),
            },
            GalleryViewEvent::OpenImage(id) => self.open_lightbox(id, cx),
            GalleryViewEvent::VisibleImagesChanged(ids) => {
                self.update_visible_thumbnails(ids, cx);
            }
        }
    }

    pub fn on_up(&mut self, _: &actions::Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.lightbox.is_some() {
            return;
        }

        self.select_adjacent_image(Direction::Up, cx);
    }

    pub fn on_down(&mut self, _: &actions::Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.lightbox.is_some() {
            return;
        }

        self.select_adjacent_image(Direction::Down, cx);
    }

    pub fn on_left(&mut self, _: &actions::Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.lightbox.is_some() {
            self.step(-1, cx);
            return;
        }

        self.select_adjacent_image(Direction::Left, cx);
    }

    pub fn on_right(&mut self, _: &actions::Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.lightbox.is_some() {
            self.step(1, cx);
            return;
        }

        self.select_adjacent_image(Direction::Right, cx);
    }

    pub fn on_open_lightbox(
        &mut self,
        _: &actions::OpenLightbox,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Open the lightbox if there is a single selected image, or if there are filtered images to show
        if self.lightbox.is_some() || self.filtered_images.is_empty() {
            return;
        }

        // Open currently-selected image
        if self.selected_images.len() == 1 {
            let id = self.selected_images[0].clone();
            self.open_lightbox(&id, cx);
            return;
        }

        let first = self.current_view(cx).first_image(&self.filtered_images);

        if let Some(id) = first {
            self.open_lightbox(&id, cx);
        }
    }

    /// Toggle directory grouping
    pub fn on_togle_view(
        &mut self,
        _: &actions::ToggleView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_view(window, cx);
    }

    /// Toggle thumbnail fit
    pub fn on_toggle_thumbnail_fit(
        &mut self,
        _: &actions::ToggleThumbnailFit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let thumbnail_fit = match self.settings.thumbnail_fit {
            ThumbnailFit::Cover => ThumbnailFit::Contain,
            ThumbnailFit::Contain => ThumbnailFit::Cover,
        };

        self.set_thumbnail_fit(thumbnail_fit, window, cx);
    }

    pub fn on_close(&mut self, _: &actions::CloseLightbox, _: &mut Window, cx: &mut Context<Self>) {
        self.close_lightbox(cx);
    }

    pub fn on_minimize(
        &mut self,
        _: &actions::Minimize,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.minimize_window();
    }

    pub fn on_zoom_in(&mut self, _: &actions::ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        // Zoom the open image rather than the grid behind it
        if self.lightbox.is_some() {
            self.zoom_lightbox_in(cx);
            return;
        }

        self.zoom_view_in(cx);
    }

    pub fn on_zoom_out(&mut self, _: &actions::ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        // Zoom the open image rather than the grid behind it
        if self.lightbox.is_some() {
            self.zoom_lightbox_out(cx);
            return;
        }

        self.zoom_view_out(cx);
    }

    pub fn on_zoom_reset(
        &mut self,
        _: &actions::ZoomReset,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lightbox.is_some() {
            self.zoom_lightbox_reset(cx);
            return;
        }

        self.reset_view_zoom(cx);
    }

    /// Fill the lightbox area with the open image
    pub fn on_zoom_fill(&mut self, _: &actions::ZoomFill, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_lightbox_fill(cx);
    }

    /// Copy the full path to the clipboard
    pub fn on_copy_path_to_clipboard(
        &mut self,
        action: &actions::CopyPathToClipboard,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            actions::CopyPathToClipboard::Current => {
                if let Some(id) = self.lightbox_image_id().cloned() {
                    self.copy_path_to_clipboard(id.path(), cx);
                } else if !self.selected_images.is_empty() {
                    self.copy_selected_paths_to_clipboard(cx);
                }
            }
            actions::CopyPathToClipboard::Path(path) => {
                self.copy_path_to_clipboard(path, cx);
            }
        }
    }

    /// Copy the file(s) at the given path(s)
    pub fn on_copy_image(&mut self, _: &actions::CopyImage, _: &mut Window, _: &mut Context<Self>) {
        unimplemented!()
    }

    /// Trash the file(s) at the given path(s)
    pub fn on_trash_file(
        &mut self,
        action: &actions::TrashFile,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = match action {
            actions::TrashFile::Current => self.current_image_paths(),
            actions::TrashFile::Path(path) => vec![path.clone()],
        };

        if !paths.is_empty() {
            self.trash_files(&paths, cx);
        }
    }

    /// Permanently delete the file(s) at the given path(s)
    pub fn on_delete_file(
        &mut self,
        action: &actions::DeleteFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = match action {
            actions::DeleteFile::Current => self.current_image_paths(),
            actions::DeleteFile::Path(path) => vec![path.clone()],
        };
        if paths.is_empty() {
            return;
        }

        let count = paths.len();
        let gallery = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let gallery = gallery.clone();
            let paths = paths.clone();
            alert
                .title(format!(
                    "Permanently delete {count} {}?",
                    if count == 1 { "file" } else { "files" }
                ))
                .description(
                    "Are you sure you want to permanently delete? This action cannot be undone.",
                )
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    gallery
                        .update(cx, |gallery, cx| gallery.delete_files(&paths, cx))
                        .ok();
                    true
                })
        });
    }

    pub fn on_toggle_bookmark(
        &mut self,
        action: &actions::Bookmark,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_pos = self
            .lightbox_image_id()
            .and_then(|id| self.get_visible_position(id));

        match action {
            actions::Bookmark::Current => {
                if let Some(content_hash) = self
                    .lightbox_image_id()
                    .and_then(|id| self.get_image_entry(id))
                    .map(|entry| entry.content_hash)
                {
                    self.toggle_bookmark(&content_hash, cx);
                }
            }
            actions::Bookmark::Hash(hash) => {
                self.toggle_bookmark(hash, cx);
            }
        }

        // Go to next image on bookmarks page, or close the lightbox if there are no more images
        if self.page == Page::Bookmarks {
            if self.filtered_images.is_empty() {
                self.close_lightbox(cx);
            } else if let Some(current) = self.lightbox_image_id().cloned() {
                if self.get_visible_position(&current).is_some() {
                    self.step(1, cx);
                } else if let Some(pos) = old_pos {
                    // Current image was unbookmarked; the next one slid into its slot
                    let next = self.filtered_images[pos % self.filtered_images.len()].clone();
                    self.open_lightbox(&next, cx);
                }
            }
        }
    }

    /// Reveal an image's source file in the system file manager
    pub fn on_open_in_finder(
        &mut self,
        action: &actions::OpenInFinder,
        _: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let path = match action {
            actions::OpenInFinder::Current => self.lightbox_image_id().map(ImageId::to_path_buf),
            actions::OpenInFinder::Path(p) => Some(p.clone()),
        };

        if let Some(p) = path {
            util::reveal_in_file_manager(&p);
        }
    }

    /// Jump to an image on the gallery page, expanding its group and scrolling to its row
    pub fn on_reveal_in_gallery(
        &mut self,
        actions::RevealInGallery(hash): &actions::RevealInGallery,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.library.primary_for_content(hash) else {
            return;
        };
        let id = entry.id.clone();
        {
            let parent = entry
                .id
                .path()
                .parent()
                .unwrap_or(Path::new(""))
                .to_path_buf();
            self.grouped_view.update(cx, |view, _| {
                view.expand_group(GroupHash(hash_path(&parent)))
            });
        }

        self.page = Page::Gallery;
        self.close_lightbox(cx);
        self.select_single_image(&id, cx);
        self.reflow(cx);

        self.scroll_to_image(&id, cx);

        cx.notify();
    }

    /// Move keyboard focus to the search input
    pub fn on_focus_search(
        &mut self,
        _: &actions::FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input_focus_handle.focus(window, cx);
    }

    /// Jump the active view to the very top
    pub fn on_jump_to_top(
        &mut self,
        _: &actions::JumpToTop,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scroll_view(ScrollTarget::Start, cx);
    }

    /// Jump the active view to the very bottom
    pub fn on_jump_to_bottom(
        &mut self,
        _: &actions::JumpToBottom,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scroll_view(ScrollTarget::End, cx);
    }

    /// Cycle to the previous page, wrapping around
    pub fn on_prev_page(&mut self, _: &actions::PrevPage, _: &mut Window, cx: &mut Context<Self>) {
        let current_index = self.page.index();
        let total_pages = Page::ALL.len();
        let last_index = (current_index + total_pages - 1) % total_pages;

        self.set_page(Page::ALL[last_index].0, cx);
    }

    /// Cycle to the next page, wrapping around
    pub fn on_next_page(&mut self, _: &actions::NextPage, _: &mut Window, cx: &mut Context<Self>) {
        let current_index = self.page.index();
        let total_pages = Page::ALL.len();
        let next_index = (current_index + 1) % total_pages;

        self.set_page(Page::ALL[next_index].0, cx);
    }

    /// Collapse every group, or expand all if everything is already collapsed
    pub fn on_toggle_collapse(
        &mut self,
        _: &actions::CollapseAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_current_view(cx, |view| view.toggle_groups());
    }
}
