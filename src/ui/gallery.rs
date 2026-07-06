use crate::{
    image::{ImageEntry, SMALL_FILE_BYTES, format_bytes, generate_thumbnail},
    path::{group_segments, label_for},
    ui::actions,
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
    scroll::Scrollbar,
    separator::Separator,
    skeleton::Skeleton,
    spinner::Spinner,
    tag::Tag,
    v_flex,
};
use std::collections::{HashSet, VecDeque};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TILE_MIN: f32 = 200.0;
const GRID_GAP: f32 = 12.0;
const GRID_H_PADDING: f32 = 32.0;

const COLOR_ACCENT: u32 = 0xca3500;
const COLOR_BACKDROP: u32 = 0x0a0a0af0;

type ImageIndex = usize;
type VisiblePos = usize;

#[derive(Clone)]
enum Row {
    Header(usize),
    Tiles(Range<VisiblePos>),
}

struct Group {
    path: PathBuf,
    range: Range<VisiblePos>,
}

#[derive(Clone, Copy)]
struct Job {
    index: ImageIndex,
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
    roots: Vec<PathBuf>,
    images: Vec<ImageEntry>,
    visible: Vec<ImageIndex>,
    groups: Vec<Group>,
    rows: Vec<Row>,
    columns: usize,
    tile_size: f32,
    grid: ListState,
    thumbs: Vec<ThumbState>,
    queue: VecDeque<Job>,
    running: usize,
    concurrency: usize,
    viewer: Option<ImageIndex>,
    collapsed_groups: HashSet<usize>,
    column_override: Option<usize>,
    focus_handle: FocusHandle,
    input: Entity<InputState>,
}

impl Gallery {
    pub fn view(
        window: &mut Window,
        cx: &mut App,
        roots: Vec<PathBuf>,
        images: Vec<ImageEntry>,
    ) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx, roots, images))
    }

    fn new(
        window: &mut Window,
        cx: &mut Context<Self>,
        roots: Vec<PathBuf>,
        images: Vec<ImageEntry>,
    ) -> Self {
        let n = images.len();
        let thumbs = vec![ThumbState::Unknown; n];
        let queue: VecDeque<Job> = images
            .iter()
            .enumerate()
            .filter(|(_, e)| e.bytes >= SMALL_FILE_BYTES)
            .map(|(index, _)| Job {
                index,
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

        let mut this = Self {
            roots,
            images,
            visible: Vec::new(),
            groups: Vec::new(),
            thumbs,
            queue,
            concurrency,
            input,
            focus_handle,
            running: 0,
            columns: 0,
            rows: Vec::new(),
            tile_size: TILE_MIN,
            grid: ListState::new(0, ListAlignment::Top, px(0.)),
            viewer: None,
            collapsed_groups: HashSet::new(),
            column_override: None,
        };

        this.process_jobs(cx);
        this
    }

    fn visible_position(&self, index: ImageIndex) -> Option<VisiblePos> {
        self.visible.iter().position(|&i| i == index)
    }

    fn compute_visible(images: &[ImageEntry], query: &str) -> Vec<ImageIndex> {
        if query.is_empty() {
            return (0..images.len()).collect();
        }

        let query = query.to_lowercase();
        images
            .iter()
            .enumerate()
            .filter(|(_, e)| e.path.to_string_lossy().to_lowercase().contains(&query))
            .map(|(index, _)| index)
            .collect()
    }

    fn compute_groups(images: &[ImageEntry], visible: &[ImageIndex]) -> Vec<Group> {
        let mut groups = Vec::new();
        let mut start = 0;

        for i in 1..=visible.len() {
            let boundary = i == visible.len()
                || images[visible[i]].path.parent() != images[visible[start]].path.parent();
            if boundary {
                let path = images[visible[start]]
                    .path
                    .parent()
                    .unwrap_or(Path::new(""))
                    .to_path_buf();
                groups.push(Group {
                    path,
                    range: start..i,
                });
                start = i;
            }
        }

        groups
    }

    fn tile_source(&mut self, index: usize, cx: &mut Context<Self>) -> Option<Arc<Path>> {
        match &self.thumbs[index] {
            ThumbState::Ready(p) => Some(p.clone()),
            ThumbState::Failed => Some(self.images[index].path.clone()),
            ThumbState::Queued | ThumbState::Generating => None,
            ThumbState::Unknown => {
                let entry = self.images[index].clone();
                if entry.bytes < SMALL_FILE_BYTES {
                    self.thumbs[index] = ThumbState::Ready(entry.path.clone());
                    Some(entry.path)
                } else if entry.thumb.exists() {
                    self.thumbs[index] = ThumbState::Ready(entry.thumb.clone());
                    Some(entry.thumb)
                } else {
                    self.thumbs[index] = ThumbState::Queued;
                    self.queue.push_front(Job {
                        index,
                        priority: JobPriority::Urgent,
                    });
                    self.process_jobs(cx);
                    None
                }
            }
        }
    }

    /// Pops the next ready-to-generate index, skipping stale entries
    fn next_job(&mut self) -> Option<usize> {
        loop {
            let Job { index, priority } = self.queue.pop_front()?;
            let live = match priority {
                JobPriority::Urgent => matches!(self.thumbs[index], ThumbState::Queued),
                JobPriority::Deferred => matches!(self.thumbs[index], ThumbState::Unknown),
            };
            if live {
                return Some(index);
            }
        }
    }

    fn process_jobs(&mut self, cx: &mut Context<Self>) {
        while self.running < self.concurrency {
            let Some(index) = self.next_job() else { return };

            self.thumbs[index] = ThumbState::Generating;
            self.running += 1;
            let src = self.images[index].path.clone();
            let dst = self.images[index].thumb.clone();

            cx.spawn(async move |this, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { generate_thumbnail(&src, &dst).await })
                    .await;

                this.update(cx, |gallery, cx| {
                    gallery.running -= 1;
                    gallery.thumbs[index] = match result {
                        Ok(()) => ThumbState::Ready(gallery.images[index].thumb.clone()),
                        Err(e) => {
                            tracing::warn!(
                                path = %gallery.images[index].path.display(),
                                error = %e,
                                "thumbnail generation failed"
                            );
                            ThumbState::Failed
                        }
                    };

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

    fn reflow(&mut self, columns: usize, tile_size: f32, cx: &mut Context<Self>) {
        self.columns = columns;
        self.tile_size = tile_size;

        let query = self.input.read(cx).value();
        self.visible = Self::compute_visible(&self.images, &query);
        self.groups = Self::compute_groups(&self.images, &self.visible);

        self.rows.clear();
        for (group_index, group) in self.groups.iter().enumerate() {
            self.rows.push(Row::Header(group_index));

            if !self.collapsed_groups.contains(&group_index) {
                let mut start = group.range.start;
                while start < group.range.end {
                    let end = (start + columns).min(group.range.end);
                    self.rows.push(Row::Tiles(start..end));
                    start = end;
                }
            }
        }

        self.grid = ListState::new(self.rows.len(), ListAlignment::Top, px(600.));
    }

    fn deprioritize(&mut self) {
        for job in &self.queue {
            if job.priority == JobPriority::Urgent {
                if matches!(self.thumbs[job.index], ThumbState::Queued) {
                    self.thumbs[job.index] = ThumbState::Unknown;
                }
            }
        }
        self.queue.retain(|j| j.priority == JobPriority::Deferred);
    }

    fn open(&mut self, index: usize, cx: &mut Context<Self>) {
        self.show(index, cx);
    }

    /// Shows image `index` in the viewer
    fn show(&mut self, index: usize, cx: &mut Context<Self>) {
        self.viewer = Some(index);

        self.deprioritize();
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.viewer = None;

        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.visible.is_empty() {
            return;
        }
        let Some(current) = self.viewer else { return };

        let pos = self.visible_position(current).unwrap_or(0) as isize;
        let len = self.visible.len() as isize;
        let next = self.visible[(pos + delta).rem_euclid(len) as usize];
        self.show(next, cx);
    }

    fn toggle_group(&mut self, group_index: usize, cx: &mut Context<Self>) {
        if !self.collapsed_groups.remove(&group_index) {
            self.collapsed_groups.insert(group_index);
        }

        self.reflow(self.columns, self.tile_size, cx);

        cx.notify();
    }

    fn on_prev(&mut self, _: &actions::Prev, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn on_next(&mut self, _: &actions::Next, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn on_close(&mut self, _: &actions::CloseLightbox, _: &mut Window, cx: &mut Context<Self>) {
        self.close(cx);
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

    fn zoom_grid_in(&mut self, cx: &mut Context<Self>) {
        let current = self.column_override.unwrap_or(self.columns);
        self.column_override = Some((current - 1).max(1));

        cx.notify();
    }

    fn zoom_grid_out(&mut self, cx: &mut Context<Self>) {
        let current = self.column_override.unwrap_or(self.columns);
        self.column_override = Some((current + 1).min(20));

        cx.notify();
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
                self.reflow(self.columns, self.tile_size, cx);
            }
            _ => {}
        };
    }

    fn render_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(index).cloned() else {
            return div().into_any_element();
        };

        match row {
            Row::Header(group_index) => {
                let group = &self.groups[group_index];
                let segments = group_segments(&self.roots, &group.path);
                let count = group.range.len();
                let is_collapsed = self.collapsed_groups.contains(&group_index);

                div()
                    .id(format!("header-{group_index}"))
                    .px_4()
                    .pt_5()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_group(group_index, cx)))
                    .child(
                        Button::new(format!("chevron-{group_index}"))
                            .ghost()
                            .small()
                            .icon(if is_collapsed {
                                IconName::ChevronRight
                            } else {
                                IconName::ChevronDown
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.toggle_group(group_index, cx);
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
            Row::Tiles(range) => div()
                .px_4()
                .pb_3()
                .flex()
                .gap_3()
                .children(range.map(|pos| self.render_thumb(self.visible[pos], cx)))
                .into_any_element(),
        }
    }

    fn render_thumb(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let source = self.tile_source(index, cx);
        let tile = px(self.tile_size);

        div()
            .id(index)
            .flex_none()
            .w(tile)
            .h(tile)
            .rounded_md()
            .overflow_hidden()
            .border_1()
            .border_color(cx.theme().border)
            .hover(|s| s.border_color(gpui::rgb(COLOR_ACCENT)))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| this.open(index, cx)))
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

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let roots = self
            .roots
            .iter()
            .map(|r| r.display().to_string())
            .collect::<Vec<_>>()
            .join(" · ");

        let upper = || {
            h_flex()
                .gap_4()
                .items_center()
                .justify_between()
                .child(
                    h_flex().items_baseline().gap_3().child(
                        div()
                            .min_w_0()
                            .text_sm()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(format!("{roots}")),
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

        let lower = || {
            h_flex()
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
                        .child(format!(
                            "{} images in {} folders",
                            self.visible.len(),
                            self.groups.len()
                        )),
                )
        };

        v_flex()
            .items_stretch()
            .gap_4()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(upper())
            .child(Separator::horizontal())
            .child(lower())
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .child("No images found.")
    }

    fn render_info_bar(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let entry = &self.images[index];
        let name = label_for(&self.roots, &entry.path);
        let bytes = format_bytes(entry.bytes);

        // Counter position/total are relative to the filtered view
        let position = self.visible_position(index).map(|p| p + 1).unwrap_or(0);
        let counter = format!("{} / {}", position, self.visible.len());

        let counter = || {
            Tag::secondary()
                .flex_none()
                .w_20()
                .justify_center()
                .child(counter)
        };

        let size = || {
            h_flex()
                .flex_none()
                .w_16()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child(bytes)
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
                .child(size()),
        )
    }

    fn render_lightbox_content(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let path = self.images[index].path.clone();

        let thumb = match &self.thumbs[index] {
            ThumbState::Ready(p) if *p != path => Some(p.clone()),
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

        let image = || {
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
            .child(image())
            .child(next_button(cx))
    }

    fn render_lightbox(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("lightbox")
            .absolute()
            .inset_0()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(COLOR_BACKDROP))
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.close(cx);
            }))
            .on_scroll_wheel(cx.listener(|_, _: &ScrollWheelEvent, _, cx| {
                cx.stop_propagation();
            }))
            .cursor_default()
            .child(self.render_lightbox_content(index, cx))
            .child(self.render_info_bar(index, cx))
    }

    fn render_grid(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let grid = self.grid.clone();

        div()
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

        if (columns != self.columns || (tile_size - self.tile_size).abs() > 0.5)
            && !self.images.is_empty()
        {
            self.reflow(columns, tile_size, cx);
        }

        v_flex()
            .key_context("Gallery")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_prev))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_close))
            .on_action(cx.listener(Self::on_zoom_in))
            .on_action(cx.listener(Self::on_zoom_out))
            .on_action(cx.listener(Self::on_zoom_reset))
            .relative()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_header(cx))
            .map(|el| {
                if self.visible.is_empty() {
                    el.child(self.render_empty(cx))
                } else {
                    el.child(self.render_grid(cx))
                }
            })
            .children(self.viewer.map(|index| self.render_lightbox(index, cx)))
    }
}
