use super::SessionError;
use super::command::{tmux_output, tmux_output_value, tmux_run};
use super::options::{required_session_option, set_session_option};
use super::slot_swap::validate_canonical_slot_registry;
use super::style::refresh_active_border_for_slot;
use crate::session::{PopupShellAction, PopupShellOutcome};

const POPUP_WIDTH_PCT: u8 = 70;
const POPUP_HEIGHT_PCT: u8 = 70;

pub(super) fn toggle_popup_shell(
    session_name: &str,
    slot_id: u8,
) -> Result<PopupShellOutcome, SessionError> {
    validate_canonical_slot_registry(session_name)?;

    let origin_slot_pane =
        required_session_option(session_name, &format!("@ezm_slot_{slot_id}_pane"))?;
    let cwd = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_cwd"))?;
    let popup_session = popup_session_name(session_name, slot_id);

    if session_exists(&popup_session)? {
        tmux_run(&["kill-session", "-t", &popup_session])?;
        let _ = refresh_active_border_for_slot(session_name, slot_id);
        let _ = tmux_run(&["select-pane", "-t", &origin_slot_pane]);
        persist_popup_defaults(session_name)?;
        return Ok(PopupShellOutcome {
            session_name: session_name.to_owned(),
            slot_id,
            action: PopupShellAction::Closed,
            cwd,
            width_pct: POPUP_WIDTH_PCT,
            height_pct: POPUP_HEIGHT_PCT,
        });
    }

    tmux_run(&[
        "new-session",
        "-d",
        "-s",
        &popup_session,
        "-c",
        &cwd,
        "sh",
        "-lc",
        "exec \"${SHELL:-/bin/sh}\" -l",
    ])?;

    persist_popup_defaults(session_name)?;
    set_session_option(&popup_session, "@ezm_popup_origin_session", session_name)?;
    set_session_option(
        &popup_session,
        "@ezm_popup_origin_slot",
        &slot_id.to_string(),
    )?;
    set_session_option(&popup_session, "@ezm_popup_origin_pane", &origin_slot_pane)?;
    set_session_option(&popup_session, "@ezm_popup_cwd", &cwd)?;

    validate_canonical_slot_registry(session_name)?;
    Ok(PopupShellOutcome {
        session_name: session_name.to_owned(),
        slot_id,
        action: PopupShellAction::Opened,
        cwd,
        width_pct: POPUP_WIDTH_PCT,
        height_pct: POPUP_HEIGHT_PCT,
    })
}

fn popup_session_name(session_name: &str, slot_id: u8) -> String {
    format!("{session_name}__popup_slot_{slot_id}")
}

fn session_exists(session_name: &str) -> Result<bool, SessionError> {
    let output = tmux_output(&["-q", "has-session", "-t", session_name])?;
    if output.status.success() {
        return Ok(true);
    }

    if output.status.code() == Some(1) {
        return Ok(false);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("has-session -t {session_name}"),
        stderr: super::command::format_output_diagnostics(&output),
    })
}

fn persist_popup_defaults(session_name: &str) -> Result<(), SessionError> {
    set_session_option(
        session_name,
        "@ezm_popup_width_pct",
        &POPUP_WIDTH_PCT.to_string(),
    )?;
    set_session_option(
        session_name,
        "@ezm_popup_height_pct",
        &POPUP_HEIGHT_PCT.to_string(),
    )?;

    let _ = tmux_output_value(&[
        "set-option",
        "-t",
        session_name,
        "@ezm_popup_geometry",
        "70x70",
    ])?;

    Ok(())
}
