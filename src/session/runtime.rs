use std::path::Path;

use super::SessionError;
use super::SessionIdentity;
use super::TmuxClient;
use super::resolve_remote_path;
use super::resolve_session_identity;
use crate::config::EZM_REMOTE_DIR_PREFIX_ENV;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Create,
    Attach,
}

impl SessionAction {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Attach => "attach",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLaunchOutcome {
    pub identity: SessionIdentity,
    pub remote_project_dir: std::path::PathBuf,
    pub remote_routing_active: bool,
    pub action: SessionAction,
}

/// Ensures a session exists for the current working directory.
///
/// # Errors
/// Returns an error when reading the current directory fails, when session
/// identity resolution fails, or when tmux operations fail.
pub fn ensure_current_project_session(
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    let project_dir = std::env::current_dir().map_err(SessionError::CurrentDir)?;
    ensure_project_session(&project_dir, tmux)
}

/// Ensures a session exists for the provided project directory.
///
/// # Errors
/// Returns an error when session identity resolution fails or any tmux
/// operation needed to create, validate, bootstrap, or attach fails.
pub fn ensure_project_session(
    project_dir: &Path,
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    let remote_prefix = std::env::var(EZM_REMOTE_DIR_PREFIX_ENV).ok();

    ensure_project_session_with_remote_prefix(project_dir, remote_prefix.as_deref(), tmux)
}

/// Ensures a session exists for the provided project directory using an
/// explicit remote remap prefix when supplied.
///
/// # Errors
/// Returns an error when session identity resolution fails or any tmux
/// operation needed to create, validate, bootstrap, or attach fails.
pub fn ensure_project_session_with_remote_prefix(
    project_dir: &Path,
    remote_prefix: Option<&str>,
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    let identity = resolve_session_identity(project_dir)?;
    let remote_path = resolve_remote_path(&identity.project_dir, remote_prefix)?;
    let remote_project_dir = remote_path.effective_path;
    let action = if tmux.session_exists(&identity.session_name)? {
        tmux.validate_session_invariants(&identity.session_name)?;
        SessionAction::Attach
    } else {
        tmux.create_detached_session(&identity.session_name, &identity.project_dir)?;
        tmux.bootstrap_default_layout(&identity.session_name, &identity.project_dir)?;
        SessionAction::Create
    };
    tmux.auxiliary_viewer(&identity.session_name, true)?;
    tmux.attach_session(&identity.session_name)?;

    Ok(SessionLaunchOutcome {
        identity,
        remote_project_dir,
        remote_routing_active: remote_path.remapped,
        action,
    })
}
