use std::process::{Command, Output};

use super::SessionError;

pub(super) fn tmux_output(args: &[&str]) -> Result<Output, SessionError> {
    Command::new("tmux")
        .args(args)
        .output()
        .map_err(|source| SessionError::TmuxSpawnFailed {
            command: args.join(" "),
            source,
        })
}

pub(super) fn tmux_run(args: &[&str]) -> Result<(), SessionError> {
    let output = tmux_output(args)?;
    if output.status.success() {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: args.join(" "),
        stderr: format_output_diagnostics(&output),
    })
}

pub(super) fn tmux_output_value(args: &[&str]) -> Result<String, SessionError> {
    let output = tmux_output(args)?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    Err(SessionError::TmuxCommandFailed {
        command: args.join(" "),
        stderr: format_output_diagnostics(&output),
    })
}

pub(super) fn tmux_primary_window_target(session_name: &str) -> Result<String, SessionError> {
    let command = format!("list-windows -t {session_name} -F #{{window_id}}");
    let output = tmux_output_value(&["list-windows", "-t", session_name, "-F", "#{window_id}"])?;
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| SessionError::TmuxCommandFailed {
            command,
            stderr: String::from("tmux returned no window id for session"),
        })
}

pub(super) fn format_output_diagnostics(output: &Output) -> String {
    let status = output
        .status
        .code()
        .map_or_else(|| String::from("signal"), |code| code.to_string());
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();

    format!("status={status}; stdout={stdout:?}; stderr={stderr:?}")
}
