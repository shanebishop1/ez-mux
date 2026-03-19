use std::path::{Path, PathBuf};

use crate::config::{EnvProvider, OperatingSystem};

use super::LoggingError;

/// Resolves the primary OS-safe log root for this launch.
///
/// # Errors
///
/// Returns [`LoggingError::MissingHome`] when default expansion requires `HOME`.
pub fn resolve_primary_log_root(
    env: &impl EnvProvider,
    os: OperatingSystem,
) -> Result<PathBuf, LoggingError> {
    match os {
        OperatingSystem::Linux => {
            if let Some(xdg_home) = env_var_non_empty(env, "XDG_STATE_HOME") {
                return Ok(PathBuf::from(xdg_home).join("ez-mux").join("logs"));
            }

            let home = env_var_non_empty(env, "HOME")
                .ok_or(LoggingError::MissingHome { os: os.label() })?;

            Ok(PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("ez-mux")
                .join("logs"))
        }
        OperatingSystem::MacOs => {
            let home = env_var_non_empty(env, "HOME")
                .ok_or(LoggingError::MissingHome { os: os.label() })?;

            Ok(PathBuf::from(home)
                .join("Library")
                .join("Logs")
                .join("ez-mux"))
        }
    }
}

#[must_use]
pub fn fallback_log_root(fallback_base: &Path) -> PathBuf {
    fallback_base.join("ez-mux").join("logs")
}

fn env_var_non_empty(env: &impl EnvProvider, key: &str) -> Option<String> {
    env.get_var(key).filter(|value| !value.is_empty())
}
