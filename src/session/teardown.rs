use super::SessionError;
use super::TmuxClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeardownOutcome {
    pub session_name: String,
    pub helper_sessions_removed: usize,
    pub helper_processes_removed: usize,
    pub project_session_removed: bool,
}

/// Executes teardown for one project session and its helpers.
///
/// # Errors
/// Returns an error when tmux teardown actions fail unexpectedly.
pub fn teardown_session(
    session_name: &str,
    tmux: &impl TmuxClient,
) -> Result<TeardownOutcome, SessionError> {
    tmux.teardown_session(session_name)
}
