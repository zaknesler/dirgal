use crate::ui::{model::*, *};
use crate::{
    core::{
        hash::hash_path,
        util::{self},
    },
    ui::gallery::Gallery,
};
use gpui::{ClickEvent, Context, Entity, ListOffset, Window, px};
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
                self.reflow(cx);
            }
            _ => {}
        };
    }

    pub fn on_thumb_click_event(
        &mut self,
        hash: &ImageHash,
        event: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.modifiers().secondary() && self.selected_hashes.contains(hash) {
            self.remove_hash_from_selection(hash, cx);
        } else if event.modifiers().secondary() {
            self.add_hash_to_selection(hash, cx);
        } else if event.modifiers().shift {
            self.add_hashes_until_selection(hash, cx);
        } else {
            self.select_single_hash(hash, cx);
        }

        cx.notify();
    }

    pub fn on_up(&mut self, _: &actions::Up, window: &mut Window, cx: &mut Context<Self>) {
        if self.lightbox.is_some() {
            return;
        }

        let (num_columns, _) = self.get_grid_layout(window);
        self.select_step(-(num_columns as isize), cx);
    }

    pub fn on_down(&mut self, _: &actions::Down, window: &mut Window, cx: &mut Context<Self>) {
        if self.lightbox.is_some() {
            return;
        }

        let (num_columns, _) = self.get_grid_layout(window);
        self.select_step(num_columns as isize, cx);
    }

    pub fn on_left(&mut self, _: &actions::Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.lightbox.is_some() {
            self.step(-1, cx);
            return;
        }

        if self.selected_hashes.len() == 1 {
            self.select_step(-1, cx);
        }
    }

    pub fn on_right(&mut self, _: &actions::Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.lightbox.is_some() {
            self.step(1, cx);
            return;
        }

        if self.selected_hashes.len() == 1 {
            self.select_step(1, cx);
        }
    }

    pub fn on_open_lightbox(
        &mut self,
        _: &actions::OpenLightbox,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.filtered_images.is_empty() {
            return;
        }

        // Open currently-selected image
        if self.selected_hashes.len() == 1 {
            let hash = self.selected_hashes[0];
            self.open_lightbox(&hash, cx);
            return;
        }

        // Otherwise find the first image (if grouped, use the image from the first opened group)
        let first = if self.settings.view == View::Grouped {
            self.groups
                .iter()
                .find(|g| !self.collapsed_groups.contains(&g.hash))
                .and_then(|g| g.image_hashes.first())
                .copied()
        } else {
            self.filtered_images.first().copied()
        };

        if let Some(hash) = first {
            self.open_lightbox(&hash, cx);
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
        self.zoom_grid_in(cx);
    }

    pub fn on_zoom_out(&mut self, _: &actions::ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_grid_out(cx);
    }

    pub fn on_zoom_reset(
        &mut self,
        _: &actions::ZoomReset,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.column_override = None;
        cx.notify();
    }

    pub fn on_copy_path_to_clipboard(
        &mut self,
        action: &actions::CopyPathToClipboard,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            actions::CopyPathToClipboard::Current => {
                if let Some(hash) = self.lightbox {
                    self.copy_path_to_clipboard(&hash, cx);
                } else if !self.selected_hashes.is_empty() {
                    self.copy_selected_paths_to_clipboard(cx);
                }
            }
            actions::CopyPathToClipboard::Thumb(hash) => {
                self.copy_path_to_clipboard(hash, cx);
            }
        }
    }

    pub fn on_toggle_bookmark(
        &mut self,
        action: &actions::Bookmark,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_pos = self
            .lightbox
            .and_then(|hash| self.get_visible_position(&hash));

        match action {
            actions::Bookmark::Current => {
                if let Some(hash) = self.lightbox {
                    self.toggle_bookmark(&hash, cx);
                }
            }
            actions::Bookmark::Thumb(hash) => {
                self.toggle_bookmark(hash, cx);
            }
        }

        // Go to next image on bookmarks page, or close the lightbox if there are no more images
        if self.page == Page::Bookmarks {
            if self.filtered_images.is_empty() {
                self.close_lightbox(cx);
            } else if let Some(current) = self.lightbox {
                if self.get_visible_position(&current).is_some() {
                    self.step(1, cx);
                } else if let Some(pos) = old_pos {
                    // Current image was unbookmarked; the next one slid into its slot
                    let next = self.filtered_images[pos % self.filtered_images.len()];
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
            actions::OpenInFinder::Current => self
                .lightbox
                .and_then(|hash| self.get_image_entry(&hash))
                .map(|e| e.src_path.to_path_buf()),
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
        if let Some(entry) = self.get_image_entry(hash) {
            let parent = entry
                .src_path
                .parent()
                .unwrap_or(Path::new(""))
                .to_path_buf();
            self.collapsed_groups.remove(&GroupHash(hash_path(&parent)));
        }

        self.page = Page::Gallery;
        self.close_lightbox(cx);
        self.select_single_hash(hash, cx);
        self.reflow(cx);

        self.scroll_to_hash(hash);

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

    /// Jump the grid scroll position to the very top
    pub fn on_jump_to_top(
        &mut self,
        _: &actions::JumpToTop,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.grid.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.),
        });
        cx.notify();
    }

    /// Jump the grid scroll position to the very bottom
    pub fn on_jump_to_bottom(
        &mut self,
        _: &actions::JumpToBottom,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.grid.scroll_to_end();
        cx.notify();
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
        if self.settings.view != View::Grouped {
            return;
        }

        if self.collapsed_groups.len() == self.groups.len() {
            self.collapsed_groups.clear();
        } else {
            self.collapsed_groups = self.groups.iter().map(|g| g.hash).collect();
        }

        self.reflow(cx);
    }
}
