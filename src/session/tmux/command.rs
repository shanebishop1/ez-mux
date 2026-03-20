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
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

pub(super) fn tmux_output_value(args: &[&str]) -> Result<String, SessionError> {
    let output = tmux_output(args)?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    Err(SessionError::TmuxCommandFailed {
        command: args.join(" "),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}
