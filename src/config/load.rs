use std::fs;
use std::path::{Path, PathBuf};

use super::ConfigError;
use super::EZM_CONFIG_ENV;
use super::EnvProvider;
use super::FileConfig;
use super::LoadedConfig;
use super::OperatingSystem;

/// Resolves the v1 config path using `EZM_CONFIG` override or OS defaults.
///
/// # Errors
///
/// Returns [`ConfigError::MissingHome`] when a required `HOME` value is absent.
pub fn resolve_config_path(
    env: &impl EnvProvider,
    os: OperatingSystem,
) -> Result<PathBuf, ConfigError> {
    if let Some(explicit_path) = env_var_non_empty(env, EZM_CONFIG_ENV) {
        return Ok(PathBuf::from(explicit_path));
    }

    match os {
        OperatingSystem::Linux => {
            if let Some(xdg_home) = env_var_non_empty(env, "XDG_CONFIG_HOME") {
                return Ok(PathBuf::from(xdg_home).join("ez-mux").join("config.toml"));
            }

            let home = env
                .get_var("HOME")
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .ok_or(ConfigError::MissingHome { os: os.label() })?;

            Ok(PathBuf::from(home)
                .join(".config")
                .join("ez-mux")
                .join("config.toml"))
        }
        OperatingSystem::MacOs => {
            let home = env
                .get_var("HOME")
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .ok_or(ConfigError::MissingHome { os: os.label() })?;

            Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("ez-mux")
                .join("config.toml"))
        }
        OperatingSystem::Unsupported => Err(ConfigError::UnsupportedPlatform { os: os.label() }),
    }
}

fn env_var_non_empty(env: &impl EnvProvider, key: &str) -> Option<String> {
    normalize_optional_value(env.get_var(key))
}

/// Loads and parses config from the resolved v1 config path.
///
/// # Errors
///
/// Returns:
/// - [`ConfigError::MissingHome`] when default path expansion requires `HOME`.
/// - [`ConfigError::ReadFailed`] when the config file cannot be read.
/// - [`ConfigError::InvalidToml`] when TOML parsing fails.
pub fn load_config(
    env: &impl EnvProvider,
    os: OperatingSystem,
) -> Result<LoadedConfig, ConfigError> {
    let path = resolve_config_path(env, os)?;
    let values = load_file_config(&path)?;
    Ok(LoadedConfig { path, values })
}

fn normalize_optional_value(value: Option<String>) -> Option<String> {
    value
        .map(|candidate| candidate.trim().to_owned())
        .filter(|candidate| !candidate.is_empty())
}

fn load_file_config(path: &Path) -> Result<FileConfig, ConfigError> {
    if !path.exists() {
        return Ok(FileConfig::default());
    }

    let raw = fs::read_to_string(path).map_err(|source| ConfigError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;

    toml::from_str(&raw).map_err(|source| ConfigError::InvalidToml {
        path: path.to_path_buf(),
        source,
    })
}
