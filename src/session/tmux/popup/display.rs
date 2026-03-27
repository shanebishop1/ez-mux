use super::super::SessionError;
use super::super::command::{format_output_diagnostics, tmux_output, tmux_run};
use super::remote_ssh::shell_single_quote;

pub(super) fn show_popup(
    origin_slot_pane: &str,
    popup_session: &str,
    cwd: &str,
    client_tty: Option<&str>,
) -> Result<(), SessionError> {
    let attach_command = popup_attach_command(popup_session);
    let args = popup_display_args(origin_slot_pane, cwd, client_tty, &attach_command);

    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    let result = tmux_run(&args_ref);

    if let Err(SessionError::TmuxCommandFailed { stderr, .. }) = &result {
        if stderr.to_ascii_lowercase().contains("no current client") {
            return Ok(());
        }
    }

    result
}

pub(super) fn close_popup(client_tty: Option<&str>) -> Result<(), SessionError> {
    let args = popup_close_args(client_tty);
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    let result = tmux_run(&args_ref);

    if let Err(SessionError::TmuxCommandFailed { stderr, .. }) = &result {
        if stderr.to_ascii_lowercase().contains("no current client") {
            return Ok(());
        }
    }

    result
}

pub(super) fn popup_visible_for_client(client_tty: Option<&str>) -> Result<bool, SessionError> {
    let args = popup_active_probe_args(client_tty);
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = tmux_output(&args_ref)?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(stdout.trim() == "1");
    }

    let stderr = format_output_diagnostics(&output);
    if stderr.to_ascii_lowercase().contains("no current client") {
        return Ok(false);
    }

    Err(SessionError::TmuxCommandFailed {
        command: args.join(" "),
        stderr,
    })
}

pub(super) fn popup_close_args(client_tty: Option<&str>) -> Vec<String> {
    let mut args = vec![String::from("display-popup")];
    if let Some(client_tty) = client_tty.filter(|tty| !tty.trim().is_empty()) {
        args.push(String::from("-c"));
        args.push(client_tty.to_owned());
    }
    args.push(String::from("-C"));
    args
}

pub(super) fn popup_active_probe_args(client_tty: Option<&str>) -> Vec<String> {
    let mut args = vec![String::from("display-message"), String::from("-p")];
    if let Some(client_tty) = client_tty.filter(|tty| !tty.trim().is_empty()) {
        args.push(String::from("-c"));
        args.push(client_tty.to_owned());
    }
    args.push(String::from("#{popup_active}"));
    args
}

pub(super) fn popup_display_args(
    origin_slot_pane: &str,
    cwd: &str,
    client_tty: Option<&str>,
    attach_command: &str,
) -> Vec<String> {
    let mut args = vec![
        String::from("display-popup"),
        String::from("-t"),
        origin_slot_pane.to_owned(),
        String::from("-w"),
        String::from("70%"),
        String::from("-h"),
        String::from("70%"),
        String::from("-d"),
        cwd.to_owned(),
    ];
    if let Some(client_tty) = client_tty.filter(|tty| !tty.trim().is_empty()) {
        args.push(String::from("-c"));
        args.push(client_tty.to_owned());
    }
    args.push(String::from("-E"));
    args.push(attach_command.to_owned());

    args
}

pub(super) fn popup_attach_command(popup_session: &str) -> String {
    format!(
        "tmux attach-session -t {}",
        shell_single_quote(popup_session)
    )
}
