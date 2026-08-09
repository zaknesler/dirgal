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

pub const PRESET_SLOTS: std::ops::RangeInclusive<u32> = 1..=9;

#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct AppConfig {
    /// Default page
    #[serde(default)]
    pub page: crate::ui::model::Page,
    #[serde(flatten)]
    pub settings: Settings,
    #[serde(default, deserialize_with = "deserialize_presets")]
    pub presets: HashMap<u32, PartialSettings>,
}

/// Display options for whichever page is active
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub view: crate::ui::model::View,
    #[serde(default)]
    pub sort_key: crate::ui::model::SortKey,
    #[serde(default)]
    pub sort_direction: crate::ui::model::SortDirection,
    #[serde(default)]
    pub thumbnail_fit: crate::ui::model::ThumbnailFit,
}

impl Settings {
    pub fn sort(&self) -> crate::ui::model::Sort {
        crate::ui::model::Sort {
            key: self.sort_key,
            ascending: self.sort_direction == crate::ui::model::SortDirection::Asc,
        }
    }

    pub fn set_sort(&mut self, sort: crate::ui::model::Sort) {
        self.sort_key = sort.key;
        self.sort_direction = sort.ascending.into();
    }
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub struct PartialSettings {
    pub view: Option<crate::ui::model::View>,
    pub sort_key: Option<crate::ui::model::SortKey>,
    pub sort_direction: Option<crate::ui::model::SortDirection>,
    pub thumbnail_fit: Option<crate::ui::model::ThumbnailFit>,
}

impl PartialSettings {
    /// Take the given settings with the defaults applied from the base settings
    pub fn with_defaults(&self, base: Settings) -> Settings {
        Settings {
            view: self.view.unwrap_or(base.view),
            sort_key: self.sort_key.unwrap_or(base.sort_key),
            sort_direction: self.sort_direction.unwrap_or(base.sort_direction),
            thumbnail_fit: self.thumbnail_fit.unwrap_or(base.thumbnail_fit),
        }
    }
}

impl AppConfig {
    /// Load the config from disk with an optional override path
    pub fn load(override_path: Option<String>) -> AppResult<AppConfig> {
        let dir = Self::init_file()?;
        let mut config = Figment::new().merge(Toml::file(dir.join(CONFIG_FILE_NAME)));

        // Maybe override with a custom config file
        if let Some(path) = override_path {
            config = config.merge(Toml::file(PathBuf::from(path)))
        }

        Ok(config.extract()?)
    }

    /// Get the stub data used to seed a fresh config file
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

// I need this fuckass deserializer to get the string keys to parse as u32 and to limit to 1-9
fn deserialize_presets<'de, D>(deserializer: D) -> Result<HashMap<u32, PartialSettings>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: HashMap<String, PartialSettings> = serde::Deserialize::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|(key, value)| match key.parse::<u32>() {
            Ok(slot) if PRESET_SLOTS.contains(&slot) => Some((slot, value)),
            _ => {
                tracing::warn!(
                    key,
                    "ignoring preset, slot must be {}-{}",
                    PRESET_SLOTS.start(),
                    PRESET_SLOTS.end()
                );
                None
            }
        })
        .collect())
}
