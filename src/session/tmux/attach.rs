use std::io::IsTerminal;
use std::process::Command;

use super::SessionError;

pub(super) fn attach_session(session_name: &str) -> Result<(), SessionError> {
    if !should_attempt_interactive_attach(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    ) {
        return Ok(());
    }

    let command = format!("attach-session -t {session_name}");
    let status = Command::new("tmux")
        .args(["attach-session", "-t", session_name])
        .status()
        .map_err(|source| SessionError::TmuxSpawnFailed {
            command: command.clone(),
            source,
        })?;

    if status.success() {
        return Ok(());
    }

    let status_code = status
        .code()
        .map_or_else(|| String::from("signal"), |code| code.to_string());

    Err(SessionError::TmuxCommandFailed {
        command,
        stderr: format!("status={status_code}; stdout=\"\"; stderr=\"\""),
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
