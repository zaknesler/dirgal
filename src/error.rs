pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Could not initialize tracing: {0}")]
    TracingInitError(String),

    #[error("User system config directory not found")]
    ConfigDirNotFound,

    #[error("Asset not found: {0}")]
    AssetNotFound(String),

    #[error(transparent)]
    ImageDecode(#[from] image::ImageError),

    #[error(transparent)]
    ConfigError(#[from] figment::Error),

    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error(transparent)]
    TomlError(#[from] toml::ser::Error),

    #[error(transparent)]
    ProgressTemplateError(#[from] indicatif::style::TemplateError),

    #[error(transparent)]
    CacheEncodeError(#[from] postcard::Error),

    #[error(transparent)]
    TrashError(#[from] trash::Error),
}
