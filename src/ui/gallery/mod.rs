use crate::core::{
    config::Settings,
    image::{ContentHash, ImageEntry, ImageId, SMALL_FILE_BYTES},
    store::Store,
};
use crate::ui::{model::*, *};
use gpui::{App, ClipboardItem, Context, Entity, FocusHandle, Focusable, Window, prelude::*};
use gpui_component::{IndexPath, input::InputState, select::SelectState};
use library::Library;
use lightbox::Lightbox;
use std::path::Path;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
};
use view::{GalleryView, GridView, GroupedView, ListView, ScrollTarget};

pub mod constant;
pub mod handler;
pub mod library;
pub mod lightbox;
pub mod render;
pub mod view;

/// Main gallery view: grid of thumbnails, search, bookmarks, and lightbox
pub struct Gallery {
    state: Entity<state::AppState>,

    // Navigation
    page: Page,
    focus_handle: FocusHandle,
    input: Entity<InputState>,
    input_focus_handle: FocusHandle,
    lightbox: Option<Lightbox>,
    settings: Settings,
    sort_select: Entity<SelectState<Vec<String>>>,

    // Data
    library: Library,
    filtered_images: Vec<ImageId>,
    grid_view: Entity<GridView>,
    grouped_view: Entity<GroupedView>,
    list_view: Entity<ListView>,
    active_image: Option<ImageId>,
    selected_images: Vec<ImageId>,

    // Thumbnails
    thumbs: HashMap<ContentHash, ThumbState>,
    queue: VecDeque<ContentHash>,
    num_running: usize,
    num_concurrency: usize,
}

impl Gallery {
    /// Create the gallery entity
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    /// Build the gallery from app state; thumbnails are queued lazily as rows enter the viewport
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = state::SharedAppState::from_app(cx).entity().clone();

        cx.observe(&state, |this, _, cx| {
            this.reflow(cx);
        })
        .detach();

        let config = state.read(cx).config.clone();
        let settings = config.settings;
        let sort = settings.sort();

        let num_concurrency = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(8);

        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
        let input_focus_handle = input.focus_handle(cx);

        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        cx.subscribe_in(&input, window, Self::on_input_event)
            .detach();

        let sort_select = cx.new(|cx| {
            SelectState::new(
                SortKey::ALL
                    .iter()
                    .map(|(_, l)| l.to_string())
                    .collect::<Vec<_>>(),
                Some(IndexPath::new(sort.key.index())),
                window,
                cx,
            )
        });
        cx.subscribe_in(&sort_select, window, Self::on_sort)
            .detach();

        let gallery = cx.entity().downgrade();
        let grid_view = cx.new(|cx| GridView::new(gallery.clone(), cx));
        let grouped_view = cx.new(|cx| GroupedView::new(gallery.clone(), cx));
        let list_view = cx.new(|cx| ListView::new(gallery, cx));

        let mut this = Self {
            state,
            page: config.page,
            focus_handle,
            input,
            input_focus_handle,
            lightbox: None,
            settings,
            sort_select,
            library: Library::empty(),
            filtered_images: Vec::new(),
            grid_view,
            grouped_view,
            list_view,
            active_image: None,
            selected_images: Vec::new(),
            thumbs: HashMap::new(),
            queue: VecDeque::new(),
            num_running: 0,
            num_concurrency,
        };

        this.reload_from_state(cx);
        this
    }

    /// Set the current page
    fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.selected_images.clear();
        self.active_image = None;
        self.close_lightbox(cx);
        self.reflow(cx);
    }

    /// Apply the given settings, updating the view and sorting if needed
    fn apply_settings(&mut self, new: Settings, window: &mut Window, cx: &mut Context<Self>) {
        let sort = new.sort();
        let should_resort = sort != self.settings.sort();

        self.settings = new;

        if should_resort {
            let bookmarks = self.state.read(cx).scanner.bookmarks.clone();
            self.library.resort(sort, &bookmarks);
        }

        // Keep the toolbar dropdown in sync when the sort is changed from elsewhere
        let index = IndexPath::new(sort.key.index());
        if self.sort_select.read(cx).selected_index(cx) != Some(index) {
            self.sort_select.update(cx, |select, cx| {
                select.set_selected_index(Some(index), window, cx)
            });
        }

        self.reflow(cx);
    }

    /// Apply a new sort
    fn set_sort(&mut self, sort: Sort, window: &mut Window, cx: &mut Context<Self>) {
        let mut new = self.settings;
        new.set_sort(sort);
        self.apply_settings(new, window, cx);
    }

    /// Apply a new view
    fn set_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_settings(
            Settings {
                view,
                ..self.settings
            },
            window,
            cx,
        );
    }

    /// Toggle sort direction from the toolbar button
    fn toggle_sort_direction(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sort = Sort {
            ascending: !self.settings.sort().ascending,
            ..self.settings.sort()
        };

        self.set_sort(sort, window, cx);
    }

    /// Toggle directory grouping where off flows all images flat like the bookmarks list
    fn toggle_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = match self.settings.view {
            View::Grid => View::Grouped,
            View::Grouped => View::List,
            View::List => View::Grid,
        };

        self.set_view(view, window, cx);
    }

    /// Apply a new thumbnail fit
    fn set_thumbnail_fit(
        &mut self,
        thumbnail_fit: ThumbnailFit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.apply_settings(
            Settings {
                thumbnail_fit,
                ..self.settings
            },
            window,
            cx,
        );
    }

    /// Image IDs for the current page in sort order filtered by a case insensitive path search
    fn get_visible_image_ids(&self, query: &str) -> Vec<ImageId> {
        let candidates: Vec<ImageId> = match self.page {
            Page::Gallery => self
                .library
                .images
                .iter()
                .map(|entry| entry.id.clone())
                .collect(),
            Page::Bookmarks => self
                .library
                .images
                .iter()
                .filter(|entry| self.library.bookmarks.contains(&entry.content_hash))
                .map(|entry| entry.id.clone())
                .collect(),
            Page::Duplicates => self
                .library
                .duplicates
                .iter()
                .map(|entry| entry.id.clone())
                .collect(),
        };

        if query.is_empty() {
            return candidates;
        }

        let query = query.to_lowercase();
        let keywords: HashSet<&str> = query.split_whitespace().collect();

        let mut matches: Vec<ImageId> = Vec::new();

        for id in candidates {
            if let Some(image) = self.get_image_entry(&id) {
                let path = image.id.path().to_string_lossy().to_lowercase();

                // Must contain all keywords
                if keywords.iter().all(|k| path.contains(k)) {
                    matches.push(id);
                }
            }
        }

        matches
    }

    /// Index of an image within the current filtered set
    fn get_visible_position(&self, id: &ImageId) -> Option<usize> {
        self.filtered_images.iter().position(|item| item == id)
    }

    /// Look up an image entry by file identity
    fn get_image_entry(&self, id: &ImageId) -> Option<&ImageEntry> {
        self.library.get(id)
    }

    /// Get displayable path for a thumbnail from already-known state, without triggering generation
    fn peek_thumb_path(&self, id: &ImageId) -> Option<Arc<Path>> {
        let entry = self.get_image_entry(id)?;
        match self.thumbs.get(&entry.content_hash) {
            Some(ThumbState::Ready(p)) => Some(p.clone()),
            Some(ThumbState::Failed) => Some(entry.id.clone_path()),
            _ => None,
        }
    }

    /// Resolve or queue a thumbnail for a single image, returning true if its state changed
    fn enqueue_thumb(&mut self, id: &ImageId) -> bool {
        let Some(entry) = self.get_image_entry(id).cloned() else {
            return false;
        };
        let hash = entry.content_hash;

        if !matches!(self.thumbs.get(&hash), None | Some(ThumbState::Unknown)) {
            return false;
        }

        if entry.bytes < SMALL_FILE_BYTES {
            self.thumbs
                .insert(hash, ThumbState::Ready(entry.id.clone_path()));
        } else if entry.thumb_path.exists() {
            self.thumbs
                .insert(hash, ThumbState::Ready(entry.thumb_path.clone()));
        } else {
            self.thumbs.insert(hash, ThumbState::Queued);
            self.queue.push_back(hash);
        }

        true
    }

    /// Queue thumbnails for the given images
    fn enqueue_thumbnails(&mut self, image_ids: &[ImageId], cx: &mut Context<Self>) {
        let mut changed = false;
        for id in image_ids {
            changed |= self.enqueue_thumb(id);
        }
        if changed {
            self.process_queue(cx);
        }
    }

    /// Queue thumbnails near the grouped viewport
    fn enqueue_grouped_thumbnails(&mut self, visible_indices: Vec<usize>, cx: &mut Context<Self>) {
        let visible_ids: Vec<ImageId> = visible_indices
            .into_iter()
            .map(|index| self.filtered_images[index].clone())
            .collect();
        let visible: HashSet<ContentHash> = visible_ids
            .iter()
            .filter_map(|id| self.get_image_entry(id).map(|entry| entry.content_hash))
            .collect();

        // Cancel jobs for rows that have scrolled out of view before they start
        let stale: Vec<ContentHash> = self
            .queue
            .iter()
            .filter(|hash| !visible.contains(hash))
            .copied()
            .collect();
        for hash in stale {
            if matches!(self.thumbs.get(&hash), Some(ThumbState::Queued)) {
                self.thumbs.insert(hash, ThumbState::Unknown);
            }
        }
        self.queue.retain(|hash| visible.contains(hash));

        let mut changed = false;
        for hash in visible {
            if let Some(entry) = self.library.primary_for_content(&hash) {
                let id = entry.id.clone();
                changed |= self.enqueue_thumb(&id);
            }
        }

        if changed {
            self.process_queue(cx);
        }
    }

    /// Pop queued jobs until one is still pending, skipping stale entries
    fn next_queued_thumb(&mut self) -> Option<ContentHash> {
        loop {
            let image = self.queue.pop_front()?;
            if matches!(self.thumbs.get(&image), Some(ThumbState::Queued)) {
                return Some(image);
            }
        }
    }

    /// Spawn background thumbnail jobs up to the concurrency limit
    fn process_queue(&mut self, cx: &mut Context<Self>) {
        while self.num_running < self.num_concurrency {
            let Some(hash) = self.next_queued_thumb() else {
                return;
            };

            self.thumbs.insert(hash, ThumbState::Generating);
            let Some(image) = self.library.primary_for_content(&hash).cloned() else {
                self.thumbs.remove(&hash);
                continue;
            };

            self.num_running += 1;

            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { image.generate_thumbnail() })
                    .await;

                this.update(cx, |gallery, cx| {
                    gallery.handle_thumb_generation(hash, result, cx)
                })
                .ok();
            })
            .detach();
        }
    }

    /// Record a job's outcome, then pull more work from the queue
    fn handle_thumb_generation(
        &mut self,
        hash: ContentHash,
        result: crate::error::AppResult<()>,
        cx: &mut Context<Self>,
    ) {
        self.num_running -= 1;

        let state = match result {
            Ok(_) => {
                let Some(entry) = self.library.primary_for_content(&hash) else {
                    self.thumbs.remove(&hash);
                    self.process_queue(cx);
                    cx.notify();
                    return;
                };
                ThumbState::Ready(entry.thumb_path.clone())
            }
            Err(err) => {
                let path = self
                    .library
                    .primary_for_content(&hash)
                    .map(|entry| entry.id.path().display().to_string())
                    .unwrap_or_default();
                tracing::warn!(path, error = %err, "thumbnail generation failed");
                ThumbState::Failed
            }
        };

        self.thumbs.insert(hash, state);
        self.process_queue(cx);
        cx.notify();
    }

    /// Rebuild the library from the current scanner state
    fn rebuild_library_from_state(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.state.read(cx).clone();
        self.library = Library::from_state(snapshot, self.settings.sort());
    }

    /// Clear the library, reload from the scanner in current state, and refresh the UI
    fn reload_from_state(&mut self, cx: &mut Context<Self>) {
        self.rebuild_library_from_state(cx);
        self.reflow(cx);
    }

    /// Remove source paths from scanner state, then rebuild state/index
    fn remove_paths_from_library(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let removed_ids = self.remove_paths_from_scanner(paths, cx);
        self.rebuild_library_from_state(cx);
        self.prune_removed_images(&removed_ids);
        self.reflow(cx);
    }

    /// Remove source paths from scanner state and return their image IDs
    fn remove_paths_from_scanner(
        &mut self,
        paths: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> HashSet<ImageId> {
        self.state
            .update(cx, |state, _| state.scanner.remove_paths(paths))
    }

    /// Clear transient UI state associated with removed or no longer loaded images
    fn prune_removed_images(&mut self, removed_ids: &HashSet<ImageId>) {
        let library = &self.library;
        self.selected_images
            .retain(|id| !removed_ids.contains(id) && library.contains_id(id));
        if self
            .active_image
            .as_ref()
            .is_some_and(|id| removed_ids.contains(id) || !library.contains_id(id))
        {
            self.active_image = None;
        }
        if self
            .lightbox_image_id()
            .is_some_and(|id| removed_ids.contains(id) || !library.contains_id(id))
        {
            self.lightbox = None;
        }

        self.queue.retain(|hash| library.contains_content(hash));
        self.thumbs.retain(|hash, _| library.contains_content(hash));
    }

    /// Refresh the library in the background
    fn refresh_library(&mut self, cx: &mut Context<Self>) {
        let state = self.state.clone();

        cx.spawn(async move |this, cx| {
            let mut scanner = state.read_with(cx, |state, _| state.scanner.clone());

            let result = cx
                .background_executor()
                .spawn(async move {
                    scanner.rescan()?;
                    crate::error::AppResult::Ok(scanner)
                })
                .await;

            match result {
                Ok(scanner) => {
                    state.update(cx, |state, cx| {
                        state.scanner = scanner;
                        cx.notify();
                    });

                    this.update(cx, |gallery, cx| gallery.reload_from_state(cx))
                        .ok();
                }
                Err(err) => {
                    tracing::warn!(error = %err, "refresh failed");
                }
            }
        })
        .detach();
    }

    /// Rebuild filtered images and grouped state for the current page and query
    fn reflow(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value();
        let filtered = self.get_visible_image_ids(&query);
        self.filtered_images = filtered;
        self.grouped_view.update(cx, |view, cx| {
            view.rebuild(&self.filtered_images, &self.library);
            cx.notify();
        });
        cx.notify();
    }

    /// Cancel all grid thumbnail generation
    fn cancel_pending_thumbs(&mut self) {
        for hash in &self.queue {
            if matches!(self.thumbs.get(hash), Some(ThumbState::Queued)) {
                self.thumbs.insert(*hash, ThumbState::Unknown);
            }
        }

        self.queue.clear();
    }

    /// Mark the given image as selected, deselecting any other items
    fn select_single_image(&mut self, id: &ImageId, cx: &mut Context<Self>) {
        self.selected_images.clear();
        self.selected_images.push(id.clone());
        self.active_image = Some(id.clone());
        cx.notify();
    }

    /// Clear the current grid selection
    fn clear_selection(&mut self, cx: &mut Context<Self>) {
        self.selected_images.clear();
        self.active_image = None;
        cx.notify();
    }

    /// Add the given image to the current selection
    fn add_image_to_selection(&mut self, id: &ImageId, cx: &mut Context<Self>) {
        if !self.selected_images.contains(id) {
            self.selected_images.push(id.clone());
        }
        self.active_image = Some(id.clone());
        cx.notify();
    }

    /// Remove the given image from the current selection
    fn remove_image_from_selection(&mut self, id: &ImageId, cx: &mut Context<Self>) {
        if let Some(index) = self.selected_images.iter().position(|item| item == id) {
            self.selected_images.swap_remove(index);
            self.active_image = Some(id.clone());
            cx.notify();
        }
    }

    /// Add all images between the active image and the given image to the selection
    fn add_images_until_selection(&mut self, id: &ImageId, cx: &mut Context<Self>) {
        if let Some(index) = self.filtered_images.iter().position(|item| item == id) {
            if let Some(active_image) = &self.active_image {
                let active_index = self
                    .filtered_images
                    .iter()
                    .position(|item| item == active_image)
                    .unwrap_or(0);

                let range = if active_index > index {
                    index..=active_index
                } else {
                    active_index..=index
                };
                let additions: Vec<ImageId> = self.filtered_images[range]
                    .iter()
                    .filter(|id| !self.selected_images.contains(id))
                    .cloned()
                    .collect();
                self.selected_images.extend(additions);

                self.active_image = Some(id.clone());

                cx.notify();
            } else {
                self.select_single_image(id, cx);
            }
        }
    }

    /// Reveal the given image within the current view
    fn scroll_to_image(&mut self, id: &ImageId, cx: &mut Context<Self>) {
        self.scroll_view(ScrollTarget::Image(id.clone()), cx);
    }

    /// Scroll the active view to a target
    fn scroll_view(&mut self, target: ScrollTarget, cx: &mut Context<Self>) {
        let image_ids = self.filtered_images.clone();
        match self.settings.view {
            View::Grid => self.grid_view.update(cx, |view, cx| {
                view.scroll_to(&image_ids, target);
                cx.notify();
            }),
            View::Grouped => self.grouped_view.update(cx, |view, cx| {
                view.scroll_to(&image_ids, target);
                cx.notify();
            }),
            View::List => self.list_view.update(cx, |view, cx| {
                view.scroll_to(&image_ids, target);
                cx.notify();
            }),
        }
    }

    /// Show the lightbox with the given image and stop generating thumbnails
    fn open_lightbox(&mut self, id: &ImageId, cx: &mut Context<Self>) {
        let dimensions = self.get_image_entry(id).and_then(|entry| entry.dimensions);

        self.lightbox = Some(Lightbox::new(id.clone(), dimensions));
        self.cancel_pending_thumbs();
        cx.notify();
    }

    /// Dismiss the lightbox
    fn close_lightbox(&mut self, cx: &mut Context<Self>) {
        self.lightbox = None;
        cx.notify();
    }

    /// Move the lightbox by delta within the filtered set, wrapping at the ends
    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.filtered_images.is_empty() {
            return;
        }
        let Some(current) = self.lightbox_image_id().cloned() else {
            return;
        };

        let pos = self.get_visible_position(&current).unwrap_or(0) as isize;
        let new_pos = pos + delta;

        let len = self.filtered_images.len();
        let new_pos_index = new_pos.rem_euclid(len as isize) as usize;
        let next = self.filtered_images[new_pos_index].clone();

        self.open_lightbox(&next, cx);
    }

    /// Select the next image in the given direction
    fn select_adjacent_image(&mut self, direction: view::Direction, cx: &mut Context<Self>) {
        let next = match self.settings.view {
            View::Grid => self.grid_view.read(cx).neighbor(
                &self.filtered_images,
                self.active_image.as_ref(),
                direction,
            ),
            View::Grouped => self.grouped_view.read(cx).neighbor(
                &self.filtered_images,
                self.active_image.as_ref(),
                direction,
            ),
            View::List => self.list_view.read(cx).neighbor(
                &self.filtered_images,
                self.active_image.as_ref(),
                direction,
            ),
        };

        if let Some(next) = next {
            self.select_single_image(&next, cx);
            self.scroll_to_image(&next, cx);
        }
    }

    /// Add or remove a bookmark and persist the change
    fn toggle_bookmark(&mut self, content_hash: &ContentHash, cx: &mut Context<Self>) {
        if let Some(index) = self.get_bookmark_index(content_hash) {
            self.library.bookmarks.remove(index);
        } else {
            self.library.bookmarks.push(*content_hash);
        }

        self.persist_bookmarks(cx);
        self.reflow(cx);
    }

    /// Add/remove all selected image as bookmarks
    fn toggle_selected_bookmarks(&mut self, cx: &mut Context<Self>) {
        let content_hashes = self.selected_content_hashes();
        if content_hashes.is_empty() {
            return;
        }

        let all_bookmarked = content_hashes
            .iter()
            .all(|hash| self.library.bookmarks.contains(hash));

        if all_bookmarked {
            self.library
                .bookmarks
                .retain(|hash| !content_hashes.contains(hash));
        } else {
            for hash in content_hashes {
                if !self.library.bookmarks.contains(&hash) {
                    self.library.bookmarks.push(hash);
                }
            }
        }

        // Only clear the selected bookmarks on the bookmarks page (cause they no longer exist there)
        if self.page == Page::Bookmarks {
            self.selected_images.clear();
            self.active_image = None;
        }

        self.persist_bookmarks(cx);
        self.reflow(cx);
    }

    /// Sync bookmarks into the shared scanner state and persist to the store file
    fn persist_bookmarks(&mut self, cx: &mut Context<Self>) {
        let current: HashSet<u64> = self.library.bookmarks.iter().map(|hash| hash.0).collect();
        let loaded: HashSet<u64> = self
            .library
            .images
            .iter()
            .map(|image| image.content_hash.0)
            .collect();

        // Merge into scanner state, only touching loaded hashes to retain other directories' bookmarks
        self.state.update(cx, |state, _cx| {
            state
                .scanner
                .bookmarks
                .retain(|h| !loaded.contains(h) || current.contains(h));

            for hash in &self.library.bookmarks {
                if !state.scanner.bookmarks.contains(&hash.0) {
                    state.scanner.bookmarks.push(hash.0);
                }
            }
        });

        let bookmarks = self.state.read(cx).scanner.bookmarks.clone();
        self.library.bookmarks =
            crate::core::image::resolve_bookmarks(&bookmarks, &self.library.images);

        cx.notify();

        if let Err(err) = Store::save_bookmarks(&bookmarks) {
            tracing::warn!(?err, "failed to save bookmarks to store");
        }
    }

    /// Move files to the trash and remove them from the gallery
    fn trash_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let mut trashed = Vec::new();

        // Trash each file individually so we can track which ones succeeded
        for path in paths {
            match crate::core::path::trash_file(path) {
                Ok(()) => trashed.push(path.clone()),
                Err(err) => tracing::warn!(?err, ?path, "failed to trash file"),
            }
        }

        // TODO: better feedback?
        if !trashed.is_empty() {
            self.remove_paths_from_library(&trashed, cx);
        }
        self.clear_selection(cx);
    }

    /// Permanently delete files and remove successfully deleted paths from the gallery
    fn delete_files(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) {
        let mut deleted = Vec::new();

        // Delete each file individually so we can track which ones succeeded
        for path in paths {
            match std::fs::remove_file(path) {
                Ok(()) => deleted.push(path.clone()),
                Err(err) => tracing::warn!(?err, ?path, "failed to delete file"),
            }
        }

        // TODO: better feedback here too!
        if !deleted.is_empty() {
            self.remove_paths_from_library(&deleted, cx);
        }
        self.clear_selection(cx);
    }

    /// Resolve the open image or current selection to source paths
    fn current_image_paths(&self) -> Vec<PathBuf> {
        if let Some(id) = self.lightbox_image_id() {
            return vec![id.to_path_buf()];
        }

        self.selected_images
            .iter()
            .map(ImageId::to_path_buf)
            .collect()
    }

    /// Resolve selected files to unique content hashes in selection order
    fn selected_content_hashes(&self) -> Vec<ContentHash> {
        let mut seen = HashSet::new();

        self.selected_images
            .iter()
            .filter_map(|id| self.get_image_entry(id))
            .map(|entry| entry.content_hash)
            .filter(|hash| seen.insert(*hash))
            .collect()
    }

    /// Copy the path of the given image to the clipboard
    fn copy_path_to_clipboard(&mut self, path: &Path, cx: &mut Context<Self>) {
        let path = path.to_string_lossy().to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(path));
    }

    /// Copy the paths of all selected images to the clipboard
    fn copy_selected_paths_to_clipboard(&mut self, cx: &mut Context<Self>) {
        if self.selected_images.is_empty() {
            return;
        }

        let paths: Vec<String> = self
            .selected_images
            .iter()
            .map(|id| id.path().to_string_lossy().to_string())
            .collect();

        cx.write_to_clipboard(ClipboardItem::new_string(paths.join("\n")));
    }

    /// Enlarge thumbnails in the active view
    fn zoom_view_in(&mut self, cx: &mut Context<Self>) {
        match self.settings.view {
            View::Grid => self.grid_view.update(cx, |view, cx| {
                view.zoom_in();
                cx.notify();
            }),
            View::Grouped => self.grouped_view.update(cx, |view, cx| {
                view.zoom_in();
                cx.notify();
            }),
            View::List => self.list_view.update(cx, |view, cx| {
                view.zoom_in();
                cx.notify();
            }),
        }
    }

    /// Shrink thumbnails in the active view
    fn zoom_view_out(&mut self, cx: &mut Context<Self>) {
        match self.settings.view {
            View::Grid => self.grid_view.update(cx, |view, cx| {
                view.zoom_out();
                cx.notify();
            }),
            View::Grouped => self.grouped_view.update(cx, |view, cx| {
                view.zoom_out();
                cx.notify();
            }),
            View::List => self.list_view.update(cx, |view, cx| {
                view.zoom_out();
                cx.notify();
            }),
        }
    }

    /// Restore the active view's default thumbnail size
    fn reset_view_zoom(&mut self, cx: &mut Context<Self>) {
        match self.settings.view {
            View::Grid => self.grid_view.update(cx, |view, cx| {
                view.zoom_reset();
                cx.notify();
            }),
            View::Grouped => self.grouped_view.update(cx, |view, cx| {
                view.zoom_reset();
                cx.notify();
            }),
            View::List => self.list_view.update(cx, |view, cx| {
                view.zoom_reset();
                cx.notify();
            }),
        }
    }

    /// Position of an image in the bookmark list, if bookmarked
    fn get_bookmark_index(&self, content_hash: &ContentHash) -> Option<usize> {
        self.library
            .bookmarks
            .iter()
            .position(|hash| hash == content_hash)
    }
}
