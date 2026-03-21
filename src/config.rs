use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::PathBuf;

use serde::Deserialize;
use thiserror::Error;

mod load;

pub use load::{load_config, resolve_config_path};

pub const EZM_CONFIG_ENV: &str = "EZM_CONFIG";
pub const EZM_BIN_ENV: &str = "EZM_BIN";
pub const OPERATOR_ENV: &str = "OPERATOR";
pub const OPENCODE_REMOTE_DIR_PREFIX_ENV: &str = "OPENCODE_REMOTE_DIR_PREFIX";
pub const OPENCODE_SERVER_URL_ENV: &str = "OPENCODE_SERVER_URL";
pub const OPENCODE_SERVER_HOST_ENV: &str = "OPENCODE_SERVER_HOST";
pub const OPENCODE_SERVER_PORT_ENV: &str = "OPENCODE_SERVER_PORT";
pub const OPENCODE_SERVER_PASSWORD_ENV: &str = "OPENCODE_SERVER_PASSWORD";
pub const DEFAULT_OPENCODE_SERVER_HOST: &str = "127.0.0.1";
pub const DEFAULT_OPENCODE_SERVER_PORT: u16 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatingSystem {
    Linux,
    MacOs,
    Unsupported,
}

impl OperatingSystem {
    #[must_use]
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Unsupported
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::MacOs => "macOS",
            Self::Unsupported => "unsupported platform",
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
    #[error("unsupported platform for config path resolution: {os}")]
    UnsupportedPlatform { os: &'static str },
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
    #[error("invalid OpenCode server URL from {origin}: expected absolute http(s) URL")]
    InvalidOpenCodeServerUrl { origin: &'static str },
    #[error("invalid OpenCode server host from {origin}: expected hostname without scheme or path")]
    InvalidOpenCodeServerHost { origin: &'static str },
    #[error("invalid OpenCode server port from {origin}: expected integer in range 1..65535")]
    InvalidOpenCodeServerPort { origin: &'static str },
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct FileConfig {
    pub operator: Option<String>,
    pub opencode_remote_dir_prefix: Option<String>,
    pub opencode_server_url: Option<String>,
    pub opencode_server_host: Option<String>,
    pub opencode_server_port: Option<u16>,
    pub opencode_server_password: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedServerRuntimeResolution {
    pub url: ResolvedValue<Option<String>>,
    pub host: ResolvedValue<String>,
    pub port: ResolvedValue<u16>,
    pub password: ResolvedValue<Option<String>>,
    pub attach_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRuntimeResolution {
    pub remote_dir_prefix: ResolvedValue<Option<String>>,
    pub shared_server: SharedServerRuntimeResolution,
}

#[must_use]
pub fn resolve_operator(
    cli_operator: Option<String>,
    env_operator: Option<String>,
    file_operator: Option<String>,
) -> ResolvedValue<Option<String>> {
    if let Some(operator) = normalize_optional_value(cli_operator) {
        return ResolvedValue {
            value: Some(operator),
            source: ValueSource::Cli,
        };
    }

    if let Some(operator) = normalize_optional_value(env_operator) {
        return ResolvedValue {
            value: Some(operator),
            source: ValueSource::Env,
        };
    }

    if let Some(operator) = normalize_optional_value(file_operator) {
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

/// Resolves remote-prefix and shared-server runtime values from env/config.
///
/// Precedence for each setting in this slice is `env > config > defaults`.
///
/// # Errors
///
/// Returns [`ConfigError`] when server URL/host/port values are invalid.
pub fn resolve_remote_runtime(
    env: &impl EnvProvider,
    file_config: &FileConfig,
) -> Result<RemoteRuntimeResolution, ConfigError> {
    let remote_dir_prefix = resolve_optional_setting(
        None,
        env.get_var(OPENCODE_REMOTE_DIR_PREFIX_ENV),
        file_config.opencode_remote_dir_prefix.clone(),
    );

    let server_url = resolve_optional_setting(
        None,
        env.get_var(OPENCODE_SERVER_URL_ENV),
        file_config.opencode_server_url.clone(),
    );
    let explicit_server_url = server_url.value.is_some();
    if let Some(url) = server_url.value.as_deref() {
        validate_server_url(
            url,
            source_scope(
                server_url.source,
                "env OPENCODE_SERVER_URL",
                "config opencode_server_url",
            ),
        )?;
    }

    let server_host = resolve_string_setting_with_default(
        None,
        env.get_var(OPENCODE_SERVER_HOST_ENV),
        file_config.opencode_server_host.clone(),
        DEFAULT_OPENCODE_SERVER_HOST,
    );
    if !explicit_server_url {
        validate_server_host(
            &server_host.value,
            source_scope(
                server_host.source,
                "env OPENCODE_SERVER_HOST",
                "config opencode_server_host",
            ),
        )?;
    }

    let server_port = resolve_server_port(
        env.get_var(OPENCODE_SERVER_PORT_ENV),
        file_config.opencode_server_port,
        !explicit_server_url,
    )?;
    let server_password = resolve_optional_setting(
        None,
        env.get_var(OPENCODE_SERVER_PASSWORD_ENV),
        file_config.opencode_server_password.clone(),
    );

    let attach_url = if let Some(url) = server_url.value.clone() {
        url
    } else {
        format_attach_url(&server_host.value, server_port.value)
    };

    Ok(RemoteRuntimeResolution {
        remote_dir_prefix,
        shared_server: SharedServerRuntimeResolution {
            url: server_url,
            host: server_host,
            port: server_port,
            password: server_password,
            attach_url,
        },
    })
}

fn normalize_optional_value(value: Option<String>) -> Option<String> {
    value
        .map(|candidate| candidate.trim().to_owned())
        .filter(|candidate| !candidate.is_empty())
}

fn resolve_optional_setting(
    cli_value: Option<String>,
    env_value: Option<String>,
    file_value: Option<String>,
) -> ResolvedValue<Option<String>> {
    if let Some(value) = normalize_optional_value(cli_value) {
        return ResolvedValue {
            value: Some(value),
            source: ValueSource::Cli,
        };
    }

    if let Some(value) = normalize_optional_value(env_value) {
        return ResolvedValue {
            value: Some(value),
            source: ValueSource::Env,
        };
    }

    if let Some(value) = normalize_optional_value(file_value) {
        return ResolvedValue {
            value: Some(value),
            source: ValueSource::File,
        };
    }

    ResolvedValue {
        value: None,
        source: ValueSource::Default,
    }
}

fn resolve_string_setting_with_default(
    cli_value: Option<String>,
    env_value: Option<String>,
    file_value: Option<String>,
    default_value: &str,
) -> ResolvedValue<String> {
    if let Some(value) = normalize_optional_value(cli_value) {
        return ResolvedValue {
            value,
            source: ValueSource::Cli,
        };
    }

    if let Some(value) = normalize_optional_value(env_value) {
        return ResolvedValue {
            value,
            source: ValueSource::Env,
        };
    }

    if let Some(value) = normalize_optional_value(file_value) {
        return ResolvedValue {
            value,
            source: ValueSource::File,
        };
    }

    ResolvedValue {
        value: default_value.to_owned(),
        source: ValueSource::Default,
    }
}

fn resolve_server_port(
    env_port: Option<String>,
    file_port: Option<u16>,
    strict: bool,
) -> Result<ResolvedValue<u16>, ConfigError> {
    if let Some(raw_port) = normalize_optional_value(env_port) {
        if let Ok(parsed_port) = raw_port.parse::<u16>() {
            if parsed_port != 0 {
                return Ok(ResolvedValue {
                    value: parsed_port,
                    source: ValueSource::Env,
                });
            }
        }

        if strict {
            return Err(ConfigError::InvalidOpenCodeServerPort {
                origin: "env OPENCODE_SERVER_PORT",
            });
        }
    }

    if let Some(file_port) = file_port {
        if file_port != 0 {
            return Ok(ResolvedValue {
                value: file_port,
                source: ValueSource::File,
            });
        }

        if strict {
            return Err(ConfigError::InvalidOpenCodeServerPort {
                origin: "config opencode_server_port",
            });
        }
    }

    Ok(ResolvedValue {
        value: DEFAULT_OPENCODE_SERVER_PORT,
        source: ValueSource::Default,
    })
}

fn source_scope(
    source: ValueSource,
    env_source: &'static str,
    file_source: &'static str,
) -> &'static str {
    match source {
        ValueSource::Cli => "cli",
        ValueSource::Env => env_source,
        ValueSource::File => file_source,
        ValueSource::Default => "default",
    }
}

fn validate_server_url(url: &str, source: &'static str) -> Result<(), ConfigError> {
    let Some((scheme, remainder)) = url.split_once("://") else {
        return Err(ConfigError::InvalidOpenCodeServerUrl { origin: source });
    };
    if scheme != "http" && scheme != "https" {
        return Err(ConfigError::InvalidOpenCodeServerUrl { origin: source });
    }

    let authority = remainder.split('/').next().unwrap_or_default().trim();
    if authority.is_empty() {
        return Err(ConfigError::InvalidOpenCodeServerUrl { origin: source });
    }

    Ok(())
}

fn validate_server_host(host: &str, source: &'static str) -> Result<(), ConfigError> {
    if host.contains(char::is_whitespace) || host.contains("://") || host.contains('/') {
        return Err(ConfigError::InvalidOpenCodeServerHost { origin: source });
    }

    Ok(())
}

fn format_attach_url(host: &str, port: u16) -> String {
    let authority = if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
        format!("[{host}]")
    } else {
        host.to_owned()
    };

    format!("http://{authority}:{port}")
}

#[cfg(test)]
mod tests;
