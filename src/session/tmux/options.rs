use super::SessionError;
use super::command::{tmux_output, tmux_run};

pub(super) fn required_session_option(
    session_name: &str,
    key: &str,
) -> Result<String, SessionError> {
    match show_session_option(session_name, key)? {
        Some(value) => Ok(value),
        None => Err(canonical_slot_mismatch_error(
            session_name,
            &format!("missing required session option {key}"),
        )),
    }
}

pub(super) fn required_pane_option(
    session_name: &str,
    slot_id: u8,
    pane_id: &str,
    key: &str,
) -> Result<String, SessionError> {
    match show_pane_option(pane_id, key) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(canonical_slot_mismatch_error(
            session_name,
            &format!("slot {slot_id} pane {pane_id} missing {key}"),
        )),
        Err(error) => Err(canonical_slot_mismatch_error(
            session_name,
            &format!("slot {slot_id} pane {pane_id} failed reading {key}: {error}"),
        )),
    }
}

pub(super) fn canonical_slot_mismatch_error(session_name: &str, reason: &str) -> SessionError {
    SessionError::TmuxCommandFailed {
        command: format!("validate-canonical-slot-registry -t {session_name}"),
        stderr: format!("canonical slot identity mismatch: {reason}"),
    }
}

pub(super) fn set_or_verify_session_option(
    session_name: &str,
    key: &str,
    value: &str,
) -> Result<(), SessionError> {
    if let Some(existing) = show_session_option(session_name, key)? {
        if existing == value {
            return Ok(());
        }

        return Err(SessionError::TmuxCommandFailed {
            command: format!("set-option -t {session_name} {key} {value}"),
            stderr: format!("refusing to remap existing value `{existing}`"),
        });
    }

    tmux_run(&["set-option", "-t", session_name, key, value])
}

pub(super) fn set_or_verify_pane_option(
    pane_id: &str,
    key: &str,
    value: &str,
) -> Result<(), SessionError> {
    if let Some(existing) = show_pane_option(pane_id, key)? {
        if existing == value {
            return Ok(());
        }

        return Err(SessionError::TmuxCommandFailed {
            command: format!("set-option -p -t {pane_id} {key} {value}"),
            stderr: format!("refusing to remap existing value `{existing}`"),
        });
    }

    tmux_run(&["set-option", "-p", "-t", pane_id, key, value])
}

fn show_session_option(session_name: &str, key: &str) -> Result<Option<String>, SessionError> {
    let output = tmux_output(&["show-options", "-v", "-t", session_name, key])?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ));
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.contains("invalid option") || stderr.contains("unknown option") {
        return Ok(None);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("show-options -v -t {session_name} {key}"),
        stderr,
    })
}

fn show_pane_option(pane_id: &str, key: &str) -> Result<Option<String>, SessionError> {
    let output = tmux_output(&["show-options", "-p", "-v", "-t", pane_id, key])?;
    if output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        ));
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.contains("invalid option") || stderr.contains("unknown option") {
        return Ok(None);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("show-options -p -v -t {pane_id} {key}"),
        stderr,
    })
}
