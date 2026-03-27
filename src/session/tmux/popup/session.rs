use super::super::SessionError;
use super::super::command::{format_output_diagnostics, tmux_output, tmux_output_value, tmux_run};
use super::super::options::set_session_option;
use super::context::PopupRemoteContext;
use super::remote_ssh::popup_remote_launch_command;

pub(super) fn popup_session_name(session_name: &str, slot_id: u8) -> String {
    format!("{session_name}__popup_slot_{slot_id}")
}

pub(super) fn session_exists(session_name: &str) -> Result<bool, SessionError> {
    let output = tmux_output(&["-q", "has-session", "-t", session_name])?;
    if output.status.success() {
        return Ok(true);
    }

    if output.status.code() == Some(1) {
        return Ok(false);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("has-session -t {session_name}"),
        stderr: format_output_diagnostics(&output),
    })
}

pub(super) fn persist_popup_defaults(session_name: &str) -> Result<(), SessionError> {
    set_session_option(
        session_name,
        "@ezm_popup_width_pct",
        &super::POPUP_WIDTH_PCT.to_string(),
    )?;
    set_session_option(
        session_name,
        "@ezm_popup_height_pct",
        &super::POPUP_HEIGHT_PCT.to_string(),
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

pub(super) fn disable_popup_session_auto_destroy(popup_session: &str) -> Result<(), SessionError> {
    let args = popup_persistence_args(popup_session);
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    tmux_run(&args_ref)
}

pub(super) fn popup_persistence_args(popup_session: &str) -> Vec<String> {
    vec![
        String::from("set-option"),
        String::from("-t"),
        popup_session.to_owned(),
        String::from("destroy-unattached"),
        String::from("off"),
    ]
}

pub(super) fn popup_new_session_args(
    popup_session: &str,
    cwd: &str,
    remote_context: Option<&PopupRemoteContext>,
) -> Result<Vec<String>, SessionError> {
    let mut args = vec![
        String::from("new-session"),
        String::from("-d"),
        String::from("-s"),
        popup_session.to_owned(),
        String::from("-c"),
        cwd.to_owned(),
    ];

    if let Some(command) = popup_remote_launch_command(remote_context)? {
        args.push(command);
    }

    Ok(args)
}
