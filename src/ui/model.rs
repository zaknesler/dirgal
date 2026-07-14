use std::path::{Path, PathBuf};
use std::sync::Arc;

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
}

impl SortKey {
    /// All keys in display order paired with their menu labels
    pub const ALL: [(SortKey, &'static str); 4] = [
        (SortKey::Name, "Name"),
        (SortKey::Modified, "Date modified"),
        (SortKey::Created, "Date created"),
        (SortKey::Size, "Size"),
    ];
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

#[derive(Clone, Copy)]
pub struct Job {
    pub image_hash: ImageHash,
    pub priority: JobPriority,
}

#[derive(Clone, Copy, PartialEq)]
pub enum JobPriority {
    Urgent,
    Deferred,
}

#[derive(Clone)]
pub enum ThumbState {
    Unknown,
    Queued,
    Generating,
    Ready(Arc<Path>),
    Failed,
}
