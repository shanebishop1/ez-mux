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
            let outcome = execute_default_session_flow(&session::ProcessTmuxClient)?;
            format!(
                "ezm v1 contract locked; operator source={}. session={}; session_action={}; remote_project_dir={}",
                source_label(resolved_operator.source),
                outcome.identity.session_name,
                outcome.action.label(),
                outcome.remote_project_dir.display()
            )
        }
        Some(Command::Repair) => {
            let outcome = session::repair_current_project_session(&session::ProcessTmuxClient)?;
            format_repair_message(&outcome)
        }
        Some(Command::Logs(LogsCommand::OpenLatest)) => {
            execute_open_latest(active_log_root, os, opener)?
        }
        Some(Command::Preset { preset }) => {
            let tmux = session::ProcessTmuxClient;
            let outcome = execute_default_session_flow(&tmux)?;
            let preset_outcome =
                session::apply_layout_preset(&outcome.identity.session_name, preset, &tmux)?;
            format!(
                "preset apply complete: session={}; preset={}",
                preset_outcome.session_name,
                preset_outcome.preset.label()
            )
        }
        Some(Command::Internal { command }) => {
            execute_internal(command, env, resolved_operator.value.as_deref())?
        }
    };

    Ok(message)
}

fn execute_default_session_flow(
    tmux: &impl session::TmuxClient,
) -> Result<session::SessionLaunchOutcome, AppError> {
    let project_dir = std::env::current_dir().map_err(session::SessionError::CurrentDir)?;
    execute_default_session_flow_for_project_dir(&project_dir, tmux)
}

fn execute_default_session_flow_for_project_dir(
    project_dir: &std::path::Path,
    tmux: &impl session::TmuxClient,
) -> Result<session::SessionLaunchOutcome, AppError> {
    let identity = session::resolve_session_identity(project_dir)?;

    match session::ensure_project_session(project_dir, tmux) {
        Ok(outcome) => Ok(outcome),
        Err(session::SessionError::Interrupted) => {
            let _ = session::teardown_session(&identity.session_name, tmux);
            Err(AppError::Interrupted)
        }
        Err(error) => Err(AppError::Session(error)),
    }
}

fn execute_open_latest(
    active_log_root: &std::path::Path,
    os: OperatingSystem,
    opener: &impl LogOpener,
) -> Result<String, AppError> {
    let opened_log_path = logging::open_latest_log(active_log_root, os, opener)?;
    Ok(format!("opened latest log: {}", opened_log_path.display()))
}

fn execute_internal(
    command: InternalCommand,
    env: &impl config::EnvProvider,
    operator: Option<&str>,
) -> Result<String, AppError> {
    match command {
        InternalCommand::Swap { session, slot } => {
            let tmux = session::ProcessTmuxClient;
            session::TmuxClient::swap_slot_with_center(&tmux, &session, slot)?;
            Ok(format!(
                "internal swap complete: session={session}; slot={slot}"
            ))
        }
        InternalCommand::Mode {
            session,
            slot,
            mode,
        } => {
            let tmux = session::ProcessTmuxClient;
            let remote_prefix = env.get_var(session::OPENCODE_REMOTE_DIR_PREFIX_ENV);
            let outcome = session::switch_slot_mode(
                &session,
                slot,
                mode,
                operator,
                remote_prefix.as_deref(),
                &tmux,
            )?;
            Ok(format!(
                "internal mode complete: session={}; slot={}; mode={}",
                outcome.session_name,
                outcome.slot_id,
                outcome.mode.label()
            ))
        }
        InternalCommand::Popup { session, slot } => {
            let tmux = session::ProcessTmuxClient;
            let outcome = session::toggle_popup_shell(&session, slot, &tmux)?;
            Ok(format!(
                "internal popup complete: session={}; slot={}; action={}; cwd={}; width_pct={}; height_pct={}",
                outcome.session_name,
                outcome.slot_id,
                outcome.action.label(),
                outcome.cwd,
                outcome.width_pct,
                outcome.height_pct
            ))
        }
        InternalCommand::Auxiliary { session, action } => {
            let tmux = session::ProcessTmuxClient;
            let open = matches!(action, AuxiliaryAction::Open);
            let outcome = session::auxiliary_viewer(&session, open, &tmux)?;
            Ok(format!(
                "internal auxiliary complete: session={}; action={}; window_name={}; window_id={}",
                outcome.session_name,
                outcome.action.label(),
                outcome.window_name,
                outcome.window_id.unwrap_or_else(|| String::from("none"))
            ))
        }
        InternalCommand::Teardown { session } => {
            let tmux = session::ProcessTmuxClient;
            let outcome = session::teardown_session(&session, &tmux)?;
            Ok(format!(
                "internal teardown complete: session={}; project_session_removed={}; helper_sessions_removed={}; helper_processes_removed={}",
                outcome.session_name,
                outcome.project_session_removed,
                outcome.helper_sessions_removed,
                outcome.helper_processes_removed
            ))
        }
        InternalCommand::Preset { session, preset } => {
            let tmux = session::ProcessTmuxClient;
            let outcome = session::apply_layout_preset(&session, preset, &tmux)?;
            Ok(format!(
                "internal preset complete: session={}; preset={}",
                outcome.session_name,
                outcome.preset.label()
            ))
        }
    }
}

fn source_label(source: ValueSource) -> &'static str {
    source.label()
}

fn format_repair_message(outcome: &session::SessionRepairExecution) -> String {
    format!(
        "repair complete: session={}; action={}; healthy_slots={}; missing_visible_slots={}; missing_backing_slots={}; recreate_order={}; recreated_slots={}",
        outcome.session_name,
        outcome.action_label(),
        format_slot_ids(&outcome.healthy_slots),
        format_slot_ids(&outcome.missing_visible_slots),
        format_slot_ids(&outcome.missing_backing_slots),
        format_slot_ids(&outcome.recreate_order),
        format_slot_ids(&outcome.recreated_slots)
    )
}

fn format_slot_ids(slot_ids: &[u8]) -> String {
    if slot_ids.is_empty() {
        return String::from("none");
    }
    slot_ids
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::io;
    use std::path::Path;

    use tempfile::tempdir;

    use super::AppError;
    use super::execute_default_session_flow_for_project_dir;
    use super::execute_with_opener;
    use super::format_repair_message;
    use crate::cli::{Cli, Command, LogsCommand};
    use crate::config::OperatingSystem;
    use crate::logging::LogOpener;
    use crate::session::{
        AuxiliaryViewerOutcome, LayoutPreset, PopupShellOutcome, SessionError, SlotMode,
        TeardownOutcome, TmuxClient,
    };

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

    #[test]
    fn repair_message_reports_action_and_slot_lists() {
        let rendered = format_repair_message(&crate::session::SessionRepairExecution {
            session_name: String::from("ezm-project-session"),
            healthy_slots: vec![1, 2, 3, 5],
            missing_visible_slots: vec![4],
            missing_backing_slots: Vec::new(),
            recreate_order: vec![4],
            recreated_slots: vec![4],
        });

        assert!(rendered.contains("repair complete: session=ezm-project-session"));
        assert!(rendered.contains("action=reconcile"));
        assert!(rendered.contains("healthy_slots=1,2,3,5"));
        assert!(rendered.contains("missing_visible_slots=4"));
        assert!(rendered.contains("missing_backing_slots=none"));
        assert!(rendered.contains("recreated_slots=4"));
    }

    struct InterruptingTmuxClient {
        teardown_calls: std::cell::RefCell<Vec<String>>,
    }

    impl InterruptingTmuxClient {
        fn new() -> Self {
            Self {
                teardown_calls: std::cell::RefCell::new(Vec::new()),
            }
        }

        fn teardown_calls(&self) -> Vec<String> {
            self.teardown_calls.borrow().clone()
        }
    }

    impl TmuxClient for InterruptingTmuxClient {
        fn session_exists(&self, _: &str) -> Result<bool, SessionError> {
            Ok(true)
        }

        fn create_detached_session(&self, _: &str, _: &Path) -> Result<(), SessionError> {
            Ok(())
        }

        fn attach_session(&self, _: &str) -> Result<(), SessionError> {
            Err(SessionError::Interrupted)
        }

        fn validate_session_invariants(&self, _: &str) -> Result<(), SessionError> {
            Ok(())
        }

        fn bootstrap_default_layout(&self, _: &str, _: &Path) -> Result<(), SessionError> {
            Ok(())
        }

        fn swap_slot_with_center(&self, _: &str, _: u8) -> Result<(), SessionError> {
            Ok(())
        }

        fn apply_layout_preset(&self, _: &str, _: LayoutPreset) -> Result<(), SessionError> {
            Ok(())
        }

        fn switch_slot_mode(
            &self,
            _: &str,
            _: u8,
            _: SlotMode,
            _: Option<&str>,
            _: Option<&str>,
        ) -> Result<(), SessionError> {
            Ok(())
        }

        fn toggle_popup_shell(&self, _: &str, _: u8) -> Result<PopupShellOutcome, SessionError> {
            Err(SessionError::TmuxCommandFailed {
                command: String::from("toggle-popup"),
                stderr: String::from("not used in this test"),
            })
        }

        fn auxiliary_viewer(
            &self,
            _: &str,
            _: bool,
        ) -> Result<AuxiliaryViewerOutcome, SessionError> {
            Err(SessionError::TmuxCommandFailed {
                command: String::from("auxiliary-viewer"),
                stderr: String::from("not used in this test"),
            })
        }

        fn teardown_session(&self, session_name: &str) -> Result<TeardownOutcome, SessionError> {
            self.teardown_calls
                .borrow_mut()
                .push(session_name.to_owned());
            Ok(TeardownOutcome {
                session_name: session_name.to_owned(),
                helper_sessions_removed: 0,
                helper_processes_removed: 0,
                project_session_removed: false,
            })
        }

        fn analyze_session_damage(
            &self,
            _: &str,
        ) -> Result<crate::session::SessionDamageAnalysis, SessionError> {
            Err(SessionError::TmuxCommandFailed {
                command: String::from("analyze-damage"),
                stderr: String::from("not used in this test"),
            })
        }

        fn reconcile_session_damage(
            &self,
            _: &str,
        ) -> Result<crate::session::SessionRepairOutcome, SessionError> {
            Err(SessionError::TmuxCommandFailed {
                command: String::from("reconcile-damage"),
                stderr: String::from("not used in this test"),
            })
        }
    }

    #[test]
    fn interrupted_default_flow_runs_teardown_and_maps_to_app_interrupt() {
        let temp = tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        std::fs::create_dir(&project_dir).expect("project dir");

        let expected_session = crate::session::resolve_session_identity(&project_dir)
            .expect("session identity")
            .session_name;

        let tmux = InterruptingTmuxClient::new();
        let error = execute_default_session_flow_for_project_dir(&project_dir, &tmux)
            .expect_err("interrupt should map to app error");

        assert!(matches!(error, AppError::Interrupted));
        assert_eq!(tmux.teardown_calls(), vec![expected_session]);
    }
}
