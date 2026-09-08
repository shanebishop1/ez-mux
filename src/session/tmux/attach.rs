use std::io;
use std::io::{IsTerminal, Read, Write};
use std::path::Path;
use std::process::{ChildStderr, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use signal_hook::SigId;
use signal_hook::consts::SIGINT;
use signal_hook::flag;
use signal_hook::low_level::unregister;

use super::SessionError;

pub(super) fn attach_session(session_name: &str) -> Result<(), SessionError> {
    attach_session_with_program(
        session_name,
        Path::new("tmux"),
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    )
}

fn attach_session_with_program(
    session_name: &str,
    program: &Path,
    stdin_is_terminal: bool,
    stdout_is_terminal: bool,
) -> Result<(), SessionError> {
    if !should_attempt_interactive_attach(stdin_is_terminal, stdout_is_terminal) {
        return Ok(());
    }

    // `-E` prevents tmux's update-environment policy from replacing this
    // session's project-scoped authentication with the attaching process's
    // environment (which may belong to a different project).
    let command = format!("attach-session -E -t {session_name}");
    let interrupt = ScopedSigintFlag::register()?;
    let mut child = Command::new(program)
        .args(["attach-session", "-E", "-t", session_name])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| SessionError::TmuxSpawnFailed {
            command: command.clone(),
            source,
        })?;
    let stderr_reader = child
        .stderr
        .take()
        .map(|stderr| thread::spawn(move || capture_child_stderr(stderr)));

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
            let captured_stderr = join_stderr_reader(stderr_reader);

            if status.success() {
                return Ok(());
            }

            if interrupt.triggered() || interrupted_status_code(status.code()) {
                return Err(SessionError::Interrupted);
            }

            let diagnostics = format_attach_failure_diagnostics(status.code(), captured_stderr);

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
    stderr: String,
}

fn capture_child_stderr(mut stderr: ChildStderr) -> io::Result<AttachFailureStreams> {
    let mut captured = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut visible_stderr = io::stderr().lock();

    loop {
        let bytes_read = stderr.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        captured.extend_from_slice(&buffer[..bytes_read]);
        let _ = visible_stderr.write_all(&buffer[..bytes_read]);
        let _ = visible_stderr.flush();
    }

    Ok(AttachFailureStreams {
        stderr: String::from_utf8_lossy(&captured).trim().to_owned(),
    })
}

fn join_stderr_reader(
    stderr_reader: Option<thread::JoinHandle<io::Result<AttachFailureStreams>>>,
) -> Result<AttachFailureStreams, io::Error> {
    match stderr_reader {
        Some(reader) => reader
            .join()
            .map_err(|_| io::Error::other("attach stderr reader thread panicked"))?,
        None => Ok(AttachFailureStreams {
            stderr: String::new(),
        }),
    }
}

fn format_attach_failure_diagnostics(
    status_code: Option<i32>,
    captured_stderr: Result<AttachFailureStreams, io::Error>,
) -> String {
    let status = status_code.map_or_else(|| String::from("signal"), |code| code.to_string());
    let stderr = match captured_stderr {
        Ok(streams) => streams.stderr,
        Err(error) => format!("failed collecting attach-session diagnostics: {error}"),
    };

    format!("status={status}; stderr={stderr:?}")
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

    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::path::Path;

    #[cfg(unix)]
    use tempfile::TempDir;

    use super::AttachFailureStreams;
    use super::SessionError;
    use super::attach_session_with_program;
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
    fn attach_failure_diagnostics_include_captured_stderr_and_status() {
        let diagnostics = format_attach_failure_diagnostics(
            Some(1),
            Ok(AttachFailureStreams {
                stderr: String::from("captured stderr"),
            }),
        );

        assert_eq!(diagnostics, "status=1; stderr=\"captured stderr\"");
    }

    #[test]
    fn attach_failure_diagnostics_report_capture_errors_with_original_status() {
        let capture_error = io::Error::new(io::ErrorKind::NotFound, "tmux missing");
        let diagnostics = format_attach_failure_diagnostics(Some(127), Err(capture_error));

        assert!(diagnostics.contains("status=127"));
        assert!(diagnostics.contains("failed collecting attach-session diagnostics"));
        assert!(diagnostics.contains("tmux missing"));
    }

    #[test]
    fn attach_failure_diagnostics_use_signal_status_for_non_exit_failures() {
        let diagnostics = format_attach_failure_diagnostics(
            None,
            Ok(AttachFailureStreams {
                stderr: String::from("session died"),
            }),
        );

        assert_eq!(diagnostics, "status=signal; stderr=\"session died\"");
    }

    #[cfg(unix)]
    struct FakeTmux {
        _directory: TempDir,
        program: std::path::PathBuf,
        count_file: std::path::PathBuf,
        args_file: std::path::PathBuf,
    }

    #[cfg(unix)]
    impl FakeTmux {
        fn new(exit_code: i32, stderr: &str) -> Self {
            let directory = TempDir::new().expect("create fake tmux directory");
            let program = directory.path().join("fake-tmux");
            let count_file = program.with_extension("count");
            let args_file = program.with_extension("args");
            let script = format!(
                "#!/bin/sh\ncount_file=\"$0.count\"\ncount=0\nif [ -f \"$count_file\" ]; then count=$(cat \"$count_file\"); fi\ncount=$((count + 1))\nprintf '%s\\n' \"$count\" > \"$count_file\"\nprintf '%s\\n' \"$*\" > \"$0.args\"\nprintf '%s\\n' '{stderr}' >&2\nexit {exit_code}\n"
            );
            fs::write(&program, script).expect("write fake tmux");
            let mut permissions = fs::metadata(&program)
                .expect("stat fake tmux")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&program, permissions).expect("make fake tmux executable");

            Self {
                _directory: directory,
                program,
                count_file,
                args_file,
            }
        }

        fn invocation_count(&self) -> u32 {
            fs::read_to_string(&self.count_file)
                .expect("read fake tmux invocation count")
                .trim()
                .parse()
                .expect("parse fake tmux invocation count")
        }

        fn invocation_args(&self) -> String {
            fs::read_to_string(&self.args_file)
                .expect("read fake tmux invocation args")
                .trim()
                .to_owned()
        }
    }

    #[cfg(unix)]
    #[test]
    fn failed_attach_runs_fake_tmux_once_and_reports_original_stderr() {
        let fake_tmux = FakeTmux::new(42, "original attach diagnostic");

        let error =
            attach_session_with_program("ezm-s42", Path::new(&fake_tmux.program), true, true)
                .expect_err("attach should fail");

        match error {
            SessionError::TmuxCommandFailed { stderr, .. } => {
                assert!(stderr.contains("status=42"));
                assert!(stderr.contains("original attach diagnostic"));
            }
            other => panic!("expected tmux command failure, got {other:?}"),
        }
        assert_eq!(fake_tmux.invocation_count(), 1);
        assert_eq!(fake_tmux.invocation_args(), "attach-session -E -t ezm-s42");
    }

    #[cfg(unix)]
    #[test]
    fn successful_attach_runs_fake_tmux_once() {
        let fake_tmux = FakeTmux::new(0, "");

        attach_session_with_program("ezm-s42", Path::new(&fake_tmux.program), true, true)
            .expect("attach should succeed");

        assert_eq!(fake_tmux.invocation_count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_attach_preserves_interrupted_result_and_runs_once() {
        let fake_tmux = FakeTmux::new(130, "interrupted attach diagnostic");

        let error =
            attach_session_with_program("ezm-s42", Path::new(&fake_tmux.program), true, true)
                .expect_err("attach should be interrupted");

        assert!(matches!(error, SessionError::Interrupted));
        assert_eq!(fake_tmux.invocation_count(), 1);
    }
}
