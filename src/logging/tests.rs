use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tempfile::tempdir;

use super::Clock;
use super::LogOpener;
use super::LoggingError;
use super::ProcessLogOpener;
use super::RunIdSource;
use super::append_launch_log_event;
use super::fallback_log_root;
use super::initialize_launch_log;
use super::open::latest_log_file;
use super::open_latest_log;
use super::resolve_primary_log_root;
use crate::config::OperatingSystem;

struct FixedClock {
    now: SystemTime,
}

impl Clock for FixedClock {
    fn now(&self) -> SystemTime {
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
fn linux_whitespace_xdg_state_home_falls_back_to_home_state() {
    let mut env = HashMap::new();
    env.insert(String::from("XDG_STATE_HOME"), String::from("   \t"));
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
fn unsupported_platform_is_rejected_for_log_roots() {
    let env = HashMap::<String, String>::new();

    let error =
        resolve_primary_log_root(&env, OperatingSystem::Unsupported).expect_err("must fail");
    assert!(matches!(error, LoggingError::UnsupportedPlatform { .. }));
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
        now: UNIX_EPOCH + Duration::from_secs(1_710_000_000),
    };
    let run_ids = SequenceRunIds::from(&["run-a", "run/../b"]);

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
    assert_eq!(first_name, "20240309-160000-000000000-run-a.log");
    assert!(
        first_name
            .bytes()
            .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.') })
    );
    assert!(
        second
            .file_path
            .ends_with("20240309-160000-000000000-run_2f_2e_2e_2fb.log")
    );
    assert_eq!(second.file_path.parent(), Some(second.root.as_path()));
}

#[test]
fn launch_log_names_sort_chronologically_within_one_second() {
    let state_root = tempdir().expect("state root");
    let fallback_root = tempdir().expect("fallback root");
    let env = HashMap::from([
        (
            String::from("XDG_STATE_HOME"),
            state_root.path().display().to_string(),
        ),
        (String::from("HOME"), String::from("/tmp/home")),
    ]);
    let run_ids = SequenceRunIds::from(&["first", "second"]);

    let first = initialize_launch_log(
        &env,
        OperatingSystem::Linux,
        &FixedClock {
            now: UNIX_EPOCH + Duration::new(1_710_000_000, 1),
        },
        &run_ids,
        fallback_root.path(),
    )
    .expect("first launch log");
    let second = initialize_launch_log(
        &env,
        OperatingSystem::Linux,
        &FixedClock {
            now: UNIX_EPOCH + Duration::new(1_710_000_000, 2),
        },
        &run_ids,
        fallback_root.path(),
    )
    .expect("second launch log");

    assert!(first.file_path.file_name() < second.file_path.file_name());
    assert_eq!(
        latest_log_file(&second.root).expect("latest log"),
        second.file_path
    );
}

#[test]
fn launch_log_creation_retries_filename_collisions() {
    let state_root = tempdir().expect("state root");
    let fallback_root = tempdir().expect("fallback root");
    let env = HashMap::from([
        (
            String::from("XDG_STATE_HOME"),
            state_root.path().display().to_string(),
        ),
        (String::from("HOME"), String::from("/tmp/home")),
    ]);
    let clock = FixedClock {
        now: UNIX_EPOCH + Duration::from_secs(1_710_000_000),
    };
    let run_ids = SequenceRunIds::from(&["same", "same", "retry"]);

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
    .expect("collision retry should create a log");

    assert_ne!(first.file_path, second.file_path);
    assert!(
        second
            .file_path
            .ends_with("20240309-160000-000000000-retry.log")
    );
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
        now: UNIX_EPOCH + Duration::from_secs(1_710_000_000),
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
fn appends_launch_event_lines_to_existing_log_file() {
    let state_root = tempdir().expect("state root");
    let fallback_root = tempdir().expect("fallback root");

    let mut env = HashMap::new();
    env.insert(
        String::from("XDG_STATE_HOME"),
        state_root.path().display().to_string(),
    );
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let clock = FixedClock {
        now: UNIX_EPOCH + Duration::from_secs(1_710_000_000),
    };
    let run_ids = SequenceRunIds::from(&["event-append"]);

    let launch = initialize_launch_log(
        &env,
        OperatingSystem::Linux,
        &clock,
        &run_ids,
        fallback_root.path(),
    )
    .expect("launch log should initialize");

    append_launch_log_event(
        &launch.file_path,
        "launch-failure",
        "remote-path routing failed",
    )
    .expect("event append should succeed");

    let content = fs::read_to_string(&launch.file_path).expect("read launch log");
    assert!(content.contains("event=launch-log-created"));
    assert!(content.contains("event=launch-failure; detail=remote-path routing failed"));
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
fn latest_log_prefers_parsed_timestamp_over_filename_lexical_order() {
    let root = tempdir().expect("root");
    fs::write(
        root.path().join("zzzzzzzz-999999-not-a-timestamp.log"),
        "junk",
    )
    .expect("write junk");
    fs::write(root.path().join("20260319-101700-run-2.log"), "new").expect("write canonical");

    let latest = latest_log_file(root.path()).expect("latest log path");
    assert_eq!(latest, root.path().join("20260319-101700-run-2.log"));
}

#[test]
fn latest_log_falls_back_to_file_mtime_when_name_timestamp_is_unparseable() {
    let root = tempdir().expect("root");
    let older = root.path().join("invalid-a.log");
    let newer = root.path().join("invalid-b.log");
    fs::write(&older, "old").expect("write old");
    fs::write(&newer, "new").expect("write new");

    let latest = latest_log_file(root.path()).expect("latest log path");
    assert_eq!(latest, newer);
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

#[test]
fn process_log_opener_rejects_unsupported_platform() {
    let root = tempdir().expect("root");
    let path = root.path().join("20260319-101700-run-2.log");
    fs::write(&path, "new").expect("write new");

    let error = ProcessLogOpener
        .open(OperatingSystem::Unsupported, &path)
        .expect_err("unsupported platform should fail");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
}
