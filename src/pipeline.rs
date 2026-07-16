use crate::{
    error::AppResult,
    image::{self, FoundFile, ImageEntry, SMALL_FILE_BYTES},
};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::{collections::HashSet, path::Path, path::PathBuf, sync::Mutex, time::Duration};

const UPDATE_DURATION_MS: u64 = 80;

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

    found.sort_by(|a, b| crate::path::compare_paths(&a.path, &b.path));

    Ok(found)
}

/// Build image entries (hashing file content) from found files
pub fn build_image_entries(
    found: Vec<FoundFile>,
    thumb_dir: &std::path::Path,
) -> AppResult<Vec<ImageEntry>> {
    let bar = ProgressBar::new(found.len() as u64);
    bar.enable_steady_tick(Duration::from_millis(UPDATE_DURATION_MS));
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
        )?
        .progress_chars("##-"),
    );
    bar.set_message(format!("hashing {} image(s)", found.len()));

    let images = found
        .into_par_iter()
        .progress_with(bar.clone())
        .map(|f| ImageEntry::new(f, thumb_dir))
        .collect::<Vec<ImageEntry>>();

    bar.finish_with_message(format!("hashed {} image(s)", images.len()));

    Ok(images)
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
            if let Some(name) = image.src_path.file_name() {
                bar.set_message(name.to_string_lossy().into_owned());
            }

            if let Err(err) = image.generate_thumbnail() {
                errors
                    .lock()
                    .unwrap()
                    .push((image.src_path.display().to_string(), err.to_string()))
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

    files
        .par_iter()
        .progress_with(bar.clone())
        .for_each(|file| {
            std::fs::remove_file(file).ok();
        });

    // remove deepest directories first so each one is empty when removed
    dirs.sort_by_key(|d| std::cmp::Reverse(d.components().count()));
    for dir in &dirs {
        std::fs::remove_dir(dir).ok();
    }
    std::fs::remove_dir(thumb_dir).ok();

    bar.finish_with_message(format!("removed {} thumbnail(s)", files.len()));
}
