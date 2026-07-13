use crate::error::{AppError, AppResult};
use figment::{
    Figment,
    providers::{Format, Toml},
};
use std::{fs, io::Write as _, path::PathBuf};

const PROJECT_DIR: &str = "dirgal";
const DEFAULT_FILE_NAME: &str = "default.config.toml";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(rust_embed::Embed)]
#[folder = "stubs"]
struct StubAssetDir;

#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct AppConfig {
    #[serde(default, with = "hex_u64_vec")]
    pub bookmarks: Vec<u64>,
}

impl AppConfig {
    pub fn load(override_path: Option<String>) -> AppResult<AppConfig> {
        Self::init_config_file()?;

        let config_dir = Self::get_config_dir()?;

        let mut config = Figment::new()
            .merge(Toml::string(std::str::from_utf8(
                Self::get_default_data().as_ref(),
            )?))
            .merge(Toml::file(
                config_dir
                    .join(CONFIG_FILE_NAME)
                    .to_str()
                    .ok_or_else(|| AppError::ConfigFileNotFound)?,
            ));

        // Maybe override with a custom config file
        if let Some(path) = override_path {
            config = config.merge(Toml::file(PathBuf::from(path)))
        }

        Ok(config.extract()?)
    }

    pub fn save(&self) -> AppResult<()> {
        let config_dir = Self::get_config_dir()?;
        let contents = toml::to_string_pretty(self)?;
        fs::write(config_dir.join(CONFIG_FILE_NAME), contents)?;

        Ok(())
    }

    fn get_default_data() -> Vec<u8> {
        let default = StubAssetDir::get(DEFAULT_FILE_NAME).expect("default.toml stub should exist");
        default.data.as_ref().to_owned()
    }

    fn get_config_dir() -> AppResult<PathBuf> {
        directories::ProjectDirs::from("", "", PROJECT_DIR)
            .map(|dirs| dirs.config_dir().to_path_buf())
            .ok_or_else(|| AppError::ConfigDirNotFound)
    }

    /// Initialize config directory and config.toml
    fn init_config_file() -> AppResult<PathBuf> {
        let config_dir = Self::init_config_dir()?;

        // Create local config if it doesn't exist
        let local_config_file = config_dir.join(CONFIG_FILE_NAME);
        let exists = local_config_file.try_exists()?;

        if !exists {
            let mut local_config = fs::File::create(local_config_file)?;
            local_config.write_all(Self::get_default_data().as_ref())?;
        }

        Ok(config_dir)
    }

    /// Initialize config directory
    fn init_config_dir() -> AppResult<PathBuf> {
        let config_dir = Self::get_config_dir()?;

        // Create project config directory if it doesn't exist
        fs::create_dir_all(&config_dir)?;

        Ok(config_dir)
    }
}

mod hex_u64_vec {
    use serde::{Deserializer, Serializer, de::Error as _};

    pub fn serialize<S: Serializer>(v: &Vec<u64>, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = s.serialize_seq(Some(v.len()))?;
        for &n in v {
            seq.serialize_element(&format!("{:016x}", n))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u64>, D::Error> {
        let strings = <Vec<String>>::deserialize(d)?;
        strings
            .iter()
            .map(|s| u64::from_str_radix(s, 16).map_err(D::Error::custom))
            .collect()
    }

    use serde::Deserialize;
}
