use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const CACHE_FILE_NAME: &str = ".dirgal-cache";
const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HashCacheEntry {
    pub size: u64,
    pub mtime: u64,
    pub hash: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    entries: HashMap<PathBuf, HashCacheEntry>,
}

#[derive(Default)]
pub struct HashCache {
    entries: HashMap<PathBuf, HashCacheEntry>,
}

impl HashCache {
    /// Load and merge the cache files found in each of the given root directories
    pub fn load(roots: &[PathBuf]) -> Self {
        let mut entries = HashMap::new();

        for root in roots {
            let path = root.join(CACHE_FILE_NAME);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };

            let Ok(cache) = postcard::from_bytes::<CacheFile>(&bytes) else {
                tracing::warn!(path = %path.display(), "failed to decode cache file, ignoring");
                continue;
            };

            if cache.version != CACHE_VERSION {
                continue;
            }

            for (rel, entry) in cache.entries {
                entries.insert(root.join(rel), entry);
            }
        }

        Self { entries }
    }

    /// Look up a cached hash for the given path, valid only if the size and mtime still match
    pub fn get(&self, path: &Path, size: u64, modified: Option<SystemTime>) -> Option<u64> {
        let mtime = to_epoch_secs(modified?)?;
        let entry = self.entries.get(path)?;

        (entry.size == size && entry.mtime == mtime).then_some(entry.hash)
    }

    /// Write the given entries back out, split into one cache file per root directory
    pub fn save(roots: &[PathBuf], entries: &HashMap<PathBuf, HashCacheEntry>) -> AppResult<()> {
        for root in roots {
            let root_entries_map = entries
                .iter()
                .filter_map(|(path, entry)| {
                    path.strip_prefix(root)
                        .ok()
                        .map(|rel| (rel.to_path_buf(), *entry))
                })
                .collect::<HashMap<PathBuf, HashCacheEntry>>();

            if root_entries_map.is_empty() {
                continue;
            }

            let cache = CacheFile {
                version: CACHE_VERSION,
                entries: root_entries_map,
            };
            let bytes = postcard::to_allocvec(&cache)?;

            let path = root.join(CACHE_FILE_NAME);
            let tmp = root.join(format!("{CACHE_FILE_NAME}.tmp"));
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(&tmp, &path)?;
        }

        Ok(())
    }
}

fn to_epoch_secs(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}
