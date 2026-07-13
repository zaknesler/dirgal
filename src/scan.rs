use crate::image::{ImageEntry, SMALL_FILE_BYTES};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::sync::atomic::{AtomicU64, Ordering};

pub fn run(images: &[ImageEntry]) {
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
        return;
    }

    let bar = ProgressBar::new(pending.len() as u64);
    bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
        )
        .unwrap()
        .progress_chars("##-"),
    );

    let errors = AtomicU64::new(0);
    pending
        .par_iter()
        .progress_with(bar.clone())
        .for_each(|img| {
            if let Err(err) = img.generate_thumbnail() {
                errors.fetch_add(1, Ordering::Relaxed);
                tracing::error!(
                    path = %img.src_path.display(),
                    error = %err,
                    "failed to generate thumbnail"
                );
            }
        });

    bar.finish_with_message("done");

    let errors = errors.into_inner();
    if errors > 0 {
        tracing::warn!("{} thumbnail(s) failed to generate", errors);
    }
}
