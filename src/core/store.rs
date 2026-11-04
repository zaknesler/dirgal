use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const PROJECT_DIR: &str = "dirgal";
const CACHE_FILE_NAME: &str = "cache";
const BOOKMARKS_FILE_NAME: &str = "bookmarks";

// Use separate versions for the cache and bookmarks files
const CACHE_VERSION: u32 = 3;
const BOOKMARKS_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CachedImage {
    pub size: u64,
    pub modified: Option<u64>,
    pub created: Option<u64>,
    pub hash: u64,
    pub dimensions: Option<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RootListing {
    image_paths: Vec<PathBuf>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    entries: HashMap<PathBuf, CachedImage>,
    roots: HashMap<PathBuf, RootListing>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BookmarksFile {
    version: u32,
    bookmarks: Vec<u64>,
}

#[derive(Debug, Default)]
pub struct Store {
    cache: HashMap<PathBuf, CachedImage>,
    roots: HashMap<PathBuf, RootListing>,
    pub bookmarks: Vec<u64>,
}

impl Store {
    /// Load the cache and bookmarks files
    pub fn load() -> Self {
        let cache = Self::load_cache();
        let cached_paths = cache
            .roots
            .values()
            .map(|listing| listing.image_paths.len())
            .sum::<usize>();
        tracing::debug!(
            images = cache.entries.len(),
            roots = cache.roots.len(),
            paths = cached_paths,
            "loaded cache"
        );

        Self {
            cache: cache.entries,
            roots: cache.roots,
            bookmarks: Self::load_bookmarks(),
        }
    }

    /// Look up a cached entry for the given path, valid only if the size and mtime still match
    pub fn get(
        &self,
        path: &Path,
        size: u64,
        modified: Option<SystemTime>,
    ) -> Option<&CachedImage> {
        let mtime = to_epoch_secs(modified?)?;
        let entry = self.cache.get(path)?;

        (entry.size == size && entry.modified == Some(mtime)).then_some(entry)
    }

    /// Load the cached images for every requested root, returning `None` if any data is missing
    pub fn cached_images(&self, roots: &[PathBuf]) -> Option<Vec<(PathBuf, CachedImage)>> {
        let mut seen = HashSet::new();
        let mut images = Vec::new();

        for root in roots {
            let listing = self.roots.get(root)?;
            for path in &listing.image_paths {
                if seen.insert(path) {
                    images.push((path.clone(), *self.cache.get(path)?));
                }
            }
        }

        Some(images)
    }

    /// Merge newly-computed entries in, overwriting any existing entry at the same path
    pub fn merge_entries(&mut self, entries: HashMap<PathBuf, CachedImage>) {
        self.cache.extend(entries);
    }

    /// Replace the last complete listing for a root
    pub fn replace_root_listing(&mut self, root: PathBuf, image_paths: Vec<PathBuf>) {
        self.roots.insert(root, RootListing { image_paths });
    }

    /// Write the cache entries back out to disk
    pub fn save(&self) -> AppResult<()> {
        let file = CacheFile {
            version: CACHE_VERSION,
            entries: self.cache.clone(),
            roots: self.roots.clone(),
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
    fn load_cache() -> CacheFile {
        let Ok(path) = Self::cache_path() else {
            return CacheFile::default();
        };

        let Ok(bytes) = std::fs::read(&path) else {
            return CacheFile::default();
        };

        let Ok(file) = postcard::from_bytes::<CacheFile>(&bytes) else {
            tracing::warn!(path = %path.display(), "failed to decode cache file, ignoring");
            return CacheFile::default();
        };

        if file.version != CACHE_VERSION {
            return CacheFile::default();
        }

        file
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

impl CachedImage {
    pub fn modified_time(self) -> Option<SystemTime> {
        from_epoch_secs(self.modified)
    }

    pub fn created_time(self) -> Option<SystemTime> {
        from_epoch_secs(self.created)
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

fn from_epoch_secs(seconds: Option<u64>) -> Option<SystemTime> {
    UNIX_EPOCH.checked_add(Duration::from_secs(seconds?))
}
