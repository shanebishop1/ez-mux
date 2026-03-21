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

    fn focus_slot(&self, _: &str, _: u8) -> Result<(), SessionError> {
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
        _: Option<&crate::session::SharedServerAttachConfig>,
    ) -> Result<(), SessionError> {
        Ok(())
    }

    fn toggle_popup_shell(&self, _: &str, _: u8) -> Result<PopupShellOutcome, SessionError> {
        Err(SessionError::TmuxCommandFailed {
            command: String::from("toggle-popup"),
            stderr: String::from("not used in this test"),
        })
    }

    fn auxiliary_viewer(&self, _: &str, _: bool) -> Result<AuxiliaryViewerOutcome, SessionError> {
        Ok(AuxiliaryViewerOutcome {
            session_name: String::from("ezm-session"),
            action: crate::session::AuxiliaryViewerAction::SkippedUnavailable,
            window_name: String::from("beads-viewer"),
            window_id: None,
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
    let error = execute_default_session_flow_for_project_dir(&project_dir, None, &tmux)
        .expect_err("interrupt should map to app error");

    assert!(matches!(error, AppError::Interrupted));
    assert_eq!(tmux.teardown_calls(), vec![expected_session]);
}
