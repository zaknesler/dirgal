use crate::{
    core::{
        hash::{hash_content, hash_path},
        image::{self, ContentHash, FoundFile, ImageEntry, SMALL_FILE_BYTES},
        store::{HashCacheEntry, Store},
    },
    error::AppResult,
};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    path::PathBuf,
    sync::Mutex,
    time::Duration,
};

const UPDATE_DURATION_MS: u64 = 80;

pub struct DiscoveredImages {
    pub images: Vec<ImageEntry>,
    pub bookmarks: Vec<u64>,
}

/// Recursively scan the given roots for image files, reporting progress as directories are visited and images discovered
pub fn collect_files(roots: &[PathBuf]) -> AppResult<Vec<FoundFile>> {
    let bar = ProgressBar::new_spinner();
    bar.enable_steady_tick(Duration::from_millis(UPDATE_DURATION_MS));
    bar.set_style(ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] {msg}",
    )?);

    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut found: Vec<FoundFile> = Vec::new();
    let mut dirs: u64 = 0;

    for root in roots {
        if !root.is_dir() {
            tracing::warn!(root = %root.display(), "not a directory, skipping");
            continue;
        }

        for entry in image::build_root_walker(root).flatten() {
            let Some(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                dirs += 1;
                bar.set_message(format!(
                    "{dirs} dir(s) visited, {} image(s) found",
                    found.len()
                ));
                continue;
            }

            if file_type.is_file()
                && image::is_image(entry.path())
                && seen.insert(entry.path().to_path_buf())
            {
                let (bytes, modified, created) = image::entry_stats(&entry);
                found.push(FoundFile {
                    path: entry.into_path(),
                    bytes,
                    modified,
                    created,
                });
                bar.set_message(format!(
                    "{dirs} dir(s) visited, {} image(s) found",
                    found.len()
                ));
            }
        }
    }

    bar.finish_with_message(format!(
        "scanned {dirs} dir(s), found {} image(s)",
        found.len()
    ));

    found.sort_by(|a, b| crate::core::path::compare_paths(&a.path, &b.path));

    Ok(found)
}

/// Build image entries from found files, reusing cached content hashes where the file's
/// size and modified time still match, and persisting any newly-computed hashes back to disk
pub fn build_image_entries(
    found: Vec<FoundFile>,
    thumb_dir: &std::path::Path,
) -> AppResult<DiscoveredImages> {
    let bar = ProgressBar::new(found.len() as u64);
    bar.enable_steady_tick(Duration::from_millis(UPDATE_DURATION_MS));
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
        )?
        .progress_chars("##-"),
    );
    bar.set_message(format!("hashing {} image(s)", found.len()));

    let mut store = Store::load();

    let images = found
        .into_par_iter()
        .progress_with(bar.clone())
        .map(|f| {
            let (hash, dimensions) = resolve_meta(&f, &store);
            ImageEntry::new(f, thumb_dir, ContentHash(hash), dimensions)
        })
        .collect::<Vec<ImageEntry>>();

    bar.finish_with_message(format!("hashed {} image(s)", images.len()));

    let entries = images
        .iter()
        .filter_map(|i| {
            let modified = i.modified?;
            let mtime = modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some((
                i.id.to_path_buf(),
                HashCacheEntry {
                    size: i.bytes,
                    mtime,
                    hash: i.content_hash.0,
                    dimensions: i.dimensions,
                },
            ))
        })
        .collect::<HashMap<PathBuf, HashCacheEntry>>();

    store.merge_entries(entries);

    if let Err(err) = store.save() {
        tracing::warn!(error = %err, "failed to write store file");
    }

    Ok(DiscoveredImages {
        images,
        bookmarks: store.bookmarks,
    })
}

/// Resolve the content hash and pixel dimensions
fn resolve_meta(file: &FoundFile, store: &Store) -> (u64, Option<(u32, u32)>) {
    // Check the store for an existing entry
    if let Some(entry) = store.get(&file.path, file.bytes, file.modified) {
        return (entry.hash, entry.dimensions);
    }

    // Hash the first few KBs and read the pixel dimensions
    let hash = hash_content(&file.path).unwrap_or_else(|e| {
        tracing::warn!(path = %file.path.display(), error = %e, "hash_content failed, falling back to hash_path");
        hash_path(&file.path)
    });
    let dimensions = image::read_dimensions(&file.path);

    (hash, dimensions)
}

/// Generate thumbnails for the given images
pub fn generate_thumbnails(images: &[ImageEntry]) -> AppResult<()> {
    let existing = images.iter().filter(|i| i.thumb_exists).count();
    let pending = images
        .iter()
        .filter(|i| !i.thumb_exists && i.bytes >= SMALL_FILE_BYTES)
        .collect::<Vec<_>>();

    if pending.is_empty() {
        return Ok(());
    }

    let bar = ProgressBar::new((existing + pending.len()) as u64);
    bar.set_position(existing as u64);
    bar.enable_steady_tick(std::time::Duration::from_millis(UPDATE_DURATION_MS));
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len}\n  {msg:.60}",
        )?
        .progress_chars("##-"),
    );

    let errors: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());
    pending
        .par_iter()
        .progress_with(bar.clone())
        .for_each(|image| {
            if let Some(name) = image.id.path().file_name() {
                bar.set_message(name.to_string_lossy().into_owned());
            }

            if let Err(err) = image.generate_thumbnail() {
                errors
                    .lock()
                    .unwrap()
                    .push((image.id.path().display().to_string(), err.to_string()))
            }
        });

    bar.finish_with_message(format!(
        "{} thumbnail(s) total, {} already cached, {} generated",
        existing + pending.len(),
        existing,
        pending.len()
    ));

    let errors = errors.into_inner().unwrap();
    if !errors.is_empty() {
        tracing::warn!("{} thumbnail(s) failed to generate:", errors.len());
        for (path, err) in &errors {
            tracing::warn!("  {path}: {err}");
        }
    }

    Ok(())
}

/// Purge the thumbnail directory, removing files and subdirectories individually, reporting progress
pub fn purge_thumbnails(thumb_dir: &Path) {
    if !thumb_dir.exists() {
        return;
    }

    let mut files: Vec<PathBuf> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();

    // Collect all files and directories in the thumbnail directory so we have an accurate total
    for entry in image::build_root_walker(thumb_dir).flatten() {
        let Some(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            if entry.path() != thumb_dir {
                dirs.push(entry.into_path());
            }
        } else if file_type.is_file() {
            files.push(entry.into_path());
        }
    }

    let bar = ProgressBar::new(files.len() as u64);
    bar.enable_steady_tick(Duration::from_millis(UPDATE_DURATION_MS));
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
        )
        .expect("valid progress style")
        .progress_chars("##-"),
    );
    bar.set_message(format!("removing {} thumbnail(s)", files.len()));

    // Delete all thumbnail files
    files
        .par_iter()
        .progress_with(bar.clone())
        .for_each(|file| {
            std::fs::remove_file(file).ok();
        });

    // Delete the root thumbnail directory
    std::fs::remove_dir_all(thumb_dir).ok();

    bar.finish_with_message(format!("removed {} thumbnail(s)", files.len()));
}
