use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::ElementId;
use gpui_component::IconName;

#[derive(Clone, Copy, Hash, PartialEq, Eq, serde::Deserialize, schemars::JsonSchema)]
pub struct ImageHash(pub u64);

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct GroupHash(pub u64);

/// Key by which images are ordered
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Name,
    Modified,
    Created,
    Size,
    DateInPath,
}

impl SortKey {
    pub const ALL: [(SortKey, &'static str); 5] = [
        (SortKey::Name, "Name"),
        (SortKey::Size, "Size"),
        (SortKey::Created, "Date created"),
        (SortKey::Modified, "Date modified"),
        (SortKey::DateInPath, "Date in path"),
    ];

    pub fn index(&self) -> usize {
        Self::ALL
            .iter()
            .position(|(k, _)| k == self)
            .expect("sort key should exist")
    }
}

/// How images are ordered
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub ascending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Sort {
            key: SortKey::default(),
            ascending: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Gallery,
    Bookmarks,
    Duplicates,
}

impl Page {
    pub const ALL: [(Page, &'static str, IconName); 3] = [
        (Page::Gallery, "Gallery", IconName::GalleryVerticalEnd),
        (Page::Bookmarks, "Bookmarks", IconName::Heart),
        (Page::Duplicates, "Duplicates", IconName::Copy),
    ];

    /// Get the default view for the page
    pub fn default_view(&self) -> View {
        match self {
            Self::Gallery => View::Grouped,
            Self::Bookmarks => View::Grid,
            Self::Duplicates => View::List,
        }
    }

    /// Index of this page within `ALL`
    pub fn index(&self) -> usize {
        Self::ALL
            .iter()
            .position(|(p, _, _)| p == self)
            .expect("page should exist")
    }
}

#[derive(Clone, PartialEq)]
pub enum Row {
    Header(GroupHash),
    Tiles(std::ops::Range<usize>),
}

impl Row {
    pub fn chunk_tiles(offset: usize, len: usize, cols: usize) -> impl Iterator<Item = Row> {
        (0..len).step_by(cols).map(move |start| {
            let end = (start + cols).min(len);
            let a = offset + start;
            let b = offset + end;
            Row::Tiles(a..b)
        })
    }
}

pub struct Group {
    pub hash: GroupHash,
    pub path: PathBuf,
    pub image_hashes: Vec<ImageHash>,
}

#[derive(Clone)]
pub enum ThumbState {
    Unknown,
    Queued,
    Generating,
    Ready(Arc<Path>),
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Grouped,
    Grid,
    List,
}

impl From<View> for ElementId {
    fn from(value: View) -> Self {
        Self::Name(match value {
            View::Grouped => "grouped".into(),
            View::Grid => "grid".into(),
            View::List => "list".into(),
        })
    }
}
