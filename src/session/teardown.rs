use super::SessionError;
use super::TmuxClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeardownOutcome {
    pub session_name: String,
    pub helper_sessions_removed: usize,
    pub helper_processes_removed: usize,
    pub project_session_removed: bool,
}

/// Resources that a bootstrap invocation has positively identified as its own.
///
/// This is intentionally narrower than the resources discovered by an
/// ordinary, explicitly requested teardown.  In particular, helper names are
/// recorded only after the corresponding exact helper identity was absent at
/// the start of bootstrap.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeardownOwnership {
    pub session_name: String,
    pub helper_sessions: Vec<String>,
}

impl TeardownOwnership {
    pub(crate) fn for_new_session(session_name: &str) -> Self {
        Self {
            session_name: session_name.to_owned(),
            helper_sessions: Vec::new(),
        }
    }

    pub(crate) fn add_helper_session(&mut self, session_name: String) {
        if !self.helper_sessions.contains(&session_name) {
            self.helper_sessions.push(session_name);
        }
    }

    /// Returns helper identities that startup can create asynchronously.
    pub(crate) fn bootstrap_helper_sessions(session_name: &str) -> Vec<String> {
        let mut helpers = vec![format!("{session_name}__mode_cache")];
        helpers.extend((1_u8..=5).map(|slot_id| format!("{session_name}__popup_slot_{slot_id}")));
        helpers
    }

    /// Returns exact helper identities supported by the shipped explicit
    /// teardown command.  No prefix matching belongs in this list.
    pub(crate) fn explicit_helper_sessions(session_name: &str) -> Vec<String> {
        let mut helpers = Self::bootstrap_helper_sessions(session_name);
        helpers.extend((1_u8..=5).map(|slot_id| format!("{session_name}__mode_slot_{slot_id}")));
        helpers
    }
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
