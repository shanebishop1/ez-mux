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
        .arg("attach-session")
        .arg("-t")
        .arg(session_name)
        .status()
        .map_err(|source| SessionError::TmuxSpawnFailed {
            command: command.clone(),
            source,
        })?;

    if status.success() {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command,
        stderr: status.to_string(),
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
