use std::collections::HashMap;
use std::fs;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

pub const EZM_CONFIG_ENV: &str = "EZM_CONFIG";
pub const EZM_BIN_ENV: &str = "EZM_BIN";
pub const OPERATOR_ENV: &str = "OPERATOR";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingSystem {
    Linux,
    MacOs,
}

impl OperatingSystem {
    #[must_use]
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::MacOs => "macOS",
        }
    }
}

pub trait EnvProvider {
    fn get_var(&self, key: &str) -> Option<String>;
}

pub struct ProcessEnv;

impl EnvProvider for ProcessEnv {
    fn get_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

impl<S: BuildHasher> EnvProvider for HashMap<String, String, S> {
    fn get_var(&self, key: &str) -> Option<String> {
        self.get(key).cloned()
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("unable to resolve default config path for {os}: HOME is not set")]
    MissingHome { os: &'static str },
    #[error("failed reading config file at {path}: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid TOML in config file at {path}: {source}")]
    InvalidToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct FileConfig {
    pub operator: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub values: FileConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSource {
    Cli,
    Env,
    File,
    Default,
}

impl ValueSource {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Env => "env",
            Self::File => "file",
            Self::Default => "default",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedValue<T> {
    pub value: T,
    pub source: ValueSource,
}

/// Resolves the v1 config path using `EZM_CONFIG` override or OS defaults.
///
/// # Errors
///
/// Returns [`ConfigError::MissingHome`] when a required `HOME` value is absent.
pub fn resolve_config_path(
    env: &impl EnvProvider,
    os: OperatingSystem,
) -> Result<PathBuf, ConfigError> {
    if let Some(explicit_path) = env.get_var(EZM_CONFIG_ENV) {
        return Ok(PathBuf::from(explicit_path));
    }

    match os {
        OperatingSystem::Linux => {
            if let Some(xdg_home) = env.get_var("XDG_CONFIG_HOME") {
                return Ok(PathBuf::from(xdg_home).join("ez-mux").join("config.toml"));
            }

            let home = env
                .get_var("HOME")
                .ok_or(ConfigError::MissingHome { os: os.label() })?;

            Ok(PathBuf::from(home)
                .join(".config")
                .join("ez-mux")
                .join("config.toml"))
        }
        OperatingSystem::MacOs => {
            let home = env
                .get_var("HOME")
                .ok_or(ConfigError::MissingHome { os: os.label() })?;

            Ok(PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("ez-mux")
                .join("config.toml"))
        }
    }
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

#[must_use]
pub fn resolve_operator(
    cli_operator: Option<String>,
    env_operator: Option<String>,
    file_operator: Option<String>,
) -> ResolvedValue<Option<String>> {
    if let Some(operator) = cli_operator {
        return ResolvedValue {
            value: Some(operator),
            source: ValueSource::Cli,
        };
    }

    if let Some(operator) = env_operator {
        return ResolvedValue {
            value: Some(operator),
            source: ValueSource::Env,
        };
    }

    if let Some(operator) = file_operator {
        return ResolvedValue {
            value: Some(operator),
            source: ValueSource::File,
        };
    }

    ResolvedValue {
        value: None,
        source: ValueSource::Default,
    }
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use tempfile::tempdir;

    use super::{
        ConfigError, EZM_CONFIG_ENV, FileConfig, OPERATOR_ENV, OperatingSystem, ValueSource,
        load_config, resolve_config_path, resolve_operator,
    };

    #[test]
    fn linux_uses_xdg_config_home() {
        let mut env = HashMap::new();
        env.insert(String::from("XDG_CONFIG_HOME"), String::from("/tmp/xdg"));
        env.insert(String::from("HOME"), String::from("/tmp/home"));

        let path = resolve_config_path(&env, OperatingSystem::Linux).expect("path should resolve");
        assert_eq!(
            path,
            std::path::PathBuf::from("/tmp/xdg/ez-mux/config.toml")
        );
    }

    #[test]
    fn linux_falls_back_to_home_config() {
        let mut env = HashMap::new();
        env.insert(String::from("HOME"), String::from("/tmp/home"));

        let path = resolve_config_path(&env, OperatingSystem::Linux).expect("path should resolve");
        assert_eq!(
            path,
            std::path::PathBuf::from("/tmp/home/.config/ez-mux/config.toml")
        );
    }

    #[test]
    fn macos_uses_application_support() {
        let mut env = HashMap::new();
        env.insert(String::from("HOME"), String::from("/Users/tester"));

        let path = resolve_config_path(&env, OperatingSystem::MacOs).expect("path should resolve");
        assert_eq!(
            path,
            std::path::PathBuf::from(
                "/Users/tester/Library/Application Support/ez-mux/config.toml"
            )
        );
    }

    #[test]
    fn ezm_config_overrides_default_path() {
        let mut env = HashMap::new();
        env.insert(
            String::from(EZM_CONFIG_ENV),
            String::from("/custom/path.toml"),
        );
        env.insert(String::from("HOME"), String::from("/tmp/home"));

        let path = resolve_config_path(&env, OperatingSystem::Linux).expect("path should resolve");
        assert_eq!(path, std::path::PathBuf::from("/custom/path.toml"));
    }

    #[test]
    fn missing_config_file_is_non_fatal() {
        let dir = tempdir().expect("tempdir");
        let mut env = HashMap::new();
        env.insert(
            String::from(EZM_CONFIG_ENV),
            dir.path().join("missing.toml").display().to_string(),
        );

        let loaded = load_config(&env, OperatingSystem::Linux).expect("load should succeed");
        assert_eq!(loaded.values, FileConfig::default());
    }

    #[test]
    fn invalid_toml_is_fatal() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        fs::write(&path, "operator = [").expect("write");

        let mut env = HashMap::new();
        env.insert(String::from(EZM_CONFIG_ENV), path.display().to_string());

        let error = load_config(&env, OperatingSystem::Linux).expect_err("load should fail");
        assert!(matches!(error, ConfigError::InvalidToml { .. }));
    }

    #[test]
    fn precedence_is_cli_then_env_then_file_then_default() {
        let resolved = resolve_operator(
            Some(String::from("cli")),
            Some(String::from("env")),
            Some(String::from("file")),
        );
        assert_eq!(resolved.value, Some(String::from("cli")));
        assert_eq!(resolved.source, ValueSource::Cli);

        let resolved =
            resolve_operator(None, Some(String::from("env")), Some(String::from("file")));
        assert_eq!(resolved.value, Some(String::from("env")));
        assert_eq!(resolved.source, ValueSource::Env);

        let resolved = resolve_operator(None, None, Some(String::from("file")));
        assert_eq!(resolved.value, Some(String::from("file")));
        assert_eq!(resolved.source, ValueSource::File);

        let resolved = resolve_operator(None, None, None);
        assert_eq!(resolved.value, None);
        assert_eq!(resolved.source, ValueSource::Default);
    }

    #[test]
    fn operator_env_constant_is_contract_stable() {
        assert_eq!(OPERATOR_ENV, "OPERATOR");
    }
}
