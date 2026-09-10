use crate::{
    core::{
        hash::{hash_content, hash_path},
        image::{self, ContentHash, FoundFile, ImageEntry, SMALL_FILE_BYTES},
        store::{CachedImage, Store},
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

#[derive(Debug)]
pub struct ScannedRoot {
    pub root: PathBuf,
    pub image_paths: Vec<PathBuf>,
    pub result: ScanResult,
}

#[derive(Debug, Clone, Copy)]
pub enum ScanResult {
    Complete,
    Incomplete { errors: usize },
}

pub struct CollectedFiles {
    pub files: Vec<FoundFile>,
    pub roots: Vec<ScannedRoot>,
}

/// Recursively scan the given roots for image files and report whether each root was fully read
pub fn collect_files(roots: &[PathBuf]) -> AppResult<CollectedFiles> {
    let bar = ProgressBar::new_spinner();
    bar.enable_steady_tick(Duration::from_millis(UPDATE_DURATION_MS));
    bar.set_style(ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] {msg}",
    )?);

    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut found: Vec<FoundFile> = Vec::new();
    let mut scanned_roots = Vec::with_capacity(roots.len());
    let mut dirs: u64 = 0;

    for root in roots {
        let is_dir = match std::fs::metadata(root) {
            Ok(metadata) => metadata.is_dir(),
            Err(error) => {
                tracing::warn!(root = %root.display(), %error, "could not read root, skipping");
                scanned_roots.push(ScannedRoot {
                    root: root.clone(),
                    image_paths: Vec::new(),
                    result: ScanResult::Incomplete { errors: 1 },
                });
                continue;
            }
        };

        if !is_dir {
            tracing::warn!(root = %root.display(), "root is not a directory, skipping");
            scanned_roots.push(ScannedRoot {
                root: root.clone(),
                image_paths: Vec::new(),
                result: ScanResult::Incomplete { errors: 1 },
            });
            continue;
        }

        let mut root_seen: HashSet<PathBuf> = HashSet::new();
        let mut image_paths = Vec::new();
        let mut errors = 0;

        // Visit each entry in the root
        for result in image::build_root_walker(root) {
            let entry = match result {
                Ok(entry) => entry,
                Err(error) => {
                    errors += 1;
                    tracing::warn!(root = %root.display(), %error, "could not read path while scanning root");
                    continue;
                }
            };

            let Some(file_type) = entry.file_type() else {
                errors += 1;
                tracing::warn!(
                    root = %root.display(),
                    path = %entry.path().display(),
                    "could not read file type while scanning root"
                );
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

            if file_type.is_file() && image::is_image(entry.path()) {
                let path = entry.path().to_path_buf();
                if root_seen.insert(path.clone()) {
                    image_paths.push(path.clone());
                }

                if !seen.insert(path) {
                    continue;
                }

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

        image_paths.sort_by(|a, b| crate::core::path::compare_paths(a, b));
        let result = if errors == 0 {
            ScanResult::Complete
        } else {
            ScanResult::Incomplete { errors }
        };
        scanned_roots.push(ScannedRoot {
            root: root.clone(),
            image_paths,
            result,
        });
    }

    let incomplete = scanned_roots
        .iter()
        .filter(|scan| matches!(scan.result, ScanResult::Incomplete { .. }))
        .count();
    bar.finish_with_message(format!(
        "scanned {dirs} dir(s), found {} image(s), {incomplete} incomplete root(s)",
        found.len(),
    ));

    found.sort_by(|a, b| crate::core::path::compare_paths(&a.path, &b.path));

    Ok(CollectedFiles {
        files: found,
        roots: scanned_roots,
    })
}

/// Build image entries from found files, reusing cached content hashes where the file's
/// size and modified time still match, and persisting any newly-computed hashes back to disk
pub fn build_image_entries(
    collected: CollectedFiles,
    thumb_dir: &std::path::Path,
) -> AppResult<DiscoveredImages> {
    let CollectedFiles {
        files: found,
        roots,
    } = collected;

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
    for scan in roots {
        match scan.result {
            ScanResult::Complete => {
                tracing::debug!(
                    root = %scan.root.display(),
                    images = scan.image_paths.len(),
                    "saving root listing"
                );
                store.replace_root_listing(scan.root, scan.image_paths);
            }
            ScanResult::Incomplete { errors } => {
                tracing::debug!(
                    root = %scan.root.display(),
                    images = scan.image_paths.len(),
                    errors,
                    "retaining previous root listing"
                );
            }
        }
    }

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
        .map(|i| {
            (
                i.id.to_path_buf(),
                CachedImage {
                    size: i.bytes,
                    modified: i.modified.and_then(to_epoch_secs),
                    created: i.created.and_then(to_epoch_secs),
                    hash: i.content_hash.0,
                    dimensions: i.dimensions,
                },
            )
        })
        .collect::<HashMap<PathBuf, CachedImage>>();

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

fn to_epoch_secs(time: std::time::SystemTime) -> Option<u64> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
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
