use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::{CliError, Result};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub default_workspace_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn system() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "ttrackii", "ttrackii").ok_or_else(|| {
            CliError::Config("could not determine the OS config directory".into())
        })?;
        Ok(Self::at(dirs.config_dir().join("config.toml")))
    }

    #[must_use]
    pub const fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Result<Config> {
        if !self.path.exists() {
            return Ok(Config::default());
        }
        let contents = fs::read_to_string(&self.path)?;
        toml::from_str(&contents).map_err(|error| CliError::Config(error.to_string()))
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents =
            toml::to_string_pretty(config).map_err(|error| CliError::Config(error.to_string()))?;
        fs::write(&self.path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_workspace_without_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::at(directory.path().join("nested/config.toml"));
        let config = Config {
            default_workspace_id: Some("team_123".into()),
        };

        store.save(&config).unwrap();
        let loaded = store.load().unwrap();

        assert_eq!(loaded.default_workspace_id.as_deref(), Some("team_123"));
        let raw = fs::read_to_string(directory.path().join("nested/config.toml")).unwrap();
        assert!(!raw.to_ascii_lowercase().contains("token"));
    }
}
