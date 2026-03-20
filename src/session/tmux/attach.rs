use std::io::IsTerminal;

use super::SessionError;
use super::command::tmux_output;

pub(super) fn attach_session(session_name: &str) -> Result<(), SessionError> {
    if !should_attempt_interactive_attach(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    ) {
        return Ok(());
    }

    let command = format!("attach-session -t {session_name}");
    let output =
        tmux_output(&["attach-session", "-t", session_name]).map_err(|error| match error {
            SessionError::TmuxSpawnFailed { source, .. } => SessionError::TmuxSpawnFailed {
                command: command.clone(),
                source,
            },
            other => other,
        })?;

    if output.status.success() {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command,
        stderr: super::command::format_output_diagnostics(&output),
    })
}

fn should_attempt_interactive_attach(stdin_is_terminal: bool, stdout_is_terminal: bool) -> bool {
    stdin_is_terminal && stdout_is_terminal
}

#[cfg(test)]
mod tests {
    use super::should_attempt_interactive_attach;

    #[test]
    fn attach_requires_tty_stdin_and_stdout() {
        assert!(!should_attempt_interactive_attach(false, false));
        assert!(!should_attempt_interactive_attach(false, true));
        assert!(!should_attempt_interactive_attach(true, false));
        assert!(should_attempt_interactive_attach(true, true));
    }
}
