use std::collections::HashMap;
use std::fmt;

use super::{FileConfig, resolve_opencode_theme_runtime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSource {
    Cli,
    Env,
    File,
    Session,
    Default,
}

impl ValueSource {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Env => "env",
            Self::File => "file",
            Self::Session => "session",
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
pub struct AuxiliaryRuntimeResolution {
    pub perles_dir: ResolvedValue<Option<String>>,
    pub perles_db: ResolvedValue<Option<String>>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SharedServerRuntimeResolution {
    pub url: ResolvedValue<Option<String>>,
    pub password: ResolvedValue<Option<String>>,
}

impl fmt::Debug for SharedServerRuntimeResolution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedServerRuntimeResolution")
            .field("url", &self.url)
            .field(
                "password",
                &ResolvedValue {
                    value: self.password.value.as_ref().map(|_| "<redacted>"),
                    source: self.password.source,
                },
            )
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct RemoteRuntimeResolution {
    pub remote_path: ResolvedValue<Option<String>>,
    pub remote_server_url: ResolvedValue<Option<String>>,
    pub use_tssh: ResolvedValue<bool>,
    pub use_mosh: ResolvedValue<bool>,
    pub shared_server: SharedServerRuntimeResolution,
}

impl fmt::Debug for RemoteRuntimeResolution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteRuntimeResolution")
            .field("remote_path", &self.remote_path)
            .field("remote_server_url", &self.remote_server_url)
            .field("use_tssh", &self.use_tssh)
            .field("use_mosh", &self.use_mosh)
            .field("shared_server", &self.shared_server)
            .finish()
    }
}

/// The single resolved boundary for a launch and all of its internal actions.
///
/// A top-level invocation resolves `env > config > defaults`. Once a project
/// session exists, its persisted session context takes precedence for internal
/// commands (`session > current invocation`). The password deliberately stays
/// in this in-memory value only; it is never part of [`SessionRuntimeContext`].
#[derive(Clone, PartialEq, Eq)]
pub struct RuntimeContext {
    pub remote: RemoteRuntimeResolution,
    pub auxiliary: AuxiliaryRuntimeResolution,
    pub agent_command: Option<String>,
    pub opencode_theme: OpencodeThemeRuntimeResolution,
}

impl fmt::Debug for RuntimeContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeContext")
            .field("remote", &self.remote)
            .field("auxiliary", &self.auxiliary)
            .field("agent_command", &self.agent_command)
            .field("opencode_theme", &self.opencode_theme)
            .finish()
    }
}

/// The non-secret portion of [`RuntimeContext`] that may be persisted in a
/// project session. It is intentionally independent of process environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRuntimeContext {
    pub remote_path: Option<String>,
    pub remote_server_url: Option<String>,
    pub use_tssh: bool,
    pub use_mosh: bool,
    pub perles_dir: Option<String>,
    pub perles_db: Option<String>,
    pub shared_server_url: Option<String>,
    pub agent_command: Option<String>,
    pub opencode_themes_enabled: bool,
    pub opencode_themes_by_slot: HashMap<u8, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpencodeThemeRuntimeResolution {
    pub enabled: bool,
    pub themes_by_slot: HashMap<u8, String>,
}

impl OpencodeThemeRuntimeResolution {
    #[must_use]
    pub fn theme_for_slot(&self, slot_id: u8) -> Option<&str> {
        if !self.enabled {
            return None;
        }

        self.themes_by_slot.get(&slot_id).map(String::as_str)
    }
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self {
            remote: RemoteRuntimeResolution {
                remote_path: default_optional_value(),
                remote_server_url: default_optional_value(),
                use_tssh: default_bool_value(),
                use_mosh: default_bool_value(),
                shared_server: SharedServerRuntimeResolution {
                    url: default_optional_value(),
                    password: default_optional_value(),
                },
            },
            auxiliary: AuxiliaryRuntimeResolution {
                perles_dir: default_optional_value(),
                perles_db: default_optional_value(),
            },
            agent_command: None,
            opencode_theme: resolve_opencode_theme_runtime(&FileConfig::default()),
        }
    }
}

fn default_optional_value<T>() -> ResolvedValue<Option<T>> {
    ResolvedValue {
        value: None,
        source: ValueSource::Default,
    }
}

fn default_bool_value() -> ResolvedValue<bool> {
    ResolvedValue {
        value: false,
        source: ValueSource::Default,
    }
}

impl RuntimeContext {
    #[must_use]
    pub fn session_context(&self) -> SessionRuntimeContext {
        SessionRuntimeContext {
            remote_path: self.remote.remote_path.value.clone(),
            remote_server_url: self.remote.remote_server_url.value.clone(),
            use_tssh: self.remote.use_tssh.value,
            use_mosh: self.remote.use_mosh.value,
            perles_dir: self.auxiliary.perles_dir.value.clone(),
            perles_db: self.auxiliary.perles_db.value.clone(),
            shared_server_url: self.remote.shared_server.url.value.clone(),
            agent_command: self.agent_command.clone(),
            opencode_themes_enabled: self.opencode_theme.enabled,
            opencode_themes_by_slot: self.opencode_theme.themes_by_slot.clone(),
        }
    }
}
