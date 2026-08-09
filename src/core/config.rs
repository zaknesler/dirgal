use crate::error::{AppError, AppResult};
use figment::{
    Figment,
    providers::{Format, Toml},
};
use std::{collections::HashMap, fs, io::Write as _, path::PathBuf};

const PROJECT_DIR: &str = "dirgal";
const DEFAULT_FILE_NAME: &str = "default.config.toml";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(rust_embed::Embed)]
#[folder = "stubs"]
struct StubAssetDir;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct AppConfig {
    #[serde(default)]
    pub view: crate::ui::model::View,
    #[serde(default)]
    pub page: crate::ui::model::Page,
    #[serde(default)]
    pub sort_key: crate::ui::model::SortKey,
    #[serde(default)]
    pub sort_direction: crate::ui::model::SortDirection,
    #[serde(default)]
    pub thumbnail_fit: crate::ui::model::ThumbnailFit,
    #[serde(default, deserialize_with = "deserialize_presets")]
    pub presets: HashMap<u32, ConfigPreset>,
}

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct ConfigPreset {
    pub view: Option<crate::ui::model::View>,
    pub sort_key: Option<crate::ui::model::SortKey>,
    pub sort_direction: Option<crate::ui::model::SortDirection>,
    pub thumbnail_fit: Option<crate::ui::model::ThumbnailFit>,
}

impl AppConfig {
    /// Load the config from disk with an optional override path
    pub fn load(override_path: Option<String>) -> AppResult<AppConfig> {
        Self::init_file()?;

        let dir = Self::get_dir()?;

        let mut config = Figment::new()
            .merge(Toml::string(std::str::from_utf8(
                Self::get_default_data().as_ref(),
            )?))
            .merge(Toml::file(
                dir.join(CONFIG_FILE_NAME)
                    .to_str()
                    .ok_or_else(|| AppError::ConfigFileNotFound)?,
            ));

        // Maybe override with a custom config file
        if let Some(path) = override_path {
            config = config.merge(Toml::file(PathBuf::from(path)))
        }

        Ok(config.extract()?)
    }

    /// Get the default data for the config file
    fn get_default_data() -> Vec<u8> {
        let default = StubAssetDir::get(DEFAULT_FILE_NAME).expect("default.toml stub should exist");
        default.data.as_ref().to_owned()
    }

    /// Get the path to the config directory
    fn get_dir() -> AppResult<PathBuf> {
        directories::ProjectDirs::from("", "", PROJECT_DIR)
            .map(|dirs| dirs.config_dir().to_path_buf())
            .ok_or_else(|| AppError::ConfigDirNotFound)
    }

    /// Initialize config directory and config.toml
    fn init_file() -> AppResult<PathBuf> {
        let dir = Self::init_dir()?;

        // Create local config if it doesn't exist
        let local_file = dir.join(CONFIG_FILE_NAME);
        let exists = local_file.try_exists()?;

        if !exists {
            let mut local_config = fs::File::create(local_file)?;
            local_config.write_all(Self::get_default_data().as_ref())?;
        }

        Ok(dir)
    }

    /// Initialize config directory
    fn init_dir() -> AppResult<PathBuf> {
        let dir = Self::get_dir()?;

        // Create project config directory if it doesn't exist
        fs::create_dir_all(&dir)?;

        Ok(dir)
    }
}

// I wish I didn't need this fuckass deserializer to get the string keys to parse as u32...
fn deserialize_presets<'de, D>(deserializer: D) -> Result<HashMap<u32, ConfigPreset>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: HashMap<String, ConfigPreset> = serde::Deserialize::deserialize(deserializer)?;
    raw.into_iter()
        .map(|(key, value)| {
            key.parse::<u32>()
                .ok()
                .filter(|key| (1..=9).contains(key))
                .map(|key| (key, value))
                .ok_or_else(|| {
                    serde::de::Error::custom(format!(
                        "invalid preset key: {key} (only 1-9 allowed)"
                    ))
                })
        })
        .collect()
}
