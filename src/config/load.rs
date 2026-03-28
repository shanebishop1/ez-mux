use std::fs;
use std::path::{Path, PathBuf};

use super::ConfigError;
use super::EZM_CONFIG_ENV;
use super::EnvProvider;
use super::FileConfig;
use super::LoadedConfig;
use super::OperatingSystem;

const CONFIG_FILE_NAME: &str = "ez-mux.toml";

/// Resolves the global v1 config path using `EZM_CONFIG` override or OS defaults.
///
/// # Errors
///
/// Returns [`ConfigError::MissingHome`] when a required `HOME` value is absent.
pub fn resolve_config_path(
    env: &impl EnvProvider,
    os: OperatingSystem,
) -> Result<PathBuf, ConfigError> {
    if let Some(explicit_path) = explicit_config_path(env) {
        return Ok(explicit_path);
    }

    resolve_default_config_path(env, os)
}

fn resolve_default_config_path(
    env: &impl EnvProvider,
    os: OperatingSystem,
) -> Result<PathBuf, ConfigError> {
    match os {
        OperatingSystem::Linux => {
            if let Some(xdg_home) = env_var_non_empty(env, "XDG_CONFIG_HOME") {
                return Ok(PathBuf::from(xdg_home)
                    .join("ez-mux")
                    .join(CONFIG_FILE_NAME));
            }

            let home = env
                .get_var("HOME")
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
                .ok_or(ConfigError::MissingHome { os: os.label() })?;

            Ok(PathBuf::from(home)
                .join(".config")
                .join("ez-mux")
                .join(CONFIG_FILE_NAME))
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
                .join(CONFIG_FILE_NAME))
        }
        OperatingSystem::Unsupported => Err(ConfigError::UnsupportedPlatform { os: os.label() }),
    }
}

fn explicit_config_path(env: &impl EnvProvider) -> Option<PathBuf> {
    env_var_non_empty(env, EZM_CONFIG_ENV).map(PathBuf::from)
}

fn local_config_path(current_dir: Option<&Path>) -> Option<PathBuf> {
    let candidate = current_dir?.join(CONFIG_FILE_NAME);
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

fn resolve_effective_config_path(
    env: &impl EnvProvider,
    os: OperatingSystem,
    current_dir: Option<&Path>,
) -> Result<PathBuf, ConfigError> {
    if let Some(explicit_path) = explicit_config_path(env) {
        return Ok(explicit_path);
    }

    if let Some(local_path) = local_config_path(current_dir) {
        return Ok(local_path);
    }

    resolve_default_config_path(env, os)
}

fn env_var_non_empty(env: &impl EnvProvider, key: &str) -> Option<String> {
    normalize_optional_value(env.get_var(key))
}

/// Loads and parses config from the effective v1 config path.
///
/// Path selection order is `EZM_CONFIG`, then `./ez-mux.toml`, then OS global defaults.
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
    let current_dir = std::env::current_dir().ok();
    load_config_with_current_dir(env, os, current_dir.as_deref())
}

pub(super) fn load_config_with_current_dir(
    env: &impl EnvProvider,
    os: OperatingSystem,
    current_dir: Option<&Path>,
) -> Result<LoadedConfig, ConfigError> {
    let path = resolve_effective_config_path(env, os, current_dir)?;
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
