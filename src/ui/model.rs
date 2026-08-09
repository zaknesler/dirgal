use crate::assets::IconAsset;
use gpui::ElementId;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Hash, PartialEq, Eq, serde::Deserialize, schemars::JsonSchema)]
pub struct ImageHash(pub u64);

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct GroupHash(pub u64);

/// Key by which images are ordered
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
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

/// Direction in which images are ordered
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

impl From<bool> for SortDirection {
    fn from(ascending: bool) -> Self {
        if ascending { Self::Asc } else { Self::Desc }
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

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Page {
    #[default]
    Gallery,
    Bookmarks,
    Duplicates,
}

impl Page {
    pub const ALL: [(Page, &'static str, IconAsset); 3] = [
        (Page::Gallery, "Gallery", IconAsset::Grid),
        (Page::Bookmarks, "Bookmarks", IconAsset::Bookmark),
        (Page::Duplicates, "Duplicates", IconAsset::Layers),
    ];

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum View {
    Grouped,
    #[default]
    Grid,
    List,
}

impl View {
    pub const ALL: [(View, &'static str, IconAsset); 3] = [
        (View::Grid, "Grid", IconAsset::Grid),
        (View::Grouped, "Grouped", IconAsset::Folder),
        (View::List, "List", IconAsset::LayoutList),
    ];
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThumbnailFit {
    /// The image will be scaled to cover the bounds of the element.
    #[default]
    Cover,
    /// The image will be scaled to fit within the bounds of the element.
    Contain,
}

impl ThumbnailFit {
    pub const ALL: [(ThumbnailFit, &'static str, IconAsset); 2] = [
        (ThumbnailFit::Cover, "Cover", IconAsset::Maximize),
        (ThumbnailFit::Contain, "Contain", IconAsset::Minimize),
    ];
}

impl From<ThumbnailFit> for ElementId {
    fn from(value: ThumbnailFit) -> Self {
        Self::Name(match value {
            ThumbnailFit::Cover => "cover".into(),
            ThumbnailFit::Contain => "contain".into(),
        })
    }
}
