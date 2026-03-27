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
mod tests;
