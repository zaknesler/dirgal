use crate::{
    error::AppResult,
    image::{ImageEntry, SMALL_FILE_BYTES},
};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::sync::Mutex;

pub fn run(images: &[ImageEntry]) -> AppResult<()> {
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
    bar.enable_steady_tick(std::time::Duration::from_millis(80));
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

    return Ok(());
}
