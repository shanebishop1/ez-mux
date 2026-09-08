use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::PathBuf;

use serde::Deserialize;
use thiserror::Error;

mod load;
mod runtime;

pub use load::{load_config, resolve_config_path};
pub use runtime::{
    AuxiliaryRuntimeResolution, OpencodeThemeRuntimeResolution, RemoteRuntimeResolution,
    ResolvedValue, RuntimeContext, SessionRuntimeContext, SharedServerRuntimeResolution,
    ValueSource,
};

pub const EZM_CONFIG_ENV: &str = "EZM_CONFIG";
pub const EZM_BIN_ENV: &str = "EZM_BIN";
pub const EZM_REMOTE_PATH_ENV: &str = "EZM_REMOTE_PATH";
pub const EZM_REMOTE_SERVER_URL_ENV: &str = "EZM_REMOTE_SERVER_URL";
pub const EZM_USE_TSSH_ENV: &str = "EZM_USE_TSSH";
pub const EZM_USE_MOSH_ENV: &str = "EZM_USE_MOSH";
pub const PERLES_DIR_ENV: &str = "PERLES_DIR";
pub const PERLES_DB_ENV: &str = "PERLES_DB";
pub const LEGACY_BEADS_DIR_ENV: &str = "BEADS_DIR";
pub const LEGACY_BEADS_DB_ENV: &str = "BEADS_DB";
pub const OPENCODE_SERVER_URL_ENV: &str = "OPENCODE_SERVER_URL";
pub const OPENCODE_SERVER_PASSWORD_ENV: &str = "OPENCODE_SERVER_PASSWORD";
pub const EZM_RUNTIME_CONTEXT_VERSION_OPTION: &str = "@ezm_runtime_context_version";
pub const EZM_RUNTIME_REMOTE_PATH_OPTION: &str = "@ezm_runtime_remote_path";
pub const EZM_RUNTIME_REMOTE_SERVER_URL_OPTION: &str = "@ezm_runtime_remote_server_url";
pub const EZM_RUNTIME_USE_TSSH_OPTION: &str = "@ezm_runtime_use_tssh";
pub const EZM_RUNTIME_USE_MOSH_OPTION: &str = "@ezm_runtime_use_mosh";
pub const EZM_RUNTIME_PERLES_DIR_OPTION: &str = "@ezm_runtime_perles_dir";
pub const EZM_RUNTIME_PERLES_DB_OPTION: &str = "@ezm_runtime_perles_db";
pub const EZM_RUNTIME_OPENCODE_SERVER_URL_OPTION: &str = "@ezm_runtime_opencode_server_url";
pub const EZM_RUNTIME_AGENT_COMMAND_OPTION: &str = "@ezm_runtime_agent_command";
pub const EZM_RUNTIME_OPENCODE_THEMES_ENABLED_OPTION: &str = "@ezm_runtime_opencode_themes_enabled";
pub const EZM_RUNTIME_OPENCODE_THEME_PREFIX: &str = "@ezm_runtime_opencode_theme_";
pub const EZM_RUNTIME_CONTEXT_VERSION: &str = "1";
pub const MIN_PANE_COUNT: u8 = 1;
pub const MAX_PANE_COUNT: u8 = 5;
pub const DEFAULT_PANE_COUNT: u8 = 5;

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
    #[error(
        "invalid OpenCode server URL from {origin}: URL userinfo is unsupported; provide credentials with OPENCODE_SERVER_PASSWORD"
    )]
    UnsupportedOpenCodeServerUrlUserinfo { origin: &'static str },
    #[error(
        "invalid pane count from {origin}: expected value in range {MIN_PANE_COUNT}..={MAX_PANE_COUNT}, got {value}"
    )]
    InvalidPaneCount { origin: &'static str, value: u8 },
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct FileConfig {
    pub panes: Option<u8>,
    pub ezm_remote_path: Option<String>,
    pub ezm_remote_server_url: Option<String>,
    pub ezm_use_tssh: Option<bool>,
    pub ezm_use_mosh: Option<bool>,
    pub perles_dir: Option<String>,
    pub perles_db: Option<String>,
    pub opencode_server_url: Option<String>,
    pub opencode_server_password: Option<String>,
    pub agent_command: Option<String>,
    pub opencode_slot_themes_enabled: Option<bool>,
    pub opencode_slot_themes: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub values: FileConfig,
}

const DEFAULT_OPENCODE_SLOT_THEMES: [(u8, &str); 5] = [
    (1, "nightowl"),
    (2, "orng"),
    (3, "osaka-jade"),
    (4, "catppuccin"),
    (5, "monokai"),
];

/// Resolves the complete runtime boundary from env/config.
///
/// Precedence for each setting in this slice is `env > config > defaults`.
///
/// # Errors
///
/// Returns [`ConfigError`] when server URL values are invalid.
pub fn resolve_remote_runtime(
    env: &impl EnvProvider,
    file_config: &FileConfig,
) -> Result<RemoteRuntimeResolution, ConfigError> {
    let remote_path = resolve_remote_path(env, file_config);
    let remote_server_url = resolve_optional_setting(
        None,
        env.get_var(EZM_REMOTE_SERVER_URL_ENV),
        file_config.ezm_remote_server_url.clone(),
    );
    let use_mosh = resolve_optional_bool_setting(
        None,
        env.get_var(EZM_USE_MOSH_ENV),
        file_config.ezm_use_mosh,
        false,
    );
    let use_tssh = resolve_optional_bool_setting(
        None,
        env.get_var(EZM_USE_TSSH_ENV),
        file_config.ezm_use_tssh,
        false,
    );

    let server_url = resolve_optional_setting(
        None,
        env.get_var(OPENCODE_SERVER_URL_ENV),
        file_config.opencode_server_url.clone(),
    );
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
    let server_password = resolve_optional_setting(
        None,
        env.get_var(OPENCODE_SERVER_PASSWORD_ENV),
        file_config.opencode_server_password.clone(),
    );

    Ok(RemoteRuntimeResolution {
        remote_path,
        remote_server_url,
        use_tssh,
        use_mosh,
        shared_server: SharedServerRuntimeResolution {
            url: server_url,
            password: server_password,
        },
    })
}

/// Resolves all values used by startup, mode, popup, and auxiliary launches.
///
/// This is intentionally the only app-level resolver. Lower layers receive
/// its result and must not reinterpret process environment values.
///
/// # Errors
///
/// Returns [`ConfigError`] when a resolved `OpenCode` server URL is invalid.
pub fn resolve_runtime_context(
    env: &impl EnvProvider,
    file_config: &FileConfig,
) -> Result<RuntimeContext, ConfigError> {
    Ok(RuntimeContext {
        remote: resolve_remote_runtime(env, file_config)?,
        auxiliary: resolve_auxiliary_runtime(env, file_config),
        agent_command: resolve_agent_command(file_config),
        opencode_theme: resolve_opencode_theme_runtime(file_config),
    })
}

#[must_use]
pub fn resolve_auxiliary_runtime(
    env: &impl EnvProvider,
    file_config: &FileConfig,
) -> AuxiliaryRuntimeResolution {
    AuxiliaryRuntimeResolution {
        perles_dir: resolve_optional_setting(
            None,
            env.get_var(PERLES_DIR_ENV)
                .or_else(|| env.get_var(LEGACY_BEADS_DIR_ENV)),
            file_config.perles_dir.clone(),
        ),
        perles_db: resolve_optional_setting(
            None,
            env.get_var(PERLES_DB_ENV)
                .or_else(|| env.get_var(LEGACY_BEADS_DB_ENV)),
            file_config.perles_db.clone(),
        ),
    }
}

#[must_use]
pub fn resolve_opencode_theme_runtime(file_config: &FileConfig) -> OpencodeThemeRuntimeResolution {
    let enabled = file_config.opencode_slot_themes_enabled.unwrap_or(true);
    let mut themes_by_slot = default_opencode_slot_themes();

    if let Some(overrides) = file_config.opencode_slot_themes.as_ref() {
        apply_opencode_slot_theme_overrides(&mut themes_by_slot, overrides);
    }

    OpencodeThemeRuntimeResolution {
        enabled,
        themes_by_slot,
    }
}

#[must_use]
pub fn resolve_agent_command(file_config: &FileConfig) -> Option<String> {
    file_config
        .agent_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

/// Resolves startup pane count from CLI/config/defaults.
///
/// Precedence for this setting is `cli > config > default(5)`.
///
/// # Errors
///
/// Returns [`ConfigError::InvalidPaneCount`] when a resolved value is outside `1..=5`.
pub fn resolve_pane_count(
    cli_pane_count: Option<u8>,
    file_config: &FileConfig,
) -> Result<ResolvedValue<u8>, ConfigError> {
    let resolved = if let Some(value) = cli_pane_count {
        ResolvedValue {
            value,
            source: ValueSource::Cli,
        }
    } else if let Some(value) = file_config.panes {
        ResolvedValue {
            value,
            source: ValueSource::File,
        }
    } else {
        ResolvedValue {
            value: DEFAULT_PANE_COUNT,
            source: ValueSource::Default,
        }
    };

    if pane_count_in_range(resolved.value) {
        Ok(resolved)
    } else {
        Err(ConfigError::InvalidPaneCount {
            origin: pane_count_origin(resolved.source),
            value: resolved.value,
        })
    }
}

fn default_opencode_slot_themes() -> HashMap<u8, String> {
    DEFAULT_OPENCODE_SLOT_THEMES
        .iter()
        .map(|(slot_id, theme)| (*slot_id, (*theme).to_owned()))
        .collect()
}

fn pane_count_in_range(value: u8) -> bool {
    (MIN_PANE_COUNT..=MAX_PANE_COUNT).contains(&value)
}

fn pane_count_origin(source: ValueSource) -> &'static str {
    match source {
        ValueSource::Cli => "cli --panes",
        ValueSource::File => "config panes",
        ValueSource::Env => "env",
        ValueSource::Session => "session",
        ValueSource::Default => "default",
    }
}

fn apply_opencode_slot_theme_overrides(
    themes_by_slot: &mut HashMap<u8, String>,
    overrides: &HashMap<String, String>,
) {
    for (slot_key, theme) in overrides {
        let Some(slot_id) = parse_slot_theme_key(slot_key) else {
            continue;
        };
        let normalized_theme = theme.trim();
        if normalized_theme.is_empty() {
            themes_by_slot.remove(&slot_id);
            continue;
        }
        themes_by_slot.insert(slot_id, normalized_theme.to_owned());
    }
}

fn parse_slot_theme_key(slot_key: &str) -> Option<u8> {
    let slot_id = slot_key.trim().parse::<u8>().ok()?;
    if (1..=5).contains(&slot_id) {
        Some(slot_id)
    } else {
        None
    }
}

fn resolve_remote_path(
    env: &impl EnvProvider,
    file_config: &FileConfig,
) -> ResolvedValue<Option<String>> {
    if let Some(value) = normalize_optional_value(env.get_var(EZM_REMOTE_PATH_ENV)) {
        return ResolvedValue {
            value: Some(value),
            source: ValueSource::Env,
        };
    }

    if let Some(value) = normalize_optional_value(file_config.ezm_remote_path.clone()) {
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

fn resolve_optional_bool_setting(
    cli_value: Option<bool>,
    env_value: Option<String>,
    file_value: Option<bool>,
    default: bool,
) -> ResolvedValue<bool> {
    if let Some(value) = cli_value {
        return ResolvedValue {
            value,
            source: ValueSource::Cli,
        };
    }

    if let Some(value) = normalize_optional_bool_env_value(env_value) {
        return ResolvedValue {
            value,
            source: ValueSource::Env,
        };
    }

    if let Some(value) = file_value {
        return ResolvedValue {
            value,
            source: ValueSource::File,
        };
    }

    ResolvedValue {
        value: default,
        source: ValueSource::Default,
    }
}

fn normalize_optional_bool_env_value(value: Option<String>) -> Option<bool> {
    let value = value?.trim().to_ascii_lowercase();
    if value.is_empty() {
        return None;
    }

    if matches!(value.as_str(), "0" | "false" | "no" | "off") {
        return Some(false);
    }

    Some(true)
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
        ValueSource::Session => "session",
        ValueSource::Default => "default",
    }
}

pub(crate) fn validate_server_url(url: &str, source: &'static str) -> Result<(), ConfigError> {
    let Some((scheme, remainder)) = url.split_once("://") else {
        return Err(ConfigError::InvalidOpenCodeServerUrl { origin: source });
    };
    if scheme != "http" && scheme != "https" {
        return Err(ConfigError::InvalidOpenCodeServerUrl { origin: source });
    }

    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    if authority.is_empty()
        || authority.chars().any(char::is_whitespace)
        || authority.chars().any(char::is_control)
    {
        return Err(ConfigError::InvalidOpenCodeServerUrl { origin: source });
    }
    if authority.contains('@') {
        return Err(ConfigError::UnsupportedOpenCodeServerUrlUserinfo { origin: source });
    }

    Ok(())
}

/// Removes URL userinfo while preserving the scheme, host, port, and suffix.
/// This is used only to scrub state persisted by older versions; new input is
/// rejected instead of silently changing authentication semantics.
pub(crate) fn server_url_without_userinfo(url: &str) -> Option<String> {
    let (scheme, remainder) = url.split_once("://")?;
    let authority_end = remainder.find(['/', '?', '#']).unwrap_or(remainder.len());
    let authority = &remainder[..authority_end];
    let (_, host_port) = authority.rsplit_once('@')?;
    if host_port.is_empty() {
        return None;
    }

    let canonical = format!("{scheme}://{host_port}{}", &remainder[authority_end..]);
    validate_server_url(&canonical, "persisted session context")
        .is_ok()
        .then_some(canonical)
}

#[cfg(test)]
mod tests;
