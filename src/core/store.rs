use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const PROJECT_DIR: &str = "dirgal";
const STORE_FILE_NAME: &str = "store";
const STORE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HashCacheEntry {
    pub size: u64,
    pub mtime: u64,
    pub hash: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    entries: HashMap<PathBuf, HashCacheEntry>,
    #[serde(default)]
    bookmarks: Vec<u64>,
}

/// Global cache of hashed image entries and bookmarks, independent of which roots are open
#[derive(Debug, Default)]
pub struct Store {
    entries: HashMap<PathBuf, HashCacheEntry>,
    pub bookmarks: Vec<u64>,
}

impl Store {
    /// Load the store file, or an empty store if it doesn't exist yet
    pub fn load() -> Self {
        let Ok(path) = Self::path() else {
            return Self::default();
        };

        let Ok(bytes) = std::fs::read(&path) else {
            return Self::default();
        };

        let Ok(store) = postcard::from_bytes::<StoreFile>(&bytes) else {
            tracing::warn!(path = %path.display(), "failed to decode store file, ignoring");
            return Self::default();
        };

        // Probably won't happen, but might as well ensure the store version matches the current format
        if store.version != STORE_VERSION {
            return Self::default();
        }

        Self {
            entries: store.entries,
            bookmarks: store.bookmarks,
        }
    }

    /// Look up a cached hash for the given path, valid only if the size and mtime still match
    pub fn get(&self, path: &Path, size: u64, modified: Option<SystemTime>) -> Option<u64> {
        let mtime = to_epoch_secs(modified?)?;
        let entry = self.entries.get(path)?;

        (entry.size == size && entry.mtime == mtime).then_some(entry.hash)
    }

    /// Merge newly-computed entries in, overwriting any existing entry at the same path
    pub fn merge_entries(&mut self, entries: HashMap<PathBuf, HashCacheEntry>) {
        self.entries.extend(entries);
    }

    /// Write the store back out to disk
    pub fn save(&self) -> AppResult<()> {
        let path = Self::path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        let file = StoreFile {
            version: STORE_VERSION,
            entries: self.entries.clone(),
            bookmarks: self.bookmarks.clone(),
        };
        let bytes = postcard::to_allocvec(&file)?;

        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &path)?;

        Ok(())
    }

    /// Load the store, replace its bookmarks, and save it back out
    pub fn save_bookmarks(bookmarks: &[u64]) -> AppResult<()> {
        let mut store = Self::load();
        store.bookmarks = bookmarks.to_vec();
        store.save()
    }

    /// Path to the store file in the app's data directory
    fn path() -> AppResult<PathBuf> {
        directories::ProjectDirs::from("", "", PROJECT_DIR)
            .map(|dirs| dirs.data_dir().join(STORE_FILE_NAME))
            .ok_or(AppError::ConfigDirNotFound)
    }
}

fn to_epoch_secs(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}
