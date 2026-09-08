use super::SessionError;
use super::TmuxClient;
use crate::config::SessionRuntimeContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuxiliaryViewerAction {
    Created,
    Reused,
    Closed,
    SkippedUnavailable,
}

impl AuxiliaryViewerAction {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Reused => "reused",
            Self::Closed => "closed",
            Self::SkippedUnavailable => "skipped-unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuxiliaryViewerOutcome {
    pub session_name: String,
    pub action: AuxiliaryViewerAction,
    pub window_name: String,
    pub window_id: Option<String>,
}

/// Creates/reuses or closes the auxiliary viewer window.
///
/// # Errors
/// Returns an error when the tmux backend cannot reconcile the auxiliary
/// viewer surface.
pub fn auxiliary_viewer(
    session_name: &str,
    open: bool,
    use_tssh: bool,
    use_mosh: bool,
    tmux: &impl TmuxClient,
) -> Result<AuxiliaryViewerOutcome, SessionError> {
    tmux.auxiliary_viewer(session_name, open, use_tssh, use_mosh)
}

/// Creates/reuses or closes the auxiliary viewer using the caller's resolved
/// project context. No remote settings are read from the process environment.
///
/// # Errors
/// Returns an error when the tmux backend cannot reconcile the auxiliary
/// viewer surface.
pub fn auxiliary_viewer_with_runtime_context(
    session_name: &str,
    open: bool,
    context: &SessionRuntimeContext,
    tmux: &impl TmuxClient,
) -> Result<AuxiliaryViewerOutcome, SessionError> {
    tmux.auxiliary_viewer_with_runtime_context(session_name, open, context)
}
