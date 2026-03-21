use std::collections::HashMap;
use std::fs;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

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

    use super::*;

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
    fn whitespace_only_env_values_are_treated_as_unset() {
        let mut env = HashMap::new();
        env.insert(String::from(EZM_CONFIG_ENV), String::from("   \t"));
        env.insert(String::from("XDG_CONFIG_HOME"), String::from("   "));
        env.insert(String::from("HOME"), String::from("/tmp/home"));

        let path = resolve_config_path(&env, OperatingSystem::Linux).expect("path should resolve");
        assert_eq!(
            path,
            std::path::PathBuf::from("/tmp/home/.config/ez-mux/config.toml")
        );
    }

    #[test]
    fn unsupported_platform_returns_typed_error() {
        let env = HashMap::<String, String>::new();

        let error = resolve_config_path(&env, OperatingSystem::Unsupported).expect_err("must fail");
        assert!(matches!(error, ConfigError::UnsupportedPlatform { .. }));
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
    fn remote_runtime_prefers_env_over_file_values() {
        let mut env = HashMap::new();
        env.insert(
            String::from(OPENCODE_REMOTE_DIR_PREFIX_ENV),
            String::from("/env/remotes"),
        );
        env.insert(
            String::from(OPENCODE_SERVER_URL_ENV),
            String::from("https://env.example:4242"),
        );
        env.insert(
            String::from(OPENCODE_SERVER_PASSWORD_ENV),
            String::from("env-secret"),
        );

        let file = FileConfig {
            operator: None,
            opencode_remote_dir_prefix: Some(String::from("/file/remotes")),
            opencode_server_url: Some(String::from("https://file.example:4096")),
            opencode_server_host: Some(String::from("file-host")),
            opencode_server_port: Some(5000),
            opencode_server_password: Some(String::from("file-secret")),
        };

        let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

        assert_eq!(
            resolved.remote_dir_prefix,
            ResolvedValue {
                value: Some(String::from("/env/remotes")),
                source: ValueSource::Env,
            }
        );
        assert_eq!(
            resolved.shared_server.url,
            ResolvedValue {
                value: Some(String::from("https://env.example:4242")),
                source: ValueSource::Env,
            }
        );
        assert_eq!(
            resolved.shared_server.attach_url,
            "https://env.example:4242"
        );
        assert_eq!(
            resolved.shared_server.password,
            ResolvedValue {
                value: Some(String::from("env-secret")),
                source: ValueSource::Env,
            }
        );
    }

    #[test]
    fn remote_runtime_uses_config_when_env_is_missing() {
        let env = HashMap::<String, String>::new();
        let file = FileConfig {
            operator: None,
            opencode_remote_dir_prefix: Some(String::from("/file/remotes")),
            opencode_server_url: None,
            opencode_server_host: Some(String::from("server.internal")),
            opencode_server_port: Some(7443),
            opencode_server_password: Some(String::from("file-secret")),
        };

        let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

        assert_eq!(
            resolved.remote_dir_prefix,
            ResolvedValue {
                value: Some(String::from("/file/remotes")),
                source: ValueSource::File,
            }
        );
        assert_eq!(
            resolved.shared_server.url,
            ResolvedValue {
                value: None,
                source: ValueSource::Default,
            }
        );
        assert_eq!(
            resolved.shared_server.host,
            ResolvedValue {
                value: String::from("server.internal"),
                source: ValueSource::File,
            }
        );
        assert_eq!(
            resolved.shared_server.port,
            ResolvedValue {
                value: 7443,
                source: ValueSource::File,
            }
        );
        assert_eq!(
            resolved.shared_server.attach_url,
            "http://server.internal:7443"
        );
        assert_eq!(
            resolved.shared_server.password,
            ResolvedValue {
                value: Some(String::from("file-secret")),
                source: ValueSource::File,
            }
        );
    }

    #[test]
    fn remote_runtime_defaults_host_and_port_when_unset() {
        let env = HashMap::<String, String>::new();
        let file = FileConfig::default();

        let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

        assert_eq!(
            resolved.shared_server.host,
            ResolvedValue {
                value: String::from(DEFAULT_OPENCODE_SERVER_HOST),
                source: ValueSource::Default,
            }
        );
        assert_eq!(
            resolved.shared_server.port,
            ResolvedValue {
                value: DEFAULT_OPENCODE_SERVER_PORT,
                source: ValueSource::Default,
            }
        );
        assert_eq!(resolved.shared_server.attach_url, "http://127.0.0.1:4096");
    }

    #[test]
    fn invalid_env_server_port_fails_fast() {
        let mut env = HashMap::new();
        env.insert(
            String::from(OPENCODE_SERVER_PORT_ENV),
            String::from("not-a-port"),
        );

        let error = resolve_remote_runtime(&env, &FileConfig::default())
            .expect_err("invalid port should fail fast");
        assert!(matches!(
            error,
            ConfigError::InvalidOpenCodeServerPort {
                origin: "env OPENCODE_SERVER_PORT"
            }
        ));
    }

    #[test]
    fn invalid_server_url_fails_fast() {
        let mut env = HashMap::new();
        env.insert(
            String::from(OPENCODE_SERVER_URL_ENV),
            String::from("localhost:4096"),
        );

        let error = resolve_remote_runtime(&env, &FileConfig::default())
            .expect_err("invalid url should fail fast");
        assert!(matches!(
            error,
            ConfigError::InvalidOpenCodeServerUrl {
                origin: "env OPENCODE_SERVER_URL"
            }
        ));
    }

    #[test]
    fn invalid_server_host_fails_fast() {
        let env = HashMap::<String, String>::new();
        let file = FileConfig {
            opencode_server_host: Some(String::from("http://bad-host")),
            ..FileConfig::default()
        };

        let error = resolve_remote_runtime(&env, &file).expect_err("invalid host should fail fast");
        assert!(matches!(
            error,
            ConfigError::InvalidOpenCodeServerHost {
                origin: "config opencode_server_host"
            }
        ));
    }

    #[test]
    fn explicit_server_url_overrides_invalid_host_and_port_inputs() {
        let mut env = HashMap::new();
        env.insert(
            String::from(OPENCODE_SERVER_URL_ENV),
            String::from("https://shared.example:9443"),
        );
        env.insert(
            String::from(OPENCODE_SERVER_HOST_ENV),
            String::from("http://bad-host"),
        );
        env.insert(
            String::from(OPENCODE_SERVER_PORT_ENV),
            String::from("bad-port"),
        );

        let resolved =
            resolve_remote_runtime(&env, &FileConfig::default()).expect("url should take priority");

        assert_eq!(
            resolved.shared_server.attach_url,
            "https://shared.example:9443"
        );
        assert_eq!(
            resolved.shared_server.url,
            ResolvedValue {
                value: Some(String::from("https://shared.example:9443")),
                source: ValueSource::Env,
            }
        );
    }

    #[test]
    fn operator_env_constant_is_contract_stable() {
        assert_eq!(OPERATOR_ENV, "OPERATOR");
    }

    #[test]
    fn remote_shared_server_env_constants_are_contract_stable() {
        assert_eq!(OPENCODE_REMOTE_DIR_PREFIX_ENV, "OPENCODE_REMOTE_DIR_PREFIX");
        assert_eq!(OPENCODE_SERVER_URL_ENV, "OPENCODE_SERVER_URL");
        assert_eq!(OPENCODE_SERVER_HOST_ENV, "OPENCODE_SERVER_HOST");
        assert_eq!(OPENCODE_SERVER_PORT_ENV, "OPENCODE_SERVER_PORT");
        assert_eq!(OPENCODE_SERVER_PASSWORD_ENV, "OPENCODE_SERVER_PASSWORD");
        assert_eq!(DEFAULT_OPENCODE_SERVER_HOST, "127.0.0.1");
        assert_eq!(DEFAULT_OPENCODE_SERVER_PORT, 4096);
    }
}
