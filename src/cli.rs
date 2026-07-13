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

    #[clap(subcommand)]
    pub command: Option<Command>,

    /// Paths to include in the gallery
    #[clap(trailing_var_arg = true)]
    pub paths: Option<Vec<String>>,
}

#[derive(Debug, Parser)]
pub enum Command {
    /// Delete all thumbnail images from the cache
    DeleteThumbnails,
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn to_string(&self) -> &str {
        match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
        }
    }
}
