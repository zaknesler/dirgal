#![allow(clippy::result_large_err)]

use clap::Parser as _;

mod cli;
mod config;
mod error;
mod hash;
mod image;
mod path;
mod pipeline;
mod ui;
mod util;

fn main() -> error::AppResult<()> {
    let args = cli::Args::parse();

    init_tracing(args.log_level)?;

    let config = config::AppConfig::load(args.config)?;

    let roots = path::get_roots(args.paths);
    let thumb_dir = path::get_thumbnail_dir();

    if args.purge {
        pipeline::purge_thumbnails(&thumb_dir);
        return Ok(());
    }

    let files = pipeline::collect_files(&roots)?;
    let images = pipeline::build_image_entries(files, &thumb_dir)?;

    if args.prefetch {
        pipeline::generate_thumbnails(&images)?;
    }

    let state = ui::state::AppState {
        config,
        roots,
        images,
    };

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
