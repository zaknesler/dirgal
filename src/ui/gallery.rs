use crate::{
    hash::hash_path,
    image::{ImageEntry, SMALL_FILE_BYTES, format_bytes},
    path::{group_segments, label_for},
    ui::{
        actions,
        state::{AppState, SharedAppState},
    },
};
use gpui::{
    AnyElement, App, Context, Entity, FocusHandle, Focusable, ListAlignment, ListState, ObjectFit,
    ScrollWheelEvent, SharedString, Window, div, img, list, prelude::*, px,
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

const TILE_MIN: f32 = 200.0;
const GRID_GAP: f32 = 12.0;
const GRID_H_PADDING: f32 = 32.0;

const NUM_PAGES: usize = 2;

const GRID_CACHE_ITEMS: usize = 500;
const LIGHTBOX_CACHE_ITEMS: usize = 10;

const COLOR_ACCENT: u32 = 0xca3500;
const COLOR_BACKDROP: u32 = 0x0a0a0af0;

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct ImageHash(u64);

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct GroupHash(u64);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Gallery,
    Bookmarks,
}

impl From<Page> for usize {
    fn from(page: Page) -> Self {
        match page {
            Page::Gallery => 0,
            Page::Bookmarks => 1,
        }
    }
}

impl From<usize> for Page {
    fn from(index: usize) -> Self {
        match index {
            0 => Page::Gallery,
            1 => Page::Bookmarks,
            _ => unreachable!(),
        }
    }
}

#[derive(Clone)]
enum Row {
    Header(GroupHash),
    Tiles(Vec<ImageHash>),
}

struct Group {
    hash: GroupHash,
    path: PathBuf,
    image_hashes: Vec<ImageHash>,
}

#[derive(Clone, Copy)]
struct Job {
    image_hash: ImageHash,
    priority: JobPriority,
}

#[derive(Clone, Copy, PartialEq)]
enum JobPriority {
    Urgent,
    Deferred,
}

#[derive(Clone)]
enum ThumbState {
    Unknown,
    Queued,
    Generating,
    Ready(Arc<Path>),
    Failed,
}

pub struct Gallery {
    state: Entity<AppState>,

    // Navigation
    page: Page,
    focus_handle: FocusHandle,
    input: Entity<InputState>,
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
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = SharedAppState::from_app(cx).entity().clone();

        cx.observe(&state, |this, _, cx| {
            this.refresh(cx);
        })
        .detach();

        let snapshot = state.read(cx).clone();

        let queue: VecDeque<Job> = snapshot
            .images
            .iter()
            .filter(|e| e.bytes >= SMALL_FILE_BYTES)
            .map(|entry| Job {
                image_hash: ImageHash(entry.hash),
                priority: JobPriority::Deferred,
            })
            .collect();

        let concurrency = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(8);

        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));

        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle, cx);

        cx.subscribe_in(&input, window, Self::on_input_event)
            .detach();

        let image_index = snapshot
            .images
            .iter()
            .enumerate()
            .map(|(i, e)| (ImageHash(e.hash), i))
            .collect();

        // Prefill bookmarks from paths in config
        let bookmarks = snapshot
            .config
            .bookmarks
            .clone()
            .into_iter()
            .filter_map(|path| {
                snapshot
                    .images
                    .iter()
                    .find(|i| i.src_path.as_ref() == path)
                    .map(|image| ImageHash(image.hash))
            })
            .collect::<Vec<_>>();

        let mut this = Self {
            state,
            page: Page::Gallery,
            roots: snapshot.roots,
            images: snapshot.images,
            image_index,
            filtered_images: Vec::new(),
            groups: Vec::new(),
            thumbs: HashMap::new(),
            queue,
            num_concurrency: concurrency,
            input,
            focus_handle,
            num_running: 0,
            num_columns: 0,
            rows: Vec::new(),
            tile_size: TILE_MIN,
            grid: ListState::new(0, ListAlignment::Top, px(0.)),
            lightbox: None,
            collapsed_groups: HashSet::new(),
            column_override: None,
            bookmarks,
        };

        this.process_jobs(cx);
        this
    }

    fn candidate_images(&self) -> Vec<ImageHash> {
        match self.page {
            Page::Gallery => self.images.iter().map(|e| ImageHash(e.hash)).collect(),
            Page::Bookmarks => self.bookmarks.iter().cloned().collect(),
        }
    }

    fn compute_visible(&self, candidates: &[ImageHash], query: &str) -> Vec<ImageHash> {
        if query.is_empty() {
            return candidates.to_vec();
        }
        let query = query.to_lowercase();
        candidates
            .iter()
            .filter(|hash| {
                self.image_entry(hash)
                    .map(|e| e.src_path.to_string_lossy().to_lowercase().contains(&query))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    fn compute_groups(&self) -> Vec<Group> {
        // Get the parent directory for each image
        let hash_to_parent: HashMap<ImageHash, PathBuf> = self
            .images
            .iter()
            .map(|e| {
                let parent = e.src_path.parent().unwrap_or(Path::new("")).to_path_buf();
                (ImageHash(e.hash), parent)
            })
            .collect();

        let mut groups: Vec<Group> = Vec::new();

        for &hash in &self.filtered_images {
            let parent = hash_to_parent[&hash].clone();
            if let Some(last) = groups.last_mut()
                && last.path == parent
            {
                last.image_hashes.push(hash);
                continue;
            }

            groups.push(Group {
                hash: GroupHash(hash_path(&parent)),
                path: parent,
                image_hashes: vec![hash],
            });
        }

        groups
    }

    fn visible_position(&self, hash: &ImageHash) -> Option<usize> {
        self.filtered_images.iter().position(|&i| i == *hash)
    }

    fn image_entry(&self, hash: &ImageHash) -> Option<&ImageEntry> {
        self.image_index.get(hash).and_then(|i| self.images.get(*i))
    }

    fn tile_source(&mut self, hash: &ImageHash, cx: &mut Context<Self>) -> Option<Arc<Path>> {
        let state = self
            .thumbs
            .get(hash)
            .cloned()
            .unwrap_or(ThumbState::Unknown);

        let hash = *hash;

        match state {
            ThumbState::Ready(p) => Some(p),
            ThumbState::Failed => self.image_entry(&hash).map(|e| e.src_path.clone()),
            ThumbState::Queued | ThumbState::Generating => None,
            ThumbState::Unknown => {
                let entry = self.image_entry(&hash)?.clone();
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

    fn next_job(&mut self) -> Option<ImageHash> {
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

    fn process_jobs(&mut self, cx: &mut Context<Self>) {
        while self.num_running < self.num_concurrency {
            let Some(hash) = self.next_job() else { return };

            self.thumbs.insert(hash, ThumbState::Generating);
            let image = self.image_entry(&hash).expect("image should exist").clone();

            self.num_running += 1;

            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { image.generate_thumbnail().await })
                    .await;

                this.update(cx, |gallery, cx| {
                    gallery.num_running -= 1;
                    gallery.thumbs.insert(
                        hash,
                        match result {
                            Ok(()) => {
                                let p = gallery
                                    .image_entry(&hash)
                                    .expect("image should exist")
                                    .thumb_path
                                    .clone();
                                ThumbState::Ready(p)
                            }
                            Err(e) => {
                                let path = gallery
                                    .image_entry(&hash)
                                    .map(|e| e.src_path.display().to_string())
                                    .unwrap_or_default();
                                tracing::warn!(path, error = %e, "thumbnail generation failed");
                                ThumbState::Failed
                            }
                        },
                    );

                    gallery.process_jobs(cx);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    fn grid_layout(&self, window: &Window) -> (usize, f32) {
        let avail = window.viewport_size().width.as_f32() - GRID_H_PADDING;
        let cols = match self.column_override {
            Some(c) => c,
            None => (((avail + GRID_GAP) / (TILE_MIN + GRID_GAP)).floor() as usize).max(1),
        };

        let tile = ((avail - cols.saturating_sub(1) as f32 * GRID_GAP) / cols as f32).max(30.0);

        (cols, tile)
    }

    fn set_layout(&mut self, columns: usize, tile_size: f32, cx: &mut Context<Self>) {
        self.num_columns = columns;
        self.tile_size = tile_size;
        self.refresh(cx);
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value();
        let candidates = self.candidate_images();
        self.filtered_images = self.compute_visible(&candidates, &query);

        self.rows.clear();

        match self.page {
            Page::Gallery => {
                self.groups = self.compute_groups();
                for group in &self.groups {
                    self.rows.push(Row::Header(group.hash));
                    if !self.collapsed_groups.contains(&group.hash) {
                        for chunk in group.image_hashes.chunks(self.num_columns) {
                            self.rows.push(Row::Tiles(chunk.to_vec()));
                        }
                    }
                }
            }
            Page::Bookmarks => {
                self.groups.clear();
                for chunk in self.filtered_images.chunks(self.num_columns) {
                    self.rows.push(Row::Tiles(chunk.to_vec()));
                }
            }
        }

        self.grid = ListState::new(self.rows.len(), ListAlignment::Top, px(600.));
        cx.notify();
    }

    fn deprioritize(&mut self) {
        for job in &self.queue {
            if job.priority == JobPriority::Urgent
                && matches!(self.thumbs.get(&job.image_hash), Some(ThumbState::Queued))
            {
                self.thumbs.insert(job.image_hash, ThumbState::Unknown);
            }
        }

        self.queue.retain(|j| j.priority == JobPriority::Deferred);
    }

    fn open(&mut self, hash: &ImageHash, cx: &mut Context<Self>) {
        self.show_lightbox(hash, cx);
    }

    fn show_lightbox(&mut self, hash: &ImageHash, cx: &mut Context<Self>) {
        self.lightbox = Some(*hash);
        self.deprioritize();
        cx.notify();
    }

    fn close_lightbox(&mut self, cx: &mut Context<Self>) {
        self.lightbox = None;
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.filtered_images.is_empty() {
            return;
        }
        let Some(current) = self.lightbox else { return };

        let pos = self.visible_position(&current).unwrap_or(0) as isize;
        let new_pos = pos + delta;

        let len = self.filtered_images.len();
        let new_pos_index = new_pos.rem_euclid(len as isize) as usize;
        let next = self.filtered_images[new_pos_index];

        self.show_lightbox(&next, cx);
    }

    fn toggle_group(&mut self, group_hash: GroupHash, cx: &mut Context<Self>) {
        if !self.collapsed_groups.remove(&group_hash) {
            self.collapsed_groups.insert(group_hash);
        }

        self.refresh(cx);
    }

    fn on_bookmark_active(
        &mut self,
        _: &actions::Bookmark,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(hash) = self.lightbox {
            self.toggle_bookmark(&hash, cx);
        }

        if self.page == Page::Bookmarks {
            self.close_lightbox(cx);
        }
    }

    fn toggle_bookmark(&mut self, image_hash: &ImageHash, cx: &mut Context<Self>) {
        if let Some(index) = self.get_bookmark_index(image_hash) {
            self.bookmarks.remove(index);
        } else {
            self.bookmarks.push(*image_hash);
        }

        self.persist_bookmarks();
        self.refresh(cx);
    }

    fn persist_bookmarks(&self) {
        // Leave other bookmarks intact
        let loaded_paths: HashSet<&Path> =
            self.images.iter().map(|e| e.src_path.as_ref()).collect();

        let current: Vec<PathBuf> = self
            .bookmarks
            .iter()
            .filter_map(|hash| self.image_entry(hash))
            .map(|entry| entry.src_path.to_path_buf())
            .collect();

        // Keep bookmarks that already exist and add the current ones
        match crate::config::AppConfig::load() {
            Ok(mut config) => {
                config
                    .bookmarks
                    .retain(|p| !loaded_paths.contains(p.as_path()));
                config.bookmarks.extend(current);

                if let Err(e) = config.save() {
                    tracing::warn!(error = %e, "failed to save bookmarks to config");
                }
            }
            Err(e) => tracing::warn!(error = %e, "failed to load config for saving bookmarks"),
        }
    }

    fn on_collapse_all(
        &mut self,
        _: &actions::CollapseAll,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.collapsed_groups.is_empty() {
            self.collapsed_groups = self.groups.iter().map(|g| g.hash).collect();
        } else {
            self.collapsed_groups.clear();
        }

        self.refresh(cx);
    }

    fn on_open(&mut self, _: &actions::OpenLightbox, _: &mut Window, cx: &mut Context<Self>) {
        if self.filtered_images.len() == 0 {
            return;
        }

        let first = match self.page {
            Page::Gallery => self
                .filtered_images
                .iter()
                .find(|hash| {
                    let group = self
                        .groups
                        .iter()
                        .find(|g| g.image_hashes.contains(hash))
                        .expect("group should exist");
                    let is_collapsed = self.collapsed_groups.contains(&group.hash);
                    !is_collapsed
                })
                .copied(),
            Page::Bookmarks => self.filtered_images.first().copied(),
        };

        if let Some(hash) = first {
            self.show_lightbox(&hash, cx);
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

    fn on_prev_page(&mut self, _: &actions::PrevPage, _: &mut Window, cx: &mut Context<Self>) {
        let current_index: usize = self.page.into();
        let last_page = (current_index + NUM_PAGES - 1) % NUM_PAGES;

        self.page = Page::from(last_page);
        self.refresh(cx);
    }

    fn on_next_page(&mut self, _: &actions::NextPage, _: &mut Window, cx: &mut Context<Self>) {
        let current_index: usize = self.page.into();
        let next_page = (current_index + 1) % NUM_PAGES;

        self.page = Page::from(next_page);
        self.refresh(cx);
    }

    fn zoom_grid_in(&mut self, cx: &mut Context<Self>) {
        let current = self.column_override.unwrap_or(self.num_columns);
        self.column_override = Some((current - 1).max(1));
        cx.notify();
    }

    fn zoom_grid_out(&mut self, cx: &mut Context<Self>) {
        let current = self.column_override.unwrap_or(self.num_columns);
        self.column_override = Some((current + 1).min(20));
        cx.notify();
    }

    fn get_bookmark_index(&self, image_hash: &ImageHash) -> Option<usize> {
        self.bookmarks.iter().position(|h| h == image_hash)
    }

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

    fn render_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(index).cloned() else {
            return div().into_any_element();
        };

        match row {
            Row::Header(group_hash) => {
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
                    .when(!is_collapsed, |el| el.pb_4())
                    .when(index != 0, |el| el.pt_4())
                    .cursor_pointer()
                    .group("header")
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_group(group_hash, cx)))
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
                                this.toggle_group(group_hash, cx);
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
                    .child(Tag::new().small().child(format!("{count}")))
                    .into_any_element()
            }
            Row::Tiles(hashes) => h_flex()
                .w_full()
                .gap_2()
                .pb_2()
                .children(
                    hashes
                        .into_iter()
                        .map(|ref hash| self.render_thumb(hash, cx)),
                )
                .into_any_element(),
        }
    }

    fn render_thumb(&mut self, hash: &ImageHash, cx: &mut Context<Self>) -> AnyElement {
        let source = self.tile_source(hash, cx);
        let tile = px(self.tile_size);

        let hash = *hash;

        div()
            .key_context(super::CONTEXT_GALLERY)
            .id(hash.0 as usize)
            .flex_none()
            .w(tile)
            .h(tile)
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .hover(|s| s.border_color(gpui::rgb(COLOR_ACCENT)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.open(&hash, cx)
            }))
            .context_menu(move |this, _, _| {
                this.check_side(gpui_component::Side::Right)
                    .menu_with_icon("Bookmark", IconName::Heart, Box::new(actions::Bookmark))
                    .separator()
                    .menu_with_icon(
                        "Open in Finder",
                        IconName::FolderOpen,
                        Box::new(actions::Quit),
                    )
            })
            .map(|tile| match source {
                Some(path) => tile.child(
                    img(path)
                        .size_full()
                        .rounded_md()
                        .overflow_hidden()
                        .object_fit(ObjectFit::Cover),
                ),
                None => tile
                    .relative()
                    .child(Skeleton::new().secondary().w_full().h_full())
                    .child(
                        v_flex()
                            .size_full()
                            .absolute()
                            .inset_0()
                            .items_center()
                            .justify_center()
                            .child(Spinner::new().large()),
                    ),
            })
            .into_any_element()
    }

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
            .child(Tab::new().px_2().label("Gallery"))
            .child(Tab::new().px_2().label("Bookmarks"))
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count_label = match self.page {
            Page::Gallery => format!(
                "{} images in {} folders",
                self.filtered_images.len(),
                self.groups.len()
            ),
            Page::Bookmarks => format!("{} bookmarked images", self.filtered_images.len()),
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

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No images found.")
    }

    fn render_info_bar(&self, hash: &ImageHash, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = self.image_entry(hash).expect("image should exist");
        let name = label_for(&self.roots, &entry.src_path);
        let bytes = format_bytes(entry.bytes);

        let position = self.visible_position(hash).map(|p| p + 1).unwrap_or(0);
        let counter = format!("{} / {}", position, self.filtered_images.len());

        let counter = || {
            Tag::secondary()
                .flex_none()
                .w_20()
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
                .bg(gpui::rgba(0x171717e6))
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

    fn render_lightbox_content(
        &self,
        hash: &ImageHash,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entry = self.image_entry(hash).expect("image should exist");
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

        let image_view = || {
            div()
                .id("image-area")
                .relative()
                .flex_1()
                .min_h_0()
                .size_full()
                .overflow_hidden()
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
            .child(image_view())
            .child(next_button(cx))
    }

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
            .on_click(cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
            }))
            .on_scroll_wheel(cx.listener(|_, _: &ScrollWheelEvent, _, cx| {
                cx.stop_propagation();
            }))
            .cursor_default()
            .child(self.render_lightbox_content(hash, cx))
            .child(self.render_info_bar(hash, cx))
    }

    fn render_grid(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let grid = self.grid.clone();

        div()
            .image_cache(super::cache::simple_lru_cache(
                super::CONTEXT_GRID,
                GRID_CACHE_ITEMS,
            ))
            .flex_1()
            .min_h_0()
            .p_4()
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
                    .child(Scrollbar::vertical(&grid)),
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
        let (columns, tile_size) = self.grid_layout(window);

        if (columns != self.num_columns || (tile_size - self.tile_size).abs() > 0.5)
            && !self.images.is_empty()
        {
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
            .on_action(cx.listener(Self::on_bookmark_active))
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
