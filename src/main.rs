#![allow(clippy::result_large_err)]

use crate::core::{config::AppConfig, path, pipeline, scan::ImageScanner, store::Store};
use clap::Parser as _;

mod assets;
mod cli;
mod core;
mod error;
mod ui;

fn main() -> error::AppResult<()> {
    let args = cli::Args::parse();

    init_tracing(args.log_level)?;

    let config = AppConfig::load(args.config)?;

    let roots = path::get_roots(args.paths);
    let thumb_dir = path::get_thumbnail_dir();

    if args.purge_thumbs {
        pipeline::purge_thumbnails(&thumb_dir);
        return Ok(());
    }

    if args.clear_cache {
        Store::clear_cache()?;
        return Ok(());
    }

    let scanner = ImageScanner::scan(roots, thumb_dir)?;

    if args.generate_thumbs {
        scanner.generate_thumbnails()?;
    }

    let state = ui::state::AppState { config, scanner };

    ui::window::create_window(state);

    Ok(())
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
