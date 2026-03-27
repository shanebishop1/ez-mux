use super::super::SessionError;
use super::super::options::{
    required_pane_option, required_session_option, set_pane_option, set_session_option,
};

#[derive(Debug, Clone)]
pub(super) struct ModeMetadataState {
    pub(super) session_cwd: String,
    pub(super) session_mode: String,
    pub(super) pane_cwd: String,
    pub(super) pane_mode: String,
    pub(super) pane_worktree: String,
}

pub(super) fn load_previous_mode_metadata(
    session_name: &str,
    slot_id: u8,
    slot_cwd_key: &str,
    slot_mode_key: &str,
    pane_id: &str,
) -> Result<ModeMetadataState, SessionError> {
    let existing_mode = required_session_option(session_name, slot_mode_key)?;
    let existing_pane_cwd = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_cwd")?;
    let existing_pane_mode =
        required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_mode")?;
    let existing_pane_worktree =
        required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_worktree")?;
    let pane_slot_id = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_id")?;
    if pane_slot_id != slot_id.to_string() {
        return Err(SessionError::TmuxCommandFailed {
            command: format!("switch-slot-mode -t {session_name} --slot {slot_id}"),
            stderr: format!(
                "slot metadata mismatch: pane {pane_id} has @ezm_slot_id={pane_slot_id}"
            ),
        });
    }

    Ok(ModeMetadataState {
        session_cwd: required_session_option(session_name, slot_cwd_key)?,
        session_mode: existing_mode,
        pane_cwd: existing_pane_cwd,
        pane_mode: existing_pane_mode,
        pane_worktree: existing_pane_worktree,
    })
}

pub(super) fn apply_mode_metadata(
    session_name: &str,
    slot_cwd_key: &str,
    slot_mode_key: &str,
    pane_id: &str,
    state: &ModeMetadataState,
) -> Result<(), SessionError> {
    set_session_option(session_name, slot_cwd_key, &state.session_cwd)?;
    set_session_option(session_name, slot_mode_key, &state.session_mode)?;
    set_pane_option(pane_id, "@ezm_slot_cwd", &state.pane_cwd)?;
    set_pane_option(pane_id, "@ezm_slot_mode", &state.pane_mode)?;
    set_pane_option(pane_id, "@ezm_slot_worktree", &state.pane_worktree)
}

pub(super) fn verify_mode_metadata(
    session_name: &str,
    slot_id: u8,
    slot_cwd_key: &str,
    slot_mode_key: &str,
    pane_id: &str,
    expected: &ModeMetadataState,
) -> Result<(), SessionError> {
    let session_cwd = required_session_option(session_name, slot_cwd_key)?;
    let session_mode = required_session_option(session_name, slot_mode_key)?;
    let pane_cwd = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_cwd")?;
    let pane_mode = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_mode")?;
    let pane_worktree = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_worktree")?;

    if session_cwd == expected.session_cwd
        && session_mode == expected.session_mode
        && pane_cwd == expected.pane_cwd
        && pane_mode == expected.pane_mode
        && pane_worktree == expected.pane_worktree
    {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("switch-slot-mode-verify -t {session_name} --slot {slot_id}"),
        stderr: format!(
            "metadata verification failed: expected session_cwd={:?} session_mode={:?} pane_cwd={:?} pane_mode={:?} pane_worktree={:?}; got session_cwd={:?} session_mode={:?} pane_cwd={:?} pane_mode={:?} pane_worktree={:?}",
            expected.session_cwd,
            expected.session_mode,
            expected.pane_cwd,
            expected.pane_mode,
            expected.pane_worktree,
            session_cwd,
            session_mode,
            pane_cwd,
            pane_mode,
            pane_worktree
        ),
    })
}

pub(super) fn compensate_mode_metadata(
    session_name: &str,
    slot_id: u8,
    slot_cwd_key: &str,
    slot_mode_key: &str,
    pane_id: &str,
    previous: &ModeMetadataState,
    original_error: SessionError,
) -> Result<(), SessionError> {
    match apply_mode_metadata(session_name, slot_cwd_key, slot_mode_key, pane_id, previous) {
        Ok(()) => Err(original_error),
        Err(compensation_error) => Err(SessionError::TmuxCommandFailed {
            command: format!("switch-slot-mode-compensate -t {session_name} --slot {slot_id}"),
            stderr: format!(
                "mode switch failed: {original_error}; rollback failed: {compensation_error}"
            ),
        }),
    }
}
