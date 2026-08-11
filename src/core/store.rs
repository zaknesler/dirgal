use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const PROJECT_DIR: &str = "dirgal";
const CACHE_FILE_NAME: &str = "cache";
const BOOKMARKS_FILE_NAME: &str = "bookmarks";

// Use separate versions for the cache and bookmarks files
const CACHE_VERSION: u32 = 2;
const BOOKMARKS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HashCacheEntry {
    pub size: u64,
    pub mtime: u64,
    pub hash: u64,
    pub dimensions: Option<(u32, u32)>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    entries: HashMap<PathBuf, HashCacheEntry>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BookmarksFile {
    version: u32,
    bookmarks: Vec<u64>,
}

#[derive(Debug, Default)]
pub struct Store {
    cache: HashMap<PathBuf, HashCacheEntry>,
    pub bookmarks: Vec<u64>,
}

impl Store {
    /// Load the cache and bookmarks files
    pub fn load() -> Self {
        Self {
            cache: Self::load_cache(),
            bookmarks: Self::load_bookmarks(),
        }
    }

    /// Look up a cached entry for the given path, valid only if the size and mtime still match
    pub fn get(
        &self,
        path: &Path,
        size: u64,
        modified: Option<SystemTime>,
    ) -> Option<&HashCacheEntry> {
        let mtime = to_epoch_secs(modified?)?;
        let entry = self.cache.get(path)?;

        (entry.size == size && entry.mtime == mtime).then_some(entry)
    }

    /// Merge newly-computed entries in, overwriting any existing entry at the same path
    pub fn merge_entries(&mut self, entries: HashMap<PathBuf, HashCacheEntry>) {
        self.cache.extend(entries);
    }

    /// Write the cache entries back out to disk
    pub fn save(&self) -> AppResult<()> {
        let file = CacheFile {
            version: CACHE_VERSION,
            entries: self.cache.clone(),
        };

        write_file(Self::cache_path()?, &file)
    }

    /// Overwrite the bookmarks file with the given bookmarks
    pub fn save_bookmarks(bookmarks: &[u64]) -> AppResult<()> {
        let file = BookmarksFile {
            version: BOOKMARKS_VERSION,
            bookmarks: bookmarks.to_vec(),
        };

        write_file(Self::bookmarks_path()?, &file)
    }

    /// Clear all cached hash entries, leaving bookmarks untouched
    pub fn clear_cache() -> AppResult<()> {
        write_file(Self::cache_path()?, &CacheFile::default())
    }

    /// Load the cache file, defaulting to empty if it doesn't exist
    fn load_cache() -> HashMap<PathBuf, HashCacheEntry> {
        let Ok(path) = Self::cache_path() else {
            return HashMap::new();
        };

        let Ok(bytes) = std::fs::read(&path) else {
            return HashMap::new();
        };

        let Ok(file) = postcard::from_bytes::<CacheFile>(&bytes) else {
            tracing::warn!(path = %path.display(), "failed to decode cache file, ignoring");
            return HashMap::new();
        };

        if file.version != CACHE_VERSION {
            return HashMap::new();
        }

        file.entries
    }

    fn load_bookmarks() -> Vec<u64> {
        let Ok(path) = Self::bookmarks_path() else {
            return Vec::new();
        };

        let Ok(bytes) = std::fs::read(&path) else {
            return Vec::new();
        };

        let Ok(file) = postcard::from_bytes::<BookmarksFile>(&bytes) else {
            tracing::warn!(path = %path.display(), "failed to decode bookmarks file, ignoring");
            return Vec::new();
        };

        if file.version != BOOKMARKS_VERSION {
            return Vec::new();
        }

        file.bookmarks
    }

    fn cache_path() -> AppResult<PathBuf> {
        Ok(Self::data_dir()?.join(CACHE_FILE_NAME))
    }

    fn bookmarks_path() -> AppResult<PathBuf> {
        Ok(Self::data_dir()?.join(BOOKMARKS_FILE_NAME))
    }

    fn data_dir() -> AppResult<PathBuf> {
        directories::ProjectDirs::from("", "", PROJECT_DIR)
            .map(|dirs| dirs.data_dir().to_path_buf())
            .ok_or(AppError::ConfigDirNotFound)
    }
}

fn write_file<T: Serialize>(path: PathBuf, value: &T) -> AppResult<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }

    // Write to temp file first to prevent corruption
    let bytes = postcard::to_allocvec(value)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path)?;

    Ok(())
}

fn to_epoch_secs(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}
