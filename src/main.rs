#![allow(clippy::result_large_err)]

use crate::ui::state::AppState;
use clap::Parser;
use std::path::PathBuf;

mod cli;
mod config;
mod error;
mod hash;
mod image;
mod path;
mod scan;
mod ui;
mod util;

fn main() -> error::AppResult<()> {
    let args = cli::Args::parse();

    init_tracing(args.log_level)?;

    let config = config::AppConfig::load(args.config)?;

    let roots = get_roots(args.paths);
    let thumb_dir = get_thumbnail_dir();

    if args.purge {
        util::purge_thumbnails(&thumb_dir);
        return Ok(());
    }

    let images = image::collect_images(&roots, &thumb_dir);

    tracing::info!("found {} images", images.len());

    if args.prefetch {
        scan::run(&images);
    }

    let state = AppState {
        config,
        roots,
        images,
    };

    ui::window::create_window(state);

    Ok(())
}

fn get_roots(paths: Option<Vec<String>>) -> Vec<PathBuf> {
    paths
        .unwrap_or_else(|| vec![".".to_string()])
        .into_iter()
        .map(|path| {
            let path = PathBuf::from(path);
            std::fs::canonicalize(&path).unwrap_or(path)
        })
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

fn init_tracing(log_level: cli::LogLevel) -> error::AppResult<()> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level.as_str()));

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .try_init()
        .map_err(|err| error::AppError::TracingInitError(err.to_string()))?;

    Ok(())
}
