use crate::{
    hash::hash_path,
    image::{ImageEntry, SMALL_FILE_BYTES, format_bytes},
    path::{compare_paths_grouped, group_segments, label_for},
    ui::model::*,
    ui::*,
    util,
};
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, ListAlignment, ListOffset, ListState,
    MouseDownEvent, ObjectFit, ScrollWheelEvent, SharedString, Window, div, img, list, prelude::*,
    px, rems,
};
use gpui_component::{
    ActiveTheme, IconName, Sizable as _,
    breadcrumb::Breadcrumb,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::ContextMenuExt,
    scroll::Scrollbar,
    skeleton::Skeleton,
    spinner::Spinner,
    tab::{Tab, TabBar},
    tag::Tag,
    v_flex,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Overlays source paths on tiles when enabled
const DEBUG: bool = false;

/// Minimum tile width in pixels before adding another column
const TILE_MIN: f32 = 200.0;
/// Spacing between tiles in pixels
const GRID_GAP: f32 = 6.0;
/// Total horizontal padding around the grid in pixels
const GRID_OUTER_MARGIN: f32 = 32.0;

/// Number of navigable pages (gallery, bookmarks)
const NUM_PAGES: usize = 2;

/// Max images retained in the grid's LRU image cache
const GRID_CACHE_ITEMS: usize = 300;
/// Max images retained in the lightbox's LRU image cache
const LIGHTBOX_CACHE_ITEMS: usize = 10;

/// Hover highlight color for tile borders
const COLOR_ACCENT: u32 = 0xca3500;
/// Semi-transparent backdrop color behind the lightbox
const COLOR_BACKDROP: u32 = 0x0a0a0af0;

/// Main gallery view: grid of thumbnails, search, bookmarks, and lightbox
pub struct Gallery {
    state: Entity<state::AppState>,

    // Navigation
    page: Page,
    focus_handle: FocusHandle,
    input: Entity<InputState>,
    input_focus_handle: FocusHandle,
    lightbox: Option<ImageHash>,

    // Data
    roots: Vec<PathBuf>,
    images: Vec<ImageEntry>,
    image_index: HashMap<ImageHash, usize>,
    filtered_images: Vec<ImageHash>,
    rows: Vec<Row>,
    groups: Vec<Group>,
    collapsed_groups: HashSet<GroupHash>,
    bookmarks: Vec<ImageHash>,

    // Grid
    grid: ListState,
    tile_size: f32,
    num_columns: usize,
    column_override: Option<usize>,

    // Thumbnails
    thumbs: HashMap<ImageHash, ThumbState>,
    queue: VecDeque<Job>,
    num_running: usize,
    num_concurrency: usize,
}

impl Gallery {
    /// Create the gallery entity
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    /// Build the gallery from app state and seed the deferred thumbnail queue
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = state::SharedAppState::from_app(cx).entity().clone();

        cx.observe(&state, |this, _, cx| {
            this.refresh(cx);
        })
        .detach();

        let snapshot = state.read(cx).clone();

        let images = Self::normalize_images(snapshot.images);

        let queue: VecDeque<Job> = images
            .iter()
            .filter(|e| e.bytes >= SMALL_FILE_BYTES)
            .map(|entry| Job {
                image_hash: ImageHash(entry.hash),
                priority: JobPriority::Deferred,
            })
            .collect();

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

        let image_index = images
            .iter()
            .enumerate()
            .map(|(i, e)| (ImageHash(e.hash), i))
            .collect();

        let bookmarks = Self::bookmarks_from_config(&snapshot.config.bookmarks, &images);

        let mut this = Self {
            state,
            page: Page::Gallery,
            focus_handle,
            input,
            input_focus_handle,
            lightbox: None,
            roots: snapshot.roots,
            images,
            image_index,
            filtered_images: Vec::new(),
            rows: Vec::new(),
            groups: Vec::new(),
            collapsed_groups: HashSet::new(),
            bookmarks,
            grid: ListState::new(0, ListAlignment::Top, px(600.)),
            tile_size: TILE_MIN,
            num_columns: 1,
            column_override: None,
            thumbs: HashMap::new(),
            queue,
            num_running: 0,
            num_concurrency,
        };

        this.process_jobs(cx);
        this
    }

    /// Dedup by content hash (keep last), then sort so each directory's images are contiguous
    fn normalize_images(images: Vec<ImageEntry>) -> Vec<ImageEntry> {
        let mut seen = HashSet::new();
        let mut images: Vec<ImageEntry> = images
            .into_iter()
            .rev()
            .filter(|e| seen.insert(e.hash))
            .collect();

        images.sort_by(|a, b| compare_paths_grouped(&a.src_path, &b.src_path));
        images
    }

    /// Resolve configured bookmark hashes against loaded images, dropping unknowns
    fn bookmarks_from_config(hashes: &[u64], images: &[ImageEntry]) -> Vec<ImageHash> {
        let known = hashes.iter().copied().collect::<HashSet<u64>>();

        images
            .iter()
            .filter(|e| known.contains(&e.hash))
            .map(|e| ImageHash(e.hash))
            .collect()
    }

    /// Hashes for the current page, filtered by a case-insensitive path search
    fn get_visible_hashes(&self, query: &str) -> Vec<ImageHash> {
        let candidates: Vec<ImageHash> = match self.page {
            Page::Gallery => self.images.iter().map(|e| ImageHash(e.hash)).collect(),
            Page::Bookmarks => self.bookmarks.clone(),
        };

        if query.is_empty() {
            return candidates;
        }

        let query = query.to_lowercase();

        candidates
            .into_iter()
            .filter(|hash| {
                self.get_image_entry(hash)
                    .map(|e| e.src_path.to_string_lossy().to_lowercase().contains(&query))
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Group filtered images by parent directory (contiguous since `images` is pre-sorted)
    fn get_computed_groups(&self) -> Vec<Group> {
        let mut groups: Vec<Group> = Vec::new();

        for &hash in &self.filtered_images {
            let parent = self
                .get_image_entry(&hash)
                .and_then(|e| e.src_path.parent())
                .unwrap_or(Path::new(""));

            match groups.last_mut() {
                Some(group) if group.path == parent => group.image_hashes.push(hash),
                _ => groups.push(Group {
                    hash: GroupHash(hash_path(parent)),
                    path: parent.to_path_buf(),
                    image_hashes: vec![hash],
                }),
            }
        }

        groups
    }

    /// Index of an image within the current filtered set
    fn get_visible_position(&self, hash: &ImageHash) -> Option<usize> {
        self.filtered_images.iter().position(|&i| i == *hash)
    }

    /// Look up an image entry by content hash
    fn get_image_entry(&self, hash: &ImageHash) -> Option<&ImageEntry> {
        let hash = self.image_index.get(hash)?;
        self.images.get(*hash)
    }

    /// Displayable path for a thumbnail, enqueueing generation on first request
    fn get_thumb_path(&mut self, hash: &ImageHash, cx: &mut Context<Self>) -> Option<Arc<Path>> {
        let state = self
            .thumbs
            .get(hash)
            .cloned()
            .unwrap_or(ThumbState::Unknown);

        let hash = *hash;

        match state {
            ThumbState::Ready(p) => Some(p),
            ThumbState::Failed => self.get_image_entry(&hash).map(|e| e.src_path.clone()),
            ThumbState::Queued | ThumbState::Generating => None,
            ThumbState::Unknown => {
                let entry = self.get_image_entry(&hash)?.clone();
                if entry.bytes < SMALL_FILE_BYTES {
                    self.thumbs
                        .insert(hash, ThumbState::Ready(entry.src_path.clone()));
                    Some(entry.src_path)
                } else if entry.thumb_path.exists() {
                    self.thumbs
                        .insert(hash, ThumbState::Ready(entry.thumb_path.clone()));
                    Some(entry.thumb_path)
                } else {
                    self.thumbs.insert(hash, ThumbState::Queued);
                    self.queue.push_front(Job {
                        image_hash: hash,
                        priority: JobPriority::Urgent,
                    });
                    self.process_jobs(cx);
                    None
                }
            }
        }
    }

    /// Pop queued jobs until one is still live for its priority, skipping stale entries
    fn get_next_job(&mut self) -> Option<ImageHash> {
        loop {
            let Job {
                image_hash: image,
                priority,
            } = self.queue.pop_front()?;
            let state = self.thumbs.get(&image).unwrap_or(&ThumbState::Unknown);

            let live = match priority {
                JobPriority::Urgent => matches!(state, ThumbState::Queued),
                JobPriority::Deferred => matches!(state, ThumbState::Unknown),
            };

            if live {
                return Some(image);
            }
        }
    }

    /// Compute column count and tile size from the viewport width
    fn get_grid_layout(&self, window: &Window) -> (usize, f32) {
        let avail = window.viewport_size().width.as_f32() - GRID_OUTER_MARGIN;
        let cols = match self.column_override {
            Some(c) => c,
            None => (((avail + GRID_GAP) / (TILE_MIN + GRID_GAP)).floor() as usize).max(1),
        };

        let tile = ((avail - cols.saturating_sub(1) as f32 * GRID_GAP) / cols as f32).max(30.0);

        (cols, tile)
    }

    /// Spawn background thumbnail jobs up to the concurrency limit
    fn process_jobs(&mut self, cx: &mut Context<Self>) {
        while self.num_running < self.num_concurrency {
            let Some(hash) = self.get_next_job() else {
                return;
            };

            self.thumbs.insert(hash, ThumbState::Generating);
            let image = self
                .get_image_entry(&hash)
                .expect("image should exist")
                .clone();

            self.num_running += 1;

            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { image.generate_thumbnail() })
                    .await;

                this.update(cx, |gallery, cx| gallery.on_job_finished(hash, result, cx))
                    .ok();
            })
            .detach();
        }
    }

    /// Record a job's outcome, then pull more work from the queue
    fn on_job_finished(
        &mut self,
        hash: ImageHash,
        result: crate::error::AppResult<()>,
        cx: &mut Context<Self>,
    ) {
        self.num_running -= 1;

        let state = match result {
            Ok(_) => {
                let entry = self.get_image_entry(&hash).expect("image should exist");
                ThumbState::Ready(entry.thumb_path.clone())
            }
            Err(err) => {
                let path = self
                    .get_image_entry(&hash)
                    .map(|e| e.src_path.display().to_string())
                    .unwrap_or_default();
                tracing::warn!(path, error = %err, "thumbnail generation failed");
                ThumbState::Failed
            }
        };

        self.thumbs.insert(hash, state);
        self.process_jobs(cx);
        cx.notify();
    }

    /// Rebuild filtered images, groups, and rows for the current page and query
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value();
        self.filtered_images = self.get_visible_hashes(&query);

        let old_rows = std::mem::take(&mut self.rows);
        let cols = self.num_columns.max(1);

        match self.page {
            Page::Gallery => {
                self.groups = self.get_computed_groups();

                let mut offset = 0;
                for group in &self.groups {
                    self.rows.push(Row::Header(group.hash));
                    let len = group.image_hashes.len();
                    if !self.collapsed_groups.contains(&group.hash) {
                        self.rows.extend(Row::chunk_tiles(offset, len, cols));
                    }
                    offset += len;
                }
            }
            Page::Bookmarks => {
                self.groups.clear();
                self.rows
                    .extend(Row::chunk_tiles(0, self.filtered_images.len(), cols));
            }
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

    /// Drop urgent jobs back to unknown so grid work yields to the lightbox
    fn deprioritize(&mut self) {
        for job in &self.queue {
            let is_queued = matches!(self.thumbs.get(&job.image_hash), Some(ThumbState::Queued));
            if is_queued && job.priority == JobPriority::Urgent {
                self.thumbs.insert(job.image_hash, ThumbState::Unknown);
            }
        }

        self.queue.retain(|j| j.priority == JobPriority::Deferred);
    }

    /// Apply a new grid layout and rebuild rows to match
    fn set_layout(&mut self, columns: usize, tile_size: f32, cx: &mut Context<Self>) {
        self.num_columns = columns;
        self.tile_size = tile_size;
        self.refresh(cx);
    }

    /// Show the lightbox for an image and pause urgent grid thumbnail work
    fn open_lightbox(&mut self, hash: &ImageHash, cx: &mut Context<Self>) {
        self.lightbox = Some(*hash);
        self.deprioritize();
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
        let Some(current) = self.lightbox else { return };

        let pos = self.get_visible_position(&current).unwrap_or(0) as isize;
        let new_pos = pos + delta;

        let len = self.filtered_images.len();
        let new_pos_index = new_pos.rem_euclid(len as isize) as usize;
        let next = self.filtered_images[new_pos_index];

        self.open_lightbox(&next, cx);
    }

    /// Collapse or expand a directory group
    fn toggle_group(&mut self, group_hash: &GroupHash, cx: &mut Context<Self>) {
        if !self.collapsed_groups.remove(group_hash) {
            self.collapsed_groups.insert(*group_hash);
        }

        self.refresh(cx);
    }

    /// Toggle a bookmark from the lightbox or a thumbnail context menu
    fn on_bookmark(&mut self, action: &actions::Bookmark, _: &mut Window, cx: &mut Context<Self>) {
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
    fn on_open_in_finder(
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
    fn on_reveal_in_gallery(
        &mut self,
        action: &actions::RevealInGallery,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let actions::RevealInGallery(hash) = action;

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
        self.refresh(cx);

        if let Some(row_ix) = self.get_visible_position(hash).and_then(|pos| {
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

        cx.notify();
    }

    /// Move keyboard focus to the search input
    fn on_focus_search(
        &mut self,
        _: &actions::FocusSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input_focus_handle.focus(window, cx);
    }

    /// Add or remove a bookmark and persist the change
    fn toggle_bookmark(&mut self, image_hash: &ImageHash, cx: &mut Context<Self>) {
        if let Some(index) = self.get_bookmark_index(image_hash) {
            self.bookmarks.remove(index);
        } else {
            self.bookmarks.push(*image_hash);
        }

        self.persist_bookmarks(cx);
        self.refresh(cx);
    }

    /// Sync bookmarks into the shared config and save it to disk
    fn persist_bookmarks(&mut self, cx: &mut Context<Self>) {
        let current: HashSet<u64> = self.bookmarks.iter().map(|hash| hash.0).collect();
        let loaded: HashSet<u64> = self.images.iter().map(|image| image.hash).collect();

        // Merge into config, only touching loaded hashes to retain directories' bookmark
        self.state.update(cx, |state, _cx| {
            state
                .config
                .bookmarks
                .retain(|h| !loaded.contains(h) || current.contains(h));

            for hash in &self.bookmarks {
                if !state.config.bookmarks.contains(&hash.0) {
                    state.config.bookmarks.push(hash.0);
                }
            }
        });

        self.bookmarks =
            Self::bookmarks_from_config(&self.state.read(cx).config.bookmarks, &self.images);

        cx.notify();

        if let Err(e) = self.state.read(cx).config.save() {
            tracing::warn!(error = %e, "failed to save bookmarks to config");
        }
    }

    /// Collapse every group, or expand all if everything is already collapsed
    fn on_collapse_all(
        &mut self,
        _: &actions::CollapseAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.page == Page::Bookmarks {
            return;
        }

        if self.collapsed_groups.len() == self.groups.len() {
            self.collapsed_groups.clear();
        } else {
            self.collapsed_groups = self.groups.iter().map(|g| g.hash).collect();
        }

        self.refresh(cx);
    }

    /// Open the lightbox at the first visible image on the current page
    fn on_open(&mut self, _: &actions::OpenLightbox, _: &mut Window, cx: &mut Context<Self>) {
        if self.filtered_images.is_empty() {
            return;
        }

        let first = match self.page {
            Page::Gallery => self
                .groups
                .iter()
                .find(|g| !self.collapsed_groups.contains(&g.hash))
                .and_then(|g| g.image_hashes.first())
                .copied(),
            Page::Bookmarks => self.filtered_images.first().copied(),
        };

        if let Some(hash) = first {
            self.open_lightbox(&hash, cx);
        }
    }

    fn on_close(&mut self, _: &actions::CloseLightbox, _: &mut Window, cx: &mut Context<Self>) {
        self.close_lightbox(cx);
    }

    fn on_zoom_in(&mut self, _: &actions::ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_grid_in(cx);
    }

    fn on_zoom_out(&mut self, _: &actions::ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_grid_out(cx);
    }

    fn on_zoom_reset(&mut self, _: &actions::ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.column_override = None;
        cx.notify();
    }

    fn on_prev(&mut self, _: &actions::Prev, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn on_next(&mut self, _: &actions::Next, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    /// Cycle to the previous page, wrapping around
    fn on_prev_page(&mut self, _: &actions::PrevPage, _: &mut Window, cx: &mut Context<Self>) {
        let current_index: usize = self.page.into();
        let last_page = (current_index + NUM_PAGES - 1) % NUM_PAGES;

        self.page = Page::from(last_page);
        self.refresh(cx);
    }

    /// Cycle to the next page, wrapping around
    fn on_next_page(&mut self, _: &actions::NextPage, _: &mut Window, cx: &mut Context<Self>) {
        let current_index: usize = self.page.into();
        let next_page = (current_index + 1) % NUM_PAGES;

        self.page = Page::from(next_page);
        self.refresh(cx);
    }

    /// Enlarge tiles by removing a column, down to a minimum of one
    fn zoom_grid_in(&mut self, cx: &mut Context<Self>) {
        let current = self.column_override.unwrap_or(self.num_columns);
        self.column_override = Some((current - 1).max(1));
        cx.notify();
    }

    /// Shrink tiles by adding a column, up to a maximum of twenty
    fn zoom_grid_out(&mut self, cx: &mut Context<Self>) {
        let current = self.column_override.unwrap_or(self.num_columns);
        self.column_override = Some((current + 1).min(20));
        cx.notify();
    }

    /// Position of an image in the bookmark list, if bookmarked
    fn get_bookmark_index(&self, image_hash: &ImageHash) -> Option<usize> {
        self.bookmarks.iter().position(|h| h == image_hash)
    }

    /// Re-filter the gallery as the search input changes
    fn on_input_event(
        &mut self,
        _: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change | InputEvent::PressEnter { .. } => {
                cx.stop_propagation();
                self.refresh(cx);
            }
            _ => {}
        };
    }

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
                    .map(|ref hash| self.render_thumb(hash, cx)),
            )
            .into_any_element()
    }

    /// Render a clickable thumbnail tile with context menu and loading placeholder
    fn render_thumb(&mut self, hash: &ImageHash, cx: &mut Context<Self>) -> AnyElement {
        let source = self.get_thumb_path(hash, cx);
        let size = px(self.tile_size);

        let hash = *hash;

        let is_bookmarked = self.bookmarks.contains(&hash);
        let page = self.page;
        let src_path = self
            .get_image_entry(&hash)
            .map(|e| e.src_path.to_path_buf())
            .expect("image should exist");

        let path_str = src_path.to_string_lossy().to_string();

        div()
            .key_context(super::CONTEXT_GALLERY)
            .id(hash.0 as usize)
            .flex_none()
            .size(size)
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .relative()
            .border_color(cx.theme().border)
            .hover(|s| s.border_color(gpui::rgb(COLOR_ACCENT)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.open_lightbox(&hash, cx)
            }))
            .context_menu(move |menu, _, _| {
                Self::thumb_context_menu(menu, hash, is_bookmarked, page, &src_path)
            })
            .map(|tile| match source {
                Some(path) => tile.child(
                    img(path)
                        .size_full()
                        .rounded_md()
                        .overflow_hidden()
                        .object_fit(ObjectFit::Cover),
                ),
                None => tile.relative().child(Self::thumb_placeholder()),
            })
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

    /// Build the right-click menu for a thumbnail
    fn thumb_context_menu(
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
            .separator()
            .when(page == Page::Bookmarks, |menu| {
                menu.menu_with_icon(
                    "Reveal in Gallery",
                    IconName::GalleryVerticalEnd,
                    Box::new(actions::RevealInGallery(hash)),
                )
            })
            .menu_with_icon(
                "Reveal in Finder",
                IconName::FolderOpen,
                Box::new(actions::OpenInFinder::Path(src_path.to_path_buf())),
            )
    }

    /// Skeleton with a spinner shown while a thumbnail loads
    fn thumb_placeholder() -> impl IntoElement {
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
            .child(
                Tab::new().px_2().child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child(IconName::GalleryVerticalEnd),
                        )
                        .child("Gallery"),
                ),
            )
            .child(
                Tab::new().px_2().child(
                    h_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child(IconName::Heart),
                        )
                        .child("Bookmarks"),
                ),
            )
    }

    /// Render the toolbar with search input, image counts, and zoom controls
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count_label = match self.page {
            Page::Gallery => format!(
                "{} images in {} folders",
                util::format_num(self.filtered_images.len()),
                util::format_num(self.groups.len())
            ),
            Page::Bookmarks => format!(
                "{} bookmarked images",
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

        let controls = || {
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

        let prev_button = |cx: &mut Context<'_, Self>| {
            Button::new("prev-arrow")
                .ghost()
                .large()
                .px_8()
                .py_16()
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
                .px_8()
                .py_16()
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

    /// Render the virtualized image grid with its scrollbar
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

        v_flex()
            .key_context(super::CONTEXT_GALLERY)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_prev))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_open))
            .on_action(cx.listener(Self::on_close))
            .on_action(cx.listener(Self::on_zoom_in))
            .on_action(cx.listener(Self::on_zoom_out))
            .on_action(cx.listener(Self::on_zoom_reset))
            .on_action(cx.listener(Self::on_bookmark))
            .on_action(cx.listener(Self::on_open_in_finder))
            .on_action(cx.listener(Self::on_reveal_in_gallery))
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_prev_page))
            .on_action(cx.listener(Self::on_next_page))
            .on_action(cx.listener(Self::on_collapse_all))
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
                    el.child(self.render_grid(cx))
                }
            })
            .when_some(self.lightbox, |el, hash| {
                el.child(self.render_lightbox(&hash, cx))
            })
    }
}
