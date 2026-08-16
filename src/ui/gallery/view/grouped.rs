use super::{Direction, GalleryView, GalleryViewEvent, ScrollTarget};
use crate::{
    assets::IconAsset,
    core::{
        hash::hash_path,
        image::{ImageId, compare_parents},
        path::group_segments,
        util,
    },
    ui::gallery::{
        Gallery,
        constant::{GRID_GAP, GRID_OUTER_MARGIN, GRID_OVERDRAW, GRID_TILE_MIN, MAX_COLS, MIN_COLS},
        library::Library,
    },
};
use gpui::{
    AnyElement, Context, Entity, EventEmitter, ListAlignment, ListOffset, ListState, Pixels,
    Render, WeakEntity, Window, div, list, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Sizable as _,
    breadcrumb::Breadcrumb,
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::Scrollbar,
    tag::Tag,
};
use std::{
    collections::HashSet,
    ops::Range,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct GroupHash(pub u64);

#[derive(Clone, PartialEq)]
pub enum Row {
    Header(GroupHash),
    Tiles(Range<usize>),
}

impl Row {
    /// Split a range of images into tile rows
    pub fn chunk_tiles(offset: usize, len: usize, columns: usize) -> impl Iterator<Item = Row> {
        (0..len).step_by(columns).map(move |start| {
            let end = (start + columns).min(len);
            Row::Tiles(offset + start..offset + end)
        })
    }
}

pub struct Group {
    pub hash: GroupHash,
    pub path: PathBuf,
    pub range: Range<usize>,
}

pub struct GroupedView {
    gallery: WeakEntity<Gallery>,
    ordered_indices: Vec<usize>,
    rows: Vec<Row>,
    groups: Vec<Group>,
    collapsed_groups: HashSet<GroupHash>,
    list_state: ListState,
    tile_size: f32,
    columns: usize,
    column_override: Option<usize>,
}

impl GroupedView {
    /// Create an empty grouped view
    pub fn new(gallery: WeakEntity<Gallery>, cx: &mut Context<Self>) -> Self {
        if let Some(parent) = gallery.upgrade() {
            cx.observe(&parent, |_, _, cx| cx.notify()).detach();
        }

        Self {
            gallery,
            ordered_indices: Vec::new(),
            rows: Vec::new(),
            groups: Vec::new(),
            collapsed_groups: HashSet::new(),
            list_state: ListState::new(0, ListAlignment::Top, px(GRID_OVERDRAW)).measure_all(),
            tile_size: GRID_TILE_MIN,
            columns: 1,
            column_override: None,
        }
    }

    /// Calculate columns and tile size
    pub fn update_layout(&mut self, width: Pixels) {
        let available = width.as_f32() - GRID_OUTER_MARGIN * 2.0;
        let columns = self.column_override.unwrap_or_else(|| {
            (((available + GRID_GAP) / (GRID_TILE_MIN + GRID_GAP)).floor() as usize).max(1)
        });
        let tile_size =
            ((available - columns.saturating_sub(1) as f32 * GRID_GAP) / columns as f32).max(30.0);

        if columns != self.columns {
            self.columns = columns;
            self.rebuild_rows();
        }
        self.tile_size = tile_size;
    }

    /// Rebuild grouped ordering from the filtered images
    pub fn rebuild(&mut self, image_ids: &[ImageId], library: &Library) {
        self.ordered_indices = (0..image_ids.len()).collect();
        self.ordered_indices.sort_by(|a, b| {
            match (library.get(&image_ids[*a]), library.get(&image_ids[*b])) {
                (Some(a), Some(b)) => compare_parents(a, b),
                _ => std::cmp::Ordering::Equal,
            }
        });
        self.rebuild_groups(image_ids, library);
        self.rebuild_rows();
    }

    /// Return a grouped row
    pub fn row(&self, index: usize) -> Option<Row> {
        self.rows.get(index).cloned()
    }

    /// Find a group by hash
    pub fn group(&self, hash: GroupHash) -> Option<&Group> {
        self.groups.iter().find(|group| group.hash == hash)
    }

    /// Return the number of groups
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Check whether a group is collapsed
    pub fn is_collapsed(&self, hash: GroupHash) -> bool {
        self.collapsed_groups.contains(&hash)
    }

    /// Return filtered image indices for a grouped range
    pub fn image_indices(&self, range: Range<usize>) -> &[usize] {
        &self.ordered_indices[range]
    }

    /// Find an image's position in grouped order
    pub fn image_position(&self, image_ids: &[ImageId], id: &ImageId) -> Option<usize> {
        self.ordered_indices
            .iter()
            .position(|index| &image_ids[*index] == id)
    }

    /// Return the first image in an expanded group
    pub fn first_image(&self, image_ids: &[ImageId]) -> Option<ImageId> {
        self.groups
            .iter()
            .find(|group| !self.collapsed_groups.contains(&group.hash))
            .and_then(|group| self.ordered_indices.get(group.range.start))
            .map(|index| image_ids[*index].clone())
    }

    /// Expand one group
    pub fn expand_group(&mut self, hash: GroupHash) {
        if self.collapsed_groups.remove(&hash) {
            self.rebuild_rows();
        }
    }

    /// Toggle one group
    pub fn toggle_group(&mut self, hash: GroupHash) {
        if !self.collapsed_groups.remove(&hash) {
            self.collapsed_groups.insert(hash);
        }
        self.rebuild_rows();
    }

    /// Collapse or expand every group
    pub fn toggle_all(&mut self) {
        if self.collapsed_groups.len() == self.groups.len() {
            self.collapsed_groups.clear();
        } else {
            self.collapsed_groups = self.groups.iter().map(|group| group.hash).collect();
        }
        self.rebuild_rows();
    }

    /// Return filtered image indices near the viewport
    pub fn visible_image_indices(&self, viewport_height: f32) -> Vec<usize> {
        if self.rows.is_empty() {
            return Vec::new();
        }

        let row_height = self.tile_size + GRID_GAP;
        let count = ((viewport_height + 2.0 * GRID_OVERDRAW) / row_height).ceil() as usize + 1;
        let len = self.rows.len();
        let anchor = self.list_state.logical_scroll_top().item_ix.min(len);
        let start = anchor.min(len.saturating_sub(count));
        let end = (start + count).min(len);

        self.rows[start..end]
            .iter()
            .filter_map(|row| match row {
                Row::Header(_) => None,
                Row::Tiles(range) => Some(self.ordered_indices[range.clone()].to_vec()),
            })
            .flatten()
            .collect()
    }

    /// Rebuild directory groups
    fn rebuild_groups(&mut self, image_ids: &[ImageId], library: &Library) {
        self.groups.clear();

        for (position, index) in self.ordered_indices.iter().copied().enumerate() {
            let id = &image_ids[index];
            let parent = library
                .get(id)
                .and_then(|entry| entry.id.path().parent())
                .unwrap_or(Path::new(""));

            match self.groups.last_mut() {
                Some(group) if group.path == parent => group.range.end = position + 1,
                _ => self.groups.push(Group {
                    hash: GroupHash(hash_path(parent)),
                    path: parent.to_path_buf(),
                    range: position..position + 1,
                }),
            }
        }
    }

    /// Rebuild visible headers and tile rows
    fn rebuild_rows(&mut self) {
        let old_rows = std::mem::take(&mut self.rows);

        for group in &self.groups {
            self.rows.push(Row::Header(group.hash));
            if !self.collapsed_groups.contains(&group.hash) {
                self.rows.extend(Row::chunk_tiles(
                    group.range.start,
                    group.range.len(),
                    self.columns,
                ));
            }
        }

        let unchanged_head = std::iter::zip(&old_rows, &self.rows)
            .take_while(|(a, b)| a == b)
            .count();
        let unchanged_tail = std::iter::zip(
            old_rows[unchanged_head..].iter().rev(),
            self.rows[unchanged_head..].iter().rev(),
        )
        .take_while(|(a, b)| a == b)
        .count();

        self.list_state.splice(
            unchanged_head..old_rows.len() - unchanged_tail,
            self.rows.len() - unchanged_head - unchanged_tail,
        );
    }

    /// Return image indices from expanded groups
    fn visible_indices(&self) -> Vec<usize> {
        self.groups
            .iter()
            .filter(|group| !self.collapsed_groups.contains(&group.hash))
            .flat_map(|group| self.ordered_indices[group.range.clone()].iter().copied())
            .collect()
    }

    /// Render one grouped row
    fn render_row(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.row(index) else {
            return div().into_any_element();
        };
        let Some(gallery) = self.gallery.upgrade() else {
            return div().into_any_element();
        };

        match row {
            Row::Header(group_hash) => self.render_header(&gallery, group_hash, index, cx),
            Row::Tiles(range) => self.render_tiles(&gallery, range, index, cx),
        }
    }

    /// Render a group header
    fn render_header(
        &self,
        gallery: &Entity<Gallery>,
        group_hash: GroupHash,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_last_row = index == self.rows.len() - 1;
        let group = self.group(group_hash).expect("group should exist");
        let segments = group_segments(&gallery.read(cx).library.roots, &group.path);
        let count = group.range.len();
        let is_collapsed = self.is_collapsed(group_hash);

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
            .on_click(cx.listener(move |view, _, _, cx| {
                view.toggle_group(group_hash);
                cx.notify();
            }))
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
                    .on_click(cx.listener(move |view, _, _, cx| {
                        cx.stop_propagation();
                        view.toggle_group(group_hash);
                        cx.notify();
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

    /// Render a row of image tiles
    fn render_tiles(
        &self,
        gallery: &Entity<Gallery>,
        range: Range<usize>,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_only_row = index == 0;
        let is_last_row = index == self.rows.len() - 1;
        let gallery_state = gallery.read(cx);
        let image_ids = self
            .image_indices(range)
            .iter()
            .map(|index| gallery_state.filtered_images[*index].clone())
            .collect::<Vec<_>>();

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
                image_ids
                    .iter()
                    .map(|id| super::thumbnail::ImageTile::render(gallery, id, self.tile_size, cx)),
            )
            .into_any_element()
    }
}

impl Render for GroupedView {
    /// Render the virtualized grouped grid
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_layout(window.viewport_size().width);
        if let Some(gallery) = self.gallery.upgrade() {
            let visible = self.visible_image_indices(window.viewport_size().height.as_f32());
            let image_ids = visible
                .into_iter()
                .filter_map(|index| gallery.read(cx).filtered_images.get(index).cloned())
                .collect::<Vec<_>>();
            cx.emit(GalleryViewEvent::VisibleImagesChanged(image_ids));
        }

        div()
            .image_cache(crate::ui::cache::simple_lru_cache(
                crate::ui::CONTEXT_GRID,
                crate::ui::gallery::constant::GRID_CACHE_ITEMS,
            ))
            .flex_1()
            .min_h_0()
            .relative()
            .child(
                list(
                    self.list_state.clone(),
                    cx.processor(|view, index, _, cx| view.render_row(index, cx)),
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .child(Scrollbar::vertical(&self.list_state)),
            )
    }
}

impl EventEmitter<GalleryViewEvent> for GroupedView {}

impl GalleryView for GroupedView {
    /// Find the adjacent image in expanded groups
    fn neighbor(
        &self,
        image_ids: &[ImageId],
        current: Option<&ImageId>,
        direction: Direction,
    ) -> Option<ImageId> {
        let visible = self.visible_indices();
        if visible.is_empty() {
            return None;
        }

        let current = current
            .and_then(|id| visible.iter().position(|index| &image_ids[*index] == id))
            .unwrap_or(0);
        let delta = match direction {
            Direction::Left => -1,
            Direction::Right => 1,
            Direction::Up => -(self.columns as isize),
            Direction::Down => self.columns as isize,
        };
        let next = (current as isize + delta).clamp(0, visible.len() as isize - 1) as usize;

        visible.get(next).map(|index| image_ids[*index].clone())
    }

    /// Scroll the grouped list to a target
    fn scroll_to(&mut self, image_ids: &[ImageId], target: ScrollTarget) -> bool {
        match target {
            ScrollTarget::Start => {
                self.list_state.scroll_to(ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.0),
                });
                true
            }
            ScrollTarget::End => {
                self.list_state.scroll_to_end();
                true
            }
            ScrollTarget::Image(id) => {
                let Some(position) = self.image_position(image_ids, &id) else {
                    return false;
                };
                let Some(row) = self.rows.iter().position(|row| match row {
                    Row::Header(_) => false,
                    Row::Tiles(range) => range.contains(&position),
                }) else {
                    return false;
                };
                self.list_state.scroll_to(ListOffset {
                    item_ix: row,
                    offset_in_item: px(0.0),
                });
                true
            }
        }
    }

    /// Return the first image in an expanded group
    fn first_image(&self, image_ids: &[ImageId]) -> Option<ImageId> {
        self.first_image(image_ids)
    }

    /// Enlarge grouped tiles
    fn zoom_in(&mut self) -> bool {
        let current = self.column_override.unwrap_or(self.columns);
        if current <= MIN_COLS {
            return false;
        }
        self.column_override = Some(current - 1);
        true
    }

    /// Shrink grouped tiles
    fn zoom_out(&mut self) -> bool {
        let current = self.column_override.unwrap_or(self.columns);
        if current >= MAX_COLS {
            return false;
        }
        self.column_override = Some(current + 1);
        true
    }

    /// Restore automatic grouped sizing
    fn zoom_reset(&mut self) -> bool {
        self.column_override.take().is_some()
    }

    /// Collapse or expand every group
    fn toggle_groups(&mut self) -> bool {
        if self.groups.is_empty() {
            return false;
        }
        self.toggle_all();
        true
    }
}
