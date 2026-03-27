use std::io;
use std::io::IsTerminal;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use signal_hook::SigId;
use signal_hook::consts::SIGINT;
use signal_hook::flag;
use signal_hook::low_level::unregister;

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

            let diagnostics = format_attach_failure_diagnostics(
                status.code(),
                capture_attach_failure_streams(session_name),
            );

            return Err(SessionError::TmuxCommandFailed {
                command,
                stderr: diagnostics,
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

struct AttachFailureStreams {
    stdout: String,
    stderr: String,
}

fn capture_attach_failure_streams(
    session_name: &str,
) -> Result<AttachFailureStreams, SessionError> {
    let output = tmux_output(&["attach-session", "-t", session_name])?;
    Ok(AttachFailureStreams {
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

fn format_attach_failure_diagnostics(
    status_code: Option<i32>,
    captured_streams: Result<AttachFailureStreams, SessionError>,
) -> String {
    let status = status_code.map_or_else(|| String::from("signal"), |code| code.to_string());
    let (stdout, stderr) = match captured_streams {
        Ok(streams) => (streams.stdout, streams.stderr),
        Err(error) => (
            String::new(),
            format!("failed collecting attach-session diagnostics: {error}"),
        ),
    };

    format!("status={status}; stdout={stdout:?}; stderr={stderr:?}")
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
    use std::io;

    use super::AttachFailureStreams;
    use super::SessionError;
    use super::format_attach_failure_diagnostics;
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

    #[test]
    fn attach_failure_diagnostics_include_captured_stdout_stderr_and_status() {
        let diagnostics = format_attach_failure_diagnostics(
            Some(1),
            Ok(AttachFailureStreams {
                stdout: String::from("captured stdout"),
                stderr: String::from("captured stderr"),
            }),
        );

        assert_eq!(
            diagnostics,
            "status=1; stdout=\"captured stdout\"; stderr=\"captured stderr\""
        );
    }

    #[test]
    fn attach_failure_diagnostics_report_capture_errors_with_original_status() {
        let capture_error = SessionError::TmuxSpawnFailed {
            command: String::from("attach-session -t ezm-s42"),
            source: io::Error::new(io::ErrorKind::NotFound, "tmux missing"),
        };
        let diagnostics = format_attach_failure_diagnostics(Some(127), Err(capture_error));

        assert!(diagnostics.contains("status=127"));
        assert!(diagnostics.contains("failed collecting attach-session diagnostics"));
        assert!(diagnostics.contains("tmux missing"));
    }
}
