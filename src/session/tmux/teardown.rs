use std::collections::BTreeSet;
use std::process::Command;

use crate::session::{SessionError, TeardownOutcome};

use super::command::{format_output_diagnostics, tmux_output};

pub(super) fn teardown_session(session_name: &str) -> Result<TeardownOutcome, SessionError> {
    let executor = ProcessTeardownExecutor;
    teardown_session_with_executor(session_name, &executor)
}

fn teardown_session_with_executor(
    session_name: &str,
    executor: &impl TeardownExecutor,
) -> Result<TeardownOutcome, SessionError> {
    let helper_session_prefix = format!("{session_name}__");
    let helper_sessions = list_sessions(executor)?
        .into_iter()
        .filter(|candidate| candidate.starts_with(&helper_session_prefix))
        .collect::<Vec<_>>();

    let helper_processes = helper_sessions.iter().try_fold(
        BTreeSet::new(),
        |mut acc, helper_session| -> Result<BTreeSet<u32>, SessionError> {
            for pid in list_pane_pids(executor, helper_session)? {
                acc.insert(pid);
            }
            Ok(acc)
        },
    )?;

    let helper_processes_removed =
        helper_processes
            .iter()
            .try_fold(0_usize, |count, pid| -> Result<usize, SessionError> {
                if terminate_process(executor, *pid)? {
                    Ok(count + 1)
                } else {
                    Ok(count)
                }
            })?;

    let helper_sessions_removed = helper_sessions.iter().try_fold(
        0_usize,
        |count, helper_session| -> Result<usize, SessionError> {
            if kill_session(executor, helper_session)? {
                Ok(count + 1)
            } else {
                Ok(count)
            }
        },
    )?;

    let project_session_removed = kill_session(executor, session_name)?;

    Ok(TeardownOutcome {
        session_name: session_name.to_owned(),
        helper_sessions_removed,
        helper_processes_removed,
        project_session_removed,
    })
}

#[derive(Debug, Clone)]
struct CommandOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    diagnostics: String,
}

trait TeardownExecutor {
    fn tmux_output(&self, args: &[&str]) -> Result<CommandOutput, SessionError>;
    fn terminate_process(&self, pid: u32) -> Result<CommandOutput, SessionError>;
}

struct ProcessTeardownExecutor;

impl TeardownExecutor for ProcessTeardownExecutor {
    fn tmux_output(&self, args: &[&str]) -> Result<CommandOutput, SessionError> {
        let output = tmux_output(args)?;
        let diagnostics = format_output_diagnostics(&output);
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
            diagnostics,
        })
    }

    fn terminate_process(&self, pid: u32) -> Result<CommandOutput, SessionError> {
        let output = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .output()
            .map_err(|source| SessionError::TmuxSpawnFailed {
                command: format!("kill -TERM {pid}"),
                source,
            })?;
        let diagnostics = format_output_diagnostics(&output);

        Ok(CommandOutput {
            success: output.status.success(),
            stdout: output.stdout,
            stderr: output.stderr,
            diagnostics,
        })
    }
}

fn list_sessions(executor: &impl TeardownExecutor) -> Result<Vec<String>, SessionError> {
    let args = ["list-sessions", "-F", "#{session_name}"];
    let output = executor.tmux_output(&args)?;
    if output.success {
        return Ok(parse_lines(&output.stdout));
    }

    if tmux_absence_status(&output.stderr) {
        return Ok(Vec::new());
    }

    Err(SessionError::TmuxCommandFailed {
        command: args.join(" "),
        stderr: output.diagnostics,
    })
}

fn list_pane_pids(
    executor: &impl TeardownExecutor,
    session_name: &str,
) -> Result<Vec<u32>, SessionError> {
    let args = ["list-panes", "-t", session_name, "-F", "#{pane_pid}"];
    let output = executor.tmux_output(&args)?;
    if output.success {
        return Ok(parse_lines(&output.stdout)
            .into_iter()
            .filter_map(|line| line.parse::<u32>().ok())
            .collect());
    }

    if tmux_absence_status(&output.stderr) {
        return Ok(Vec::new());
    }

    Err(SessionError::TmuxCommandFailed {
        command: args.join(" "),
        stderr: output.diagnostics,
    })
}

fn kill_session(
    executor: &impl TeardownExecutor,
    session_name: &str,
) -> Result<bool, SessionError> {
    let args = ["kill-session", "-t", session_name];
    let output = executor.tmux_output(&args)?;
    if output.success {
        return Ok(true);
    }

    if tmux_absence_status(&output.stderr) {
        return Ok(false);
    }

    Err(SessionError::TmuxCommandFailed {
        command: args.join(" "),
        stderr: output.diagnostics,
    })
}

fn terminate_process(executor: &impl TeardownExecutor, pid: u32) -> Result<bool, SessionError> {
    let output = executor.terminate_process(pid)?;

    if output.success {
        return Ok(true);
    }

    if process_absence_status(&output.stderr) {
        return Ok(false);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("kill -TERM {pid}"),
        stderr: output.diagnostics,
    })
}

fn parse_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn tmux_absence_status(stderr: &[u8]) -> bool {
    let normalized = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    normalized.contains("can't find session")
        || normalized.contains("no server running")
        || normalized.contains("failed to connect to server")
}

fn process_absence_status(stderr: &[u8]) -> bool {
    let normalized = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    normalized.contains("no such process")
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};

    use super::{
        CommandOutput, TeardownExecutor, parse_lines, process_absence_status,
        teardown_session_with_executor, tmux_absence_status,
    };

    #[derive(Default)]
    struct FakeTeardownExecutor {
        tmux_responses: RefCell<HashMap<String, VecDeque<CommandOutput>>>,
        kill_responses: RefCell<HashMap<u32, VecDeque<CommandOutput>>>,
        calls: RefCell<Vec<String>>,
    }

    impl FakeTeardownExecutor {
        fn push_tmux_output(&self, args: &[&str], output: CommandOutput) {
            self.tmux_responses
                .borrow_mut()
                .entry(args.join("\u{1f}"))
                .or_default()
                .push_back(output);
        }

        fn push_kill_output(&self, pid: u32, output: CommandOutput) {
            self.kill_responses
                .borrow_mut()
                .entry(pid)
                .or_default()
                .push_back(output);
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl TeardownExecutor for FakeTeardownExecutor {
        fn tmux_output(
            &self,
            args: &[&str],
        ) -> Result<CommandOutput, crate::session::SessionError> {
            let command = args.join(" ");
            self.calls.borrow_mut().push(format!("tmux {command}"));

            let key = args.join("\u{1f}");
            let mut responses = self.tmux_responses.borrow_mut();
            let queue = responses
                .get_mut(&key)
                .unwrap_or_else(|| panic!("missing fake tmux output for `{command}`"));

            Ok(queue
                .pop_front()
                .unwrap_or_else(|| panic!("no fake tmux outputs left for `{command}`")))
        }

        fn terminate_process(
            &self,
            pid: u32,
        ) -> Result<CommandOutput, crate::session::SessionError> {
            self.calls.borrow_mut().push(format!("kill -TERM {pid}"));

            let mut responses = self.kill_responses.borrow_mut();
            let queue = responses
                .get_mut(&pid)
                .unwrap_or_else(|| panic!("missing fake kill output for pid `{pid}`"));

            Ok(queue
                .pop_front()
                .unwrap_or_else(|| panic!("no fake kill outputs left for pid `{pid}`")))
        }
    }

    fn success_output(stdout: &str) -> CommandOutput {
        CommandOutput {
            success: true,
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            diagnostics: String::from("status=0"),
        }
    }

    fn failed_output(stderr: &str) -> CommandOutput {
        CommandOutput {
            success: false,
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
            diagnostics: format!("status=1; stderr={stderr:?}"),
        }
    }

    #[test]
    fn parse_lines_discards_empty_entries() {
        let lines = parse_lines(b"one\n\n two \n\n");
        assert_eq!(lines, vec![String::from("one"), String::from("two")]);
    }

    #[test]
    fn tmux_absence_status_matches_expected_errors() {
        assert!(tmux_absence_status(b"can't find session: foo"));
        assert!(tmux_absence_status(
            b"no server running on /tmp/tmux-1000/default"
        ));
        assert!(tmux_absence_status(b"failed to connect to server"));
        assert!(!tmux_absence_status(b"permission denied"));
    }

    #[test]
    fn process_absence_status_matches_no_such_process_only() {
        assert!(process_absence_status(b"kill: (12345): No such process"));
        assert!(!process_absence_status(b"operation not permitted"));
    }

    #[test]
    fn teardown_filters_namespace_collects_unique_pids_and_orders_steps() {
        let executor = FakeTeardownExecutor::default();
        executor.push_tmux_output(
            &["list-sessions", "-F", "#{session_name}"],
            success_output(
                "ezm-s123\nezm-s123__popup__1\nother\nezm-s123__aux\nezm-s12\nezm-s1234__other\n",
            ),
        );
        executor.push_tmux_output(
            &[
                "list-panes",
                "-t",
                "ezm-s123__popup__1",
                "-F",
                "#{pane_pid}",
            ],
            success_output("101\n102\ninvalid\n"),
        );
        executor.push_tmux_output(
            &["list-panes", "-t", "ezm-s123__aux", "-F", "#{pane_pid}"],
            success_output("102\n103\n"),
        );
        executor.push_kill_output(101, success_output(""));
        executor.push_kill_output(102, success_output(""));
        executor.push_kill_output(103, success_output(""));
        executor.push_tmux_output(
            &["kill-session", "-t", "ezm-s123__popup__1"],
            success_output(""),
        );
        executor.push_tmux_output(&["kill-session", "-t", "ezm-s123__aux"], success_output(""));
        executor.push_tmux_output(&["kill-session", "-t", "ezm-s123"], success_output(""));

        let outcome =
            teardown_session_with_executor("ezm-s123", &executor).expect("teardown should succeed");

        assert!(outcome.project_session_removed);
        assert_eq!(outcome.helper_sessions_removed, 2);
        assert_eq!(outcome.helper_processes_removed, 3);
        assert_eq!(
            executor.calls(),
            vec![
                String::from("tmux list-sessions -F #{session_name}"),
                String::from("tmux list-panes -t ezm-s123__popup__1 -F #{pane_pid}"),
                String::from("tmux list-panes -t ezm-s123__aux -F #{pane_pid}"),
                String::from("kill -TERM 101"),
                String::from("kill -TERM 102"),
                String::from("kill -TERM 103"),
                String::from("tmux kill-session -t ezm-s123__popup__1"),
                String::from("tmux kill-session -t ezm-s123__aux"),
                String::from("tmux kill-session -t ezm-s123"),
            ]
        );
    }

    #[test]
    fn teardown_treats_absent_helpers_and_processes_as_idempotent() {
        let executor = FakeTeardownExecutor::default();
        executor.push_tmux_output(
            &["list-sessions", "-F", "#{session_name}"],
            success_output("ezm-s123\nezm-s123__popup\n"),
        );
        executor.push_tmux_output(
            &["list-panes", "-t", "ezm-s123__popup", "-F", "#{pane_pid}"],
            success_output("200\n"),
        );
        executor.push_kill_output(200, failed_output("kill: (200): No such process"));
        executor.push_tmux_output(
            &["kill-session", "-t", "ezm-s123__popup"],
            failed_output("can't find session: ezm-s123__popup"),
        );
        executor.push_tmux_output(
            &["kill-session", "-t", "ezm-s123"],
            failed_output("can't find session: ezm-s123"),
        );

        let outcome =
            teardown_session_with_executor("ezm-s123", &executor).expect("teardown should succeed");

        assert!(!outcome.project_session_removed);
        assert_eq!(outcome.helper_sessions_removed, 0);
        assert_eq!(outcome.helper_processes_removed, 0);
    }

    #[test]
    fn teardown_is_idempotent_across_repeated_runs_when_targets_disappear() {
        let executor = FakeTeardownExecutor::default();

        executor.push_tmux_output(
            &["list-sessions", "-F", "#{session_name}"],
            success_output("ezm-s123\nezm-s123__popup\n"),
        );
        executor.push_tmux_output(
            &["list-panes", "-t", "ezm-s123__popup", "-F", "#{pane_pid}"],
            success_output("400\n"),
        );
        executor.push_kill_output(400, success_output(""));
        executor.push_tmux_output(
            &["kill-session", "-t", "ezm-s123__popup"],
            success_output(""),
        );
        executor.push_tmux_output(&["kill-session", "-t", "ezm-s123"], success_output(""));

        executor.push_tmux_output(
            &["list-sessions", "-F", "#{session_name}"],
            success_output("ezm-s123\n"),
        );
        executor.push_tmux_output(
            &["kill-session", "-t", "ezm-s123"],
            failed_output("can't find session: ezm-s123"),
        );

        let first = teardown_session_with_executor("ezm-s123", &executor).expect("first run");
        let second = teardown_session_with_executor("ezm-s123", &executor).expect("second run");

        assert!(first.project_session_removed);
        assert_eq!(first.helper_sessions_removed, 1);
        assert_eq!(first.helper_processes_removed, 1);

        assert!(!second.project_session_removed);
        assert_eq!(second.helper_sessions_removed, 0);
        assert_eq!(second.helper_processes_removed, 0);
    }
}
