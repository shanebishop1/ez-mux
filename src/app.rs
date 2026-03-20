use thiserror::Error;

use crate::cli::{AuxiliaryAction, Cli, Command, InternalCommand, LogsCommand};
use crate::config::OPERATOR_ENV;
use crate::config::{self, ConfigError, OperatingSystem, ValueSource};
use crate::logging::{self, LogOpener, LoggingError};
use crate::session::{self, SessionError};

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Logging(#[from] LoggingError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("{0}")]
    Runtime(String),
    #[error("interrupted")]
    Interrupted,
}

/// Executes the parsed CLI command and returns a success message.
///
/// # Errors
///
/// Returns [`AppError::Config`] when config path resolution or TOML parsing fails,
/// and [`AppError::Logging`] when log-open operations fail.
pub fn execute(
    cli: Cli,
    env: &impl config::EnvProvider,
    os: OperatingSystem,
    active_log_root: &std::path::Path,
) -> Result<String, AppError> {
    execute_with_opener(cli, env, os, active_log_root, &logging::ProcessLogOpener)
}

pub(crate) fn execute_with_opener(
    cli: Cli,
    env: &impl config::EnvProvider,
    os: OperatingSystem,
    active_log_root: &std::path::Path,
    opener: &impl LogOpener,
) -> Result<String, AppError> {
    let loaded = config::load_config(env, os)?;
    let resolved_operator = config::resolve_operator(
        cli.operator,
        env.get_var(OPERATOR_ENV),
        loaded.values.operator,
    );

    let message = match cli.command {
        None => {
            let outcome = session::ensure_current_project_session(&session::ProcessTmuxClient)?;
            format!(
                "ezm v1 contract locked; operator source={}. session={}; session_action={}.",
                source_label(resolved_operator.source),
                outcome.identity.session_name,
                outcome.action.label()
            )
        }
        Some(Command::Repair) => {
            String::from("repair contract entrypoint accepted (implementation pending).")
        }
        Some(Command::Logs(LogsCommand::OpenLatest)) => {
            let opened_log_path = logging::open_latest_log(active_log_root, os, opener)?;
            format!("opened latest log: {}", opened_log_path.display())
        }
        Some(Command::Internal {
            command: InternalCommand::Swap { session, slot },
        }) => {
            let tmux = session::ProcessTmuxClient;
            session::TmuxClient::swap_slot_with_center(&tmux, &session, slot)?;
            format!("internal swap complete: session={session}; slot={slot}")
        }
        Some(Command::Internal {
            command:
                InternalCommand::Mode {
                    session,
                    slot,
                    mode,
                },
        }) => {
            let tmux = session::ProcessTmuxClient;
            let outcome = session::switch_slot_mode(&session, slot, mode, &tmux)?;
            format!(
                "internal mode complete: session={}; slot={}; mode={}",
                outcome.session_name,
                outcome.slot_id,
                outcome.mode.label()
            )
        }
        Some(Command::Internal {
            command: InternalCommand::Popup { session, slot },
        }) => {
            let tmux = session::ProcessTmuxClient;
            let outcome = session::toggle_popup_shell(&session, slot, &tmux)?;
            format!(
                "internal popup complete: session={}; slot={}; action={}; cwd={}; width_pct={}; height_pct={}",
                outcome.session_name,
                outcome.slot_id,
                outcome.action.label(),
                outcome.cwd,
                outcome.width_pct,
                outcome.height_pct
            )
        }
        Some(Command::Internal {
            command: InternalCommand::Auxiliary { session, action },
        }) => {
            let tmux = session::ProcessTmuxClient;
            let open = matches!(action, AuxiliaryAction::Open);
            let outcome = session::auxiliary_viewer(&session, open, &tmux)?;
            format!(
                "internal auxiliary complete: session={}; action={}; window_name={}; window_id={}",
                outcome.session_name,
                outcome.action.label(),
                outcome.window_name,
                outcome.window_id.unwrap_or_else(|| String::from("none"))
            )
        }
    };

    Ok(message)
}

fn source_label(source: ValueSource) -> &'static str {
    source.label()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::io;
    use std::path::Path;

    use tempfile::tempdir;

    use super::AppError;
    use super::execute_with_opener;
    use crate::cli::{Cli, Command, LogsCommand};
    use crate::config::OperatingSystem;
    use crate::logging::LogOpener;

    struct OkOpener;

    impl LogOpener for OkOpener {
        fn open(&self, _: OperatingSystem, _: &Path) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailingOpener;

    impl LogOpener for FailingOpener {
        fn open(&self, _: OperatingSystem, _: &Path) -> io::Result<()> {
            Err(io::Error::other("open failed"))
        }
    }

    #[test]
    fn open_latest_succeeds_and_reports_opened_path() {
        let root = tempdir().expect("root");
        fs::write(root.path().join("20260319-101500-run-1.log"), "old").expect("write old");
        fs::write(root.path().join("20260319-101700-run-2.log"), "new").expect("write new");
        let mut env = HashMap::new();
        env.insert(String::from("HOME"), String::from("/tmp/home"));

        let message = execute_with_opener(
            Cli {
                operator: None,
                command: Some(Command::Logs(LogsCommand::OpenLatest)),
            },
            &env,
            OperatingSystem::Linux,
            root.path(),
            &OkOpener,
        )
        .expect("open-latest should succeed");

        assert!(message.contains("opened latest log:"));
        assert!(message.contains("20260319-101700-run-2.log"));
    }

    #[test]
    fn open_latest_errors_when_no_logs_exist() {
        let root = tempdir().expect("root");
        let mut env = HashMap::new();
        env.insert(String::from("HOME"), String::from("/tmp/home"));

        let error = execute_with_opener(
            Cli {
                operator: None,
                command: Some(Command::Logs(LogsCommand::OpenLatest)),
            },
            &env,
            OperatingSystem::Linux,
            root.path(),
            &OkOpener,
        )
        .expect_err("open-latest should fail");

        let rendered = error.to_string();
        assert!(rendered.contains("no log files found"));
    }

    #[test]
    fn open_latest_missing_logs_is_typed_logging_error() {
        let root = tempdir().expect("root");
        let mut env = HashMap::new();
        env.insert(String::from("HOME"), String::from("/tmp/home"));

        let error = execute_with_opener(
            Cli {
                operator: None,
                command: Some(Command::Logs(LogsCommand::OpenLatest)),
            },
            &env,
            OperatingSystem::Linux,
            root.path(),
            &OkOpener,
        )
        .expect_err("open-latest should fail");

        assert!(matches!(
            error,
            AppError::Logging(crate::logging::LoggingError::NoLogFiles { .. })
        ));
    }

    #[test]
    fn open_latest_errors_when_open_command_fails() {
        let root = tempdir().expect("root");
        fs::write(root.path().join("20260319-101700-run-2.log"), "new").expect("write log");
        let mut env = HashMap::new();
        env.insert(String::from("HOME"), String::from("/tmp/home"));

        let error = execute_with_opener(
            Cli {
                operator: None,
                command: Some(Command::Logs(LogsCommand::OpenLatest)),
            },
            &env,
            OperatingSystem::Linux,
            root.path(),
            &FailingOpener,
        )
        .expect_err("open-latest should fail");

        let rendered = error.to_string();
        assert!(rendered.contains("failed opening log file"));
    }
}
