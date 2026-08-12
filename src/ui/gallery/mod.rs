use crate::core::{
    config::Settings,
    hash::hash_path,
    image::{ContentHash, ImageEntry, ImageId, SMALL_FILE_BYTES},
    store::Store,
};
use crate::ui::{gallery::constant::*, model::*, *};
use gpui::{
    App, ClipboardItem, Context, Entity, FocusHandle, Focusable, ListAlignment, ListOffset,
    ListState, Window, prelude::*, px,
};
use gpui_component::{IndexPath, input::InputState, select::SelectState};
use library::Library;
use lightbox::Lightbox;
use std::path::Path;
use std::sync::Arc;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
};

pub mod constant;
pub mod handler;
pub mod library;
pub mod lightbox;
pub mod render;

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
    rows: Vec<Row>,
    groups: Vec<Group>,
    collapsed_groups: HashSet<GroupHash>,

    // Grid
    grid: ListState,
    tile_size: f32,
    num_columns: usize,
    column_override: Option<usize>,
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

        // Create a grid that is sized to show all of the items upon first load
        let grid = ListState::new(0, ListAlignment::Top, px(GRID_OVERDRAW)).measure_all();

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
            rows: Vec::new(),
            groups: Vec::new(),
            collapsed_groups: HashSet::new(),
            grid,
            tile_size: GRID_TILE_MIN,
            num_columns: 1,
            column_override: None,
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
        self.selected_images = Vec::new();
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

    /// Group filtered images by parent directory which is contiguous since filtered_images is parent sorted
    fn get_computed_groups(&self) -> Vec<Group> {
        let mut groups: Vec<Group> = Vec::new();

        for id in &self.filtered_images {
            let parent = self
                .get_image_entry(id)
                .and_then(|entry| entry.id.path().parent())
                .unwrap_or(Path::new(""));

            match groups.last_mut() {
                Some(group) if group.path == parent => group.image_ids.push(id.clone()),
                _ => groups.push(Group {
                    hash: GroupHash(hash_path(parent)),
                    path: parent.to_path_buf(),
                    image_ids: vec![id.clone()],
                }),
            }
        }

        groups
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

    /// Queue thumbnails for the rows in (or near) the viewport, dropping pending work that scrolled away
    fn enqueue_visible(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }

        let len = self.rows.len();
        let row_height = self.tile_size + GRID_GAP;
        let viewport = window.viewport_size().height.as_f32() + 2.0 * GRID_OVERDRAW;
        let count = (viewport / row_height).ceil() as usize + 1;

        // The scroll top can sit past the last row (e.g. after jumping to the bottom),
        // so anchor the window to the end in that case rather than covering nothing
        let anchor = self.grid.logical_scroll_top().item_ix.min(len);
        let start = anchor.min(len.saturating_sub(count));
        let end = (start + count).min(len);

        let visible: HashSet<ContentHash> = self.rows[start..end]
            .iter()
            .filter_map(|row| match row {
                Row::Tiles(range) => Some(self.filtered_images[range.clone()].to_vec()),
                Row::Header(_) => None,
            })
            .flatten()
            .filter_map(|id| self.get_image_entry(&id).map(|entry| entry.content_hash))
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

    /// Compute optimal column count and tile size from the viewport width
    fn get_grid_layout(&self, window: &Window) -> (usize, f32) {
        let avail = window.viewport_size().width.as_f32() - GRID_OUTER_MARGIN * 2.0;

        // Respect the user's chosen column count over the calculated count
        let cols = match self.column_override {
            Some(c) => c,
            None => (((avail + GRID_GAP) / (GRID_TILE_MIN + GRID_GAP)).floor() as usize).max(1),
        };

        let tile = ((avail - cols.saturating_sub(1) as f32 * GRID_GAP) / cols as f32).max(30.0);

        (cols, tile)
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

    /// Rebuild filtered images, groups, and rows for the current page and query
    fn reflow(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value();
        let mut filtered = self.get_visible_image_ids(&query);

        // Grouped view needs same directory images contiguous and a stable sort by parent
        // keeps their sort key order within each group intact
        if self.settings.view == View::Grouped {
            filtered.sort_by(
                |a, b| match (self.get_image_entry(a), self.get_image_entry(b)) {
                    (Some(x), Some(y)) => crate::core::image::compare_parents(x, y),
                    _ => std::cmp::Ordering::Equal,
                },
            );
        }
        self.filtered_images = filtered;

        let old_rows = std::mem::take(&mut self.rows);
        let cols = self.num_columns.max(1);

        if self.settings.view == View::Grouped {
            self.groups = self.get_computed_groups();

            let mut offset = 0;
            for group in &self.groups {
                self.rows.push(Row::Header(group.hash));
                let len = group.image_ids.len();
                if !self.collapsed_groups.contains(&group.hash) {
                    self.rows.extend(Row::chunk_tiles(offset, len, cols));
                }
                offset += len;
            }
        } else {
            self.groups.clear();
            self.rows
                .extend(Row::chunk_tiles(0, self.filtered_images.len(), cols));
        }

        self.splice_changed_rows(&old_rows);
        cx.notify();
    }

    /// Splice only the changed middle range into the list state to preserve scroll position
    fn splice_changed_rows(&mut self, old_rows: &[Row]) {
        let unchanged_head = std::iter::zip(old_rows, &self.rows)
            .take_while(|(a, b)| a == b)
            .count();

        let unchanged_tail = std::iter::zip(
            old_rows[unchanged_head..].iter().rev(),
            self.rows[unchanged_head..].iter().rev(),
        )
        .take_while(|(a, b)| a == b)
        .count();

        self.grid.splice(
            unchanged_head..old_rows.len() - unchanged_tail,
            self.rows.len() - unchanged_head - unchanged_tail,
        );
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

    /// Apply a new grid layout and rebuild rows to match
    fn set_layout(&mut self, columns: usize, tile_size: f32, cx: &mut Context<Self>) {
        self.num_columns = columns;
        self.tile_size = tile_size;
        self.reflow(cx);
    }

    /// Mark the given image as selected, deselecting any other items
    fn select_single_image(&mut self, id: &ImageId, cx: &mut Context<Self>) {
        self.selected_images.clear();
        self.selected_images.push(id.clone());
        self.active_image = Some(id.clone());
        cx.notify();
    }

    /// Add the given image to the current selection
    fn add_image_to_selection(&mut self, id: &ImageId, cx: &mut Context<Self>) {
        self.selected_images.push(id.clone());
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

                if active_index > index {
                    self.selected_images
                        .extend(self.filtered_images[index..=active_index].iter().cloned());
                } else {
                    self.selected_images
                        .extend(self.filtered_images[active_index..=index].iter().cloned());
                }

                self.active_image = Some(id.clone());

                cx.notify();
            } else {
                self.select_single_image(id, cx);
            }
        }
    }

    /// Reveal the given image within the current view
    fn scroll_to_image(&mut self, id: &ImageId) {
        // TODO: only "scroll" if it's not already in view

        if let Some(row_ix) = self.get_visible_position(id).and_then(|pos| {
            self.rows.iter().position(|row| match row {
                Row::Tiles(range) => range.contains(&pos),
                Row::Header(_) => false,
            })
        }) {
            self.grid.scroll_to(ListOffset {
                item_ix: row_ix,
                offset_in_item: px(0.),
            });
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

    /// Select the next or previous image in the filtered set
    fn select_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        // Only change single selections
        if self.selected_images.len() != 1 {
            return;
        }

        let selected_image = self
            .selected_images
            .first()
            .expect("image should be selected");

        let pos = self
            .get_visible_position(selected_image)
            .expect("image should exist") as isize;

        let next_index = (pos + delta).rem_euclid(self.filtered_images.len() as isize);

        let new_image = self.filtered_images[next_index as usize].clone();
        self.select_single_image(&new_image, cx);
        self.scroll_to_image(&new_image);
    }

    /// Collapse or expand a directory group
    fn toggle_group(&mut self, group_hash: &GroupHash, cx: &mut Context<Self>) {
        if !self.collapsed_groups.remove(group_hash) {
            self.collapsed_groups.insert(*group_hash);
        }

        self.reflow(cx);
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

        self.library.bookmarks = crate::core::image::resolve_bookmarks(
            &self.state.read(cx).scanner.bookmarks,
            &self.library.images,
        );

        cx.notify();

        let bookmarks: Vec<u64> = self.library.bookmarks.iter().map(|hash| hash.0).collect();
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

    /// Enlarge tiles by removing a column
    fn zoom_grid_in(&mut self, cx: &mut Context<Self>) {
        let current = self.column_override.unwrap_or(self.num_columns);
        self.column_override = Some((current - 1).max(MIN_COLS));
        cx.notify();
    }

    /// Shrink tiles by adding a column
    fn zoom_grid_out(&mut self, cx: &mut Context<Self>) {
        let current = self.column_override.unwrap_or(self.num_columns);
        self.column_override = Some((current + 1).min(MAX_COLS));
        cx.notify();
    }

    /// Position of an image in the bookmark list, if bookmarked
    fn get_bookmark_index(&self, content_hash: &ContentHash) -> Option<usize> {
        self.library
            .bookmarks
            .iter()
            .position(|hash| hash == content_hash)
    }
}
