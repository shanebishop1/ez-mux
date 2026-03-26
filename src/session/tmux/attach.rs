use std::io;
use std::io::IsTerminal;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use signal_hook::consts::SIGINT;
use signal_hook::flag;
use signal_hook::low_level::unregister;
use signal_hook::SigId;

use super::SessionError;

pub(super) fn attach_session(session_name: &str) -> Result<(), SessionError> {
    if !should_attempt_interactive_attach(
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    ) {
        return Ok(());
    }

    let command = format!("attach-session -t {session_name}");
    let interrupt = ScopedSigintFlag::register()?;
    let mut child = Command::new("tmux")
        .args(["attach-session", "-t", session_name])
        .spawn()
        .map_err(|source| SessionError::TmuxSpawnFailed {
            command: command.clone(),
            source,
        })?;

    loop {
        if interrupt.triggered() {
            let _ = best_effort_interrupt_child(&mut child);
        }

        let status = child
            .try_wait()
            .map_err(|source| SessionError::TmuxSpawnFailed {
                command: command.clone(),
                source,
            })?;

        if let Some(status) = status {
            if status.success() {
                return Ok(());
            }

            if interrupt.triggered() || interrupted_status_code(status.code()) {
                return Err(SessionError::Interrupted);
            }

            let status_code = status
                .code()
                .map_or_else(|| String::from("signal"), |code| code.to_string());

            return Err(SessionError::TmuxCommandFailed {
                command,
                stderr: format!("status={status_code}; stdout=\"\"; stderr=\"\""),
            });
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn interrupted_status_code(status_code: Option<i32>) -> bool {
    status_code == Some(130)
}

fn best_effort_interrupt_child(child: &mut std::process::Child) -> io::Result<()> {
    if let Err(error) = child.kill() {
        if !matches!(
            error.kind(),
            io::ErrorKind::InvalidInput | io::ErrorKind::NotFound
        ) {
            return Err(error);
        }
    }

    Ok(())
}

struct ScopedSigintFlag {
    signal_id: SigId,
    interrupted: Arc<AtomicBool>,
}

impl ScopedSigintFlag {
    fn register() -> Result<Self, SessionError> {
        let interrupted = Arc::new(AtomicBool::new(false));
        let signal_id = flag::register(SIGINT, Arc::clone(&interrupted))
            .map_err(|source| SessionError::SignalRegistrationFailed { source })?;

        Ok(Self {
            signal_id,
            interrupted,
        })
    }

    fn triggered(&self) -> bool {
        self.interrupted.load(Ordering::Relaxed)
    }
}

impl Drop for ScopedSigintFlag {
    fn drop(&mut self) {
        let _ = unregister(self.signal_id);
    }
}

fn should_attempt_interactive_attach(stdin_is_terminal: bool, stdout_is_terminal: bool) -> bool {
    stdin_is_terminal && stdout_is_terminal
}

#[cfg(test)]
mod tests {
    use super::interrupted_status_code;
    use super::should_attempt_interactive_attach;

    #[test]
    fn attach_requires_tty_stdin_and_stdout() {
        assert!(!should_attempt_interactive_attach(false, false));
        assert!(!should_attempt_interactive_attach(false, true));
        assert!(!should_attempt_interactive_attach(true, false));
        assert!(should_attempt_interactive_attach(true, true));
    }

    #[test]
    fn status_code_130_is_interrupted() {
        assert!(interrupted_status_code(Some(130)));
        assert!(!interrupted_status_code(Some(1)));
        assert!(!interrupted_status_code(None));
    }
}
