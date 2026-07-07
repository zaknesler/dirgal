use image::collect_images;
use std::path::PathBuf;

mod hash;
mod image;
mod path;
mod ui;

fn main() {
    init_tracing();

    let roots = get_roots();
    let thumb_dir = get_thumbnail_dir();
    let images = collect_images(&roots, &thumb_dir);

    ui::window::create_window(roots, images);
}

fn get_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        roots.push(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    }

    roots
        .into_iter()
        .map(|r| std::fs::canonicalize(&r).unwrap_or(r))
        .collect()
}

fn get_thumbnail_dir() -> PathBuf {
    let thumb_dir = dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(env!("CARGO_PKG_NAME"))
        .join("thumbnails");

    if let Err(e) = std::fs::create_dir_all(&thumb_dir) {
        tracing::warn!(dir = %thumb_dir.display(), error = %e, "could not create thumbnail cache, using local directory");
        return PathBuf::from("thumbnails");
    }

    thumb_dir
}

fn init_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}
