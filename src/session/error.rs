use std::io;
use std::path::PathBuf;

use thiserror::Error;

use super::slot_registry::SlotRegistryError;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("failed resolving current working directory: {0}")]
    CurrentDir(#[source] io::Error),
    #[error("failed canonicalizing project path {path}: {source}")]
    CanonicalizeProjectPath {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid remote path `{prefix}`: expected absolute path")]
    InvalidRemotePathMappingPrefix { prefix: String },
    #[error(
        "invalid remote ssh authority `{value}`: {reason}. Set EZM_REMOTE_SERVER_URL (or ezm_remote_server_url) to host, host:port, [ipv6], [ipv6]:port, or <scheme>://<authority>"
    )]
    InvalidRemoteSshAuthority { value: String, reason: String },
    #[error("agent mode requires shared-server attach configuration")]
    MissingSharedServerAttachConfig,
    #[error(
        "agent mode shared-server URL must be an absolute http(s) URL without userinfo; provide credentials with OPENCODE_SERVER_PASSWORD"
    )]
    InvalidSharedServerAttachUrl,
    #[error(
        "session `{session_name}` has no owned runtime-context marker and no recoverable session-scoped legacy settings; refusing to import the current process/config context. Run `ezm kill` from the owning project and relaunch to reconcile explicitly"
    )]
    UnsafeLegacyRuntimeContextMigration { session_name: String },
    #[error(
        "session `{session_name}` has legacy OpenCode server settings that cannot be migrated safely; URL userinfo is unsupported. Reconcile with a credential-free OPENCODE_SERVER_URL and OPENCODE_SERVER_PASSWORD"
    )]
    UnsafeLegacyOpenCodeUrlMigration { session_name: String },
    #[error(
        "session `{session_name}` has an invalid OpenCode server URL; URL userinfo is unsupported and credentials must be provided with OPENCODE_SERVER_PASSWORD"
    )]
    UnsafeSessionOpenCodeUrl { session_name: String },
    #[error("tmux command `{command}` failed: {stderr}")]
    TmuxCommandFailed { command: String, stderr: String },
    #[error(
        "tmux was not found on PATH while running `{command}`; install tmux 3.2+ and ensure it is on PATH"
    )]
    TmuxNotFound { command: String },
    #[error("failed spawning tmux command `{command}`: {source}")]
    TmuxSpawnFailed {
        command: String,
        #[source]
        source: io::Error,
    },
    #[error(
        "git is required to discover worktrees for `{project_dir}`, but was not found on PATH; install Git and ensure it is on PATH (or use --no-worktrees)"
    )]
    GitNotFound { project_dir: PathBuf },
    #[error("failed registering SIGINT handler: {source}")]
    SignalRegistrationFailed {
        #[source]
        source: io::Error,
    },
    #[error("interrupted")]
    Interrupted,
    #[error(transparent)]
    SlotRegistry(#[from] SlotRegistryError),
}

impl SessionError {
    pub(crate) fn tmux_spawn(command: impl Into<String>, source: io::Error) -> Self {
        let command = command.into();
        if source.kind() == io::ErrorKind::NotFound {
            return Self::TmuxNotFound { command };
        }

        Self::TmuxSpawnFailed { command, source }
    }

    pub(crate) fn git_not_found(project_dir: &std::path::Path) -> Self {
        Self::GitNotFound {
            project_dir: project_dir.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::SessionError;

    #[test]
    fn missing_tmux_error_explains_install_and_path_recovery() {
        let error =
            SessionError::tmux_spawn("new-session -d", io::Error::from(io::ErrorKind::NotFound));
        let rendered = error.to_string();

        assert!(rendered.contains("install tmux 3.2+"));
        assert!(rendered.contains("PATH"));
    }

    #[test]
    fn missing_git_error_explains_no_worktrees_escape_hatch() {
        let error = SessionError::git_not_found(std::path::Path::new("/tmp/project"));
        let rendered = error.to_string();

        assert!(rendered.contains("Git"));
        assert!(rendered.contains("PATH"));
        assert!(rendered.contains("--no-worktrees"));
    }

    #[test]
    fn permission_denied_tmux_spawn_is_not_classified_as_missing() {
        let error = SessionError::tmux_spawn(
            "new-session -d",
            io::Error::from(io::ErrorKind::PermissionDenied),
        );

        assert!(matches!(error, SessionError::TmuxSpawnFailed { .. }));
    }
}
