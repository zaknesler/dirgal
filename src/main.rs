use crate::ui::window::create_window;
use image::collect_images;
use std::fs;
use std::path::PathBuf;

mod image;
mod ui;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let mut roots: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        roots.push(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    }
    let roots: Vec<PathBuf> = roots
        .into_iter()
        .map(|r| fs::canonicalize(&r).unwrap_or(r))
        .collect();

    let thumb_dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(env!("CARGO_PKG_NAME"))
        .join("thumbnails");
    if let Err(e) = fs::create_dir_all(&thumb_dir) {
        tracing::warn!(dir = %thumb_dir.display(), error = %e, "could not create thumbnail cache");
    }

    let images = collect_images(&roots, &thumb_dir);

    create_window(roots, images);
}
