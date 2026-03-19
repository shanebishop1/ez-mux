use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use tempfile::tempdir;
use time::OffsetDateTime;

use super::Clock;
use super::LogOpener;
use super::LoggingError;
use super::RunIdSource;
use super::fallback_log_root;
use super::initialize_launch_log;
use super::open::latest_log_file;
use super::open_latest_log;
use super::resolve_primary_log_root;
use crate::config::OperatingSystem;

struct FixedClock {
    now: OffsetDateTime,
}

impl Clock for FixedClock {
    fn now_utc(&self) -> OffsetDateTime {
        self.now
    }
}

struct SequenceRunIds {
    values: std::sync::Mutex<Vec<String>>,
}

impl SequenceRunIds {
    fn from(values: &[&str]) -> Self {
        Self {
            values: std::sync::Mutex::new(values.iter().map(|s| (*s).to_owned()).collect()),
        }
    }
}

impl RunIdSource for SequenceRunIds {
    fn next_run_id(&self) -> String {
        self.values.lock().expect("lock").remove(0)
    }
}

struct OkOpener;

impl LogOpener for OkOpener {
    fn open(&self, _: OperatingSystem, _: &Path) -> io::Result<()> {
        Ok(())
    }
}

struct FailOpener;

impl LogOpener for FailOpener {
    fn open(&self, _: OperatingSystem, _: &Path) -> io::Result<()> {
        Err(io::Error::other("open failed"))
    }
}

#[test]
fn linux_log_root_prefers_xdg_state_home() {
    let mut env = HashMap::new();
    env.insert(String::from("XDG_STATE_HOME"), String::from("/tmp/state"));
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let resolved =
        resolve_primary_log_root(&env, OperatingSystem::Linux).expect("path should resolve");
    assert_eq!(resolved, std::path::PathBuf::from("/tmp/state/ez-mux/logs"));
}

#[test]
fn linux_empty_xdg_state_home_falls_back_to_home_state() {
    let mut env = HashMap::new();
    env.insert(String::from("XDG_STATE_HOME"), String::new());
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let resolved =
        resolve_primary_log_root(&env, OperatingSystem::Linux).expect("path should resolve");
    assert_eq!(
        resolved,
        std::path::PathBuf::from("/tmp/home/.local/state/ez-mux/logs")
    );
}

#[test]
fn linux_log_root_falls_back_to_home_state() {
    let mut env = HashMap::new();
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let resolved =
        resolve_primary_log_root(&env, OperatingSystem::Linux).expect("path should resolve");
    assert_eq!(
        resolved,
        std::path::PathBuf::from("/tmp/home/.local/state/ez-mux/logs")
    );
}

#[test]
fn linux_empty_home_is_treated_as_missing() {
    let mut env = HashMap::new();
    env.insert(String::from("HOME"), String::new());

    let error =
        resolve_primary_log_root(&env, OperatingSystem::Linux).expect_err("empty HOME must fail");
    assert!(matches!(error, LoggingError::MissingHome { .. }));
}

#[test]
fn macos_log_root_uses_library_logs() {
    let mut env = HashMap::new();
    env.insert(String::from("HOME"), String::from("/Users/tester"));

    let resolved =
        resolve_primary_log_root(&env, OperatingSystem::MacOs).expect("path should resolve");
    assert_eq!(
        resolved,
        std::path::PathBuf::from("/Users/tester/Library/Logs/ez-mux")
    );
}

#[test]
fn creates_unique_per_launch_log_files() {
    let state_root = tempdir().expect("state root");
    let fallback_root = tempdir().expect("fallback root");

    let mut env = HashMap::new();
    env.insert(
        String::from("XDG_STATE_HOME"),
        state_root.path().display().to_string(),
    );
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let clock = FixedClock {
        now: OffsetDateTime::from_unix_timestamp(1_710_000_000).expect("timestamp"),
    };
    let run_ids = SequenceRunIds::from(&["run-a", "run-b"]);

    let first = initialize_launch_log(
        &env,
        OperatingSystem::Linux,
        &clock,
        &run_ids,
        fallback_root.path(),
    )
    .expect("first launch log");
    let second = initialize_launch_log(
        &env,
        OperatingSystem::Linux,
        &clock,
        &run_ids,
        fallback_root.path(),
    )
    .expect("second launch log");

    assert_ne!(first.file_path, second.file_path);
    assert!(first.file_path.exists());
    assert!(second.file_path.exists());

    let first_name = first
        .file_path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .expect("name");
    assert!(first_name.ends_with("-run-a.log"));
    assert_eq!(first_name.len(), "YYYYMMDD-HHMMSS-run-a.log".len());
}

#[test]
fn falls_back_when_primary_log_root_creation_fails() {
    let temp = tempdir().expect("temp");
    let primary_base = temp.path().join("primary-base-file");
    fs::write(&primary_base, "not a directory").expect("write file");
    let fallback_base = temp.path().join("fallback-base");
    fs::create_dir_all(&fallback_base).expect("create fallback base");

    let mut env = HashMap::new();
    env.insert(
        String::from("XDG_STATE_HOME"),
        primary_base.display().to_string(),
    );
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let clock = FixedClock {
        now: OffsetDateTime::from_unix_timestamp(1_710_000_000).expect("timestamp"),
    };
    let run_ids = SequenceRunIds::from(&["fallback"]);

    let launch = initialize_launch_log(
        &env,
        OperatingSystem::Linux,
        &clock,
        &run_ids,
        &fallback_base,
    )
    .expect("launch log should still initialize");

    assert_eq!(launch.root, fallback_log_root(&fallback_base));
    let warning = launch.warning.expect("warning should be present");
    assert!(warning.contains("failed to create primary log root"));
    assert!(warning.contains(&launch.root.display().to_string()));
}

#[test]
fn selects_and_opens_latest_log() {
    let root = tempdir().expect("root");
    fs::write(root.path().join("20260319-101500-run-1.log"), "old").expect("write old");
    fs::write(root.path().join("20260319-101700-run-2.log"), "new").expect("write new");

    let opened = open_latest_log(root.path(), OperatingSystem::Linux, &OkOpener)
        .expect("open latest should succeed");
    assert_eq!(opened, root.path().join("20260319-101700-run-2.log"));
}

#[test]
fn returns_error_when_no_logs_exist() {
    let root = tempdir().expect("root");

    let error = latest_log_file(root.path()).expect_err("must error without logs");
    assert!(matches!(error, LoggingError::NoLogFiles { .. }));
}

#[test]
fn returns_error_when_open_command_fails() {
    let root = tempdir().expect("root");
    fs::write(root.path().join("20260319-101700-run-2.log"), "new").expect("write new");

    let error = open_latest_log(root.path(), OperatingSystem::Linux, &FailOpener)
        .expect_err("open should fail");

    assert!(matches!(error, LoggingError::OpenLogFailed { .. }));
}
