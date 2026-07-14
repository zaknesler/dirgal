use crate::{
    error::AppResult,
    image::{self, FoundFile, ImageEntry, SMALL_FILE_BYTES},
};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::{collections::HashSet, path::PathBuf, sync::Mutex, time::Duration};

const UPDATE_DURATION_MS: u64 = 80;

/// Recursively scan the given roots for images, reporting progress as directories are visited and images discovered
pub fn collect_images(
    roots: &[PathBuf],
    thumb_dir: &std::path::Path,
) -> AppResult<Vec<ImageEntry>> {
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

    Ok(found
        .into_par_iter()
        .map(|f| ImageEntry::new(f, thumb_dir))
        .collect())
}

/// Generate thumbnails for the given images
pub fn generate_thumbnails(images: &[ImageEntry]) -> AppResult<()> {
    let existing = images.iter().filter(|i| i.thumb_exists).count();
    let pending = images
        .iter()
        .filter(|i| !i.thumb_exists && i.bytes >= SMALL_FILE_BYTES)
        .collect::<Vec<_>>();

    tracing::info!(
        "{} thumbnail(s) already cached, {} to generate",
        existing,
        pending.len()
    );

    if pending.is_empty() {
        return Ok(());
    }

    let bar = ProgressBar::new(pending.len() as u64);
    bar.enable_steady_tick(std::time::Duration::from_millis(UPDATE_DURATION_MS));
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})\n  {msg:.60}"
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

    bar.finish_with_message("done");

    let errors = errors.into_inner().unwrap();
    if !errors.is_empty() {
        tracing::warn!("{} thumbnail(s) failed to generate:", errors.len());
        for (path, err) in &errors {
            tracing::warn!("  {path}: {err}");
        }
    }

    Ok(())
}
