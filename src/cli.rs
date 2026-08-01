use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
#[clap(version, author, about, long_about = None)]
pub struct Args {
    /// Set log level
    #[clap(long, short, default_value = "info")]
    pub log_level: LogLevel,

    /// Path to config file, overrides default
    #[clap(long, short)]
    pub config: Option<String>,

    /// Clear caches image hashes (does not affect bookmarks)
    #[clap(long)]
    pub clear_cache: bool,

    /// Pre-generate all thumbnails before opening
    #[clap(long)]
    pub generate_thumbs: bool,

    /// Delete all cached thumbnail images
    #[clap(long)]
    pub purge_thumbs: bool,

    /// Paths to include in the gallery
    #[clap(trailing_var_arg = true)]
    pub paths: Option<Vec<String>>,
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}
