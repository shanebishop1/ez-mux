#![allow(dead_code)]

mod support;

use std::fmt::Write as _;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use ez_mux::session::resolve_session_identity;
use serde_json::to_string;
use support::foundation_harness::{FoundationHarness, serial_test_guard};

const PASSWORD_ENV: &str = "OPENCODE_SERVER_PASSWORD";
// The intentional backslashes prove legacy tmux output normalization removes
// only the display escape it added and preserves the credential byte-for-byte.
const OWNER_PASSWORD_A: &str = "owner password;$(special)'\"\\$LITERAL";
const OWNER_PASSWORD_B: &str = "changed owner [special];\\$HOME";
const GLOBAL_PASSWORD: &str = "global secret [wrong]";

#[test]
fn new_cache_processes_use_the_owner_credential_and_mask_global_state() {
    let _guard = serial_test_guard();
    let harness = FoundationHarness::new_for_suite("runtime-auth-cache-regression")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));
    let project_dir = harness.work_dir().join("runtime-auth-project");
    fs::create_dir_all(&project_dir).expect("create runtime-auth project");
    let sentinel = harness.work_dir().join("cache-passwords.txt");
    write_runtime_auth_config(&project_dir, &sentinel, Some(OWNER_PASSWORD_A));

    harness
        .tmux_capture(&["set-environment", "-g", PASSWORD_ENV, GLOBAL_PASSWORD])
        .expect("set conflicting global password");

    let identity = resolve_session_identity(&project_dir).expect("project identity");
    let launch = harness
        .run_ezm_in_dir(&project_dir, &["--no-worktrees"], &[], 0)
        .expect("startup should execute");
    assert_eq!(launch.exit_code, 0, "startup stderr: {}", launch.stderr);
    // Slot 1's normal startup agent is intentionally configured from the
    // same file. Let that background startup settle before using the file as
    // the sentinel for the shell-to-agent cache allocations below.
    wait_for_sentinel(&sentinel, &[OWNER_PASSWORD_A]);
    fs::write(&sentinel, "").expect("reset cache sentinel after startup");

    switch_mode(&harness, &identity.session_name, 2);
    wait_for_sentinel(&sentinel, &[OWNER_PASSWORD_A]);

    harness
        .tmux_capture(&[
            "set-environment",
            "-t",
            &identity.session_name,
            PASSWORD_ENV,
            OWNER_PASSWORD_B,
        ])
        .expect("change owner session password");
    switch_mode(&harness, &identity.session_name, 3);
    wait_for_sentinel(&sentinel, &[OWNER_PASSWORD_A, OWNER_PASSWORD_B]);

    harness
        .tmux_capture(&[
            "set-environment",
            "-t",
            &identity.session_name,
            "-u",
            PASSWORD_ENV,
        ])
        .expect("remove owner session password");
    switch_mode(&harness, &identity.session_name, 4);
    wait_for_sentinel(&sentinel, &[OWNER_PASSWORD_A, OWNER_PASSWORD_B, ""]);

    teardown(&harness, &identity.session_name);
}

#[test]
fn new_cache_without_owner_password_masks_conflicting_global_state() {
    let _guard = serial_test_guard();
    let harness = FoundationHarness::new_for_suite("runtime-auth-cache-absent-owner")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));
    let project_dir = harness.work_dir().join("runtime-auth-project");
    fs::create_dir_all(&project_dir).expect("create runtime-auth project");
    let sentinel = harness.work_dir().join("cache-passwords.txt");
    write_runtime_auth_config(&project_dir, &sentinel, None);

    harness
        .tmux_capture(&["set-environment", "-g", PASSWORD_ENV, GLOBAL_PASSWORD])
        .expect("set conflicting global password");

    let identity = resolve_session_identity(&project_dir).expect("project identity");
    let launch = harness
        .run_ezm_in_dir(&project_dir, &["--no-worktrees"], &[], 0)
        .expect("startup should execute");
    assert_eq!(launch.exit_code, 0, "startup stderr: {}", launch.stderr);
    // Consume the normal startup agent sentinel so this test specifically
    // observes the first process allocated in the mode-cache session.
    wait_for_sentinel(&sentinel, &[""]);
    fs::write(&sentinel, "").expect("reset cache sentinel after startup");

    switch_mode(&harness, &identity.session_name, 2);
    wait_for_sentinel(&sentinel, &[""]);

    teardown(&harness, &identity.session_name);
}

#[test]
fn new_session_mode_cache_failure_masks_custom_agent_command() {
    let _guard = serial_test_guard();
    let harness = FoundationHarness::new_for_suite("runtime-auth-cache-new-session-failure")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));
    let project_dir = harness.work_dir().join("runtime-auth-project");
    fs::create_dir_all(&project_dir).expect("create runtime-auth project");
    let password = "fake new-session password ' with spaces";
    let token = "fake-new-session-agent-token";
    let agent_command = format!("exec sleep 3600 # --password '{password}' --token '{token}'");

    let identity = resolve_session_identity(&project_dir).expect("project identity");
    let launch = harness
        .run_ezm_in_dir(&project_dir, &["--no-worktrees"], &[], 0)
        .expect("startup should execute");
    assert_eq!(launch.exit_code, 0, "startup stderr: {}", launch.stderr);
    thread::sleep(Duration::from_secs(1));
    harness
        .settle_tmux_snapshot(Duration::from_millis(25), Duration::from_secs(2))
        .expect("startup should settle");
    configure_custom_session_context(&harness, &identity.session_name, &agent_command, password);

    let marker = harness.work_dir().join("fail-new-session-once");
    let failed = switch_mode_in_dir(
        &harness,
        &project_dir,
        &identity.session_name,
        2,
        &[
            ("E2E_TMUX_FAIL_MATCH", "new-session -d"),
            (
                "E2E_TMUX_FAIL_ONCE",
                marker.to_str().expect("failure marker path"),
            ),
            ("EZM_STARTUP_TRACE_TMUX", "1"),
        ],
    );

    assert_ne!(failed.exit_code, 0, "new-session failure should surface");
    assert!(!failed.stderr.contains(password));
    assert!(!failed.stderr.contains(token));
    assert!(
        failed
            .stderr
            .contains("OPENCODE_SERVER_PASSWORD=<redacted>"),
        "failure stderr: {}",
        failed.stderr
    );
    assert!(
        failed.stderr.contains("<mode-launch-command>"),
        "failure stderr: {}",
        failed.stderr
    );

    teardown(&harness, &identity.session_name);
}

#[test]
fn new_window_mode_cache_failure_masks_custom_agent_command() {
    let _guard = serial_test_guard();
    let harness = FoundationHarness::new_for_suite("runtime-auth-cache-new-window-failure")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));
    let project_dir = harness.work_dir().join("runtime-auth-project");
    fs::create_dir_all(&project_dir).expect("create runtime-auth project");
    let password = "fake new-window password \" with spaces";
    let token = "fake-new-window-agent-token";
    let agent_command = format!("exec sleep 3600 # --password \"{password}\" --token '{token}'");

    let identity = resolve_session_identity(&project_dir).expect("project identity");
    let launch = harness
        .run_ezm_in_dir(&project_dir, &["--no-worktrees"], &[], 0)
        .expect("startup should execute");
    assert_eq!(launch.exit_code, 0, "startup stderr: {}", launch.stderr);
    harness
        .settle_tmux_snapshot(Duration::from_millis(25), Duration::from_secs(2))
        .expect("startup should settle");
    configure_custom_session_context(&harness, &identity.session_name, &agent_command, password);

    let first_switch = switch_mode_in_dir(&harness, &project_dir, &identity.session_name, 2, &[]);
    assert_eq!(
        first_switch.exit_code, 0,
        "cache setup failed: {}",
        first_switch.stderr
    );

    let marker = harness.work_dir().join("fail-new-window-once");
    let failed = switch_mode_in_dir(
        &harness,
        &project_dir,
        &identity.session_name,
        3,
        &[
            ("E2E_TMUX_FAIL_MATCH", "new-window -d"),
            (
                "E2E_TMUX_FAIL_ONCE",
                marker.to_str().expect("failure marker path"),
            ),
            ("EZM_STARTUP_TRACE_TMUX", "1"),
        ],
    );

    assert_ne!(failed.exit_code, 0, "new-window failure should surface");
    assert!(!failed.stderr.contains(password));
    assert!(!failed.stderr.contains(token));
    assert!(
        failed.stderr.contains("<mode-launch-command>"),
        "failure stderr: {}",
        failed.stderr
    );

    teardown(&harness, &identity.session_name);
}

fn write_runtime_auth_config(
    project_dir: &std::path::Path,
    sentinel: &std::path::Path,
    password: Option<&str>,
) {
    let agent_command = format!(
        "printf '%s\\n' \"$OPENCODE_SERVER_PASSWORD\" >> {}; sleep 3600",
        toml_string(&sentinel.display().to_string())
    );
    let mut config = format!(
        "opencode_server_url = {}\nagent_command = {}\n",
        toml_string("http://127.0.0.1:4096"),
        toml_string(&agent_command)
    );
    if let Some(password) = password {
        writeln!(
            config,
            "opencode_server_password = {}",
            toml_string(password)
        )
        .expect("writing to a String should not fail");
    }
    FoundationHarness::write_file(&project_dir.join("ez-mux.toml"), &config)
        .expect("write runtime-auth config");
}

fn configure_custom_session_context(
    harness: &FoundationHarness,
    session: &str,
    agent_command: &str,
    password: &str,
) {
    harness
        .tmux_capture(&["set-environment", "-t", session, PASSWORD_ENV, password])
        .expect("set fake session password");
    harness
        .tmux_capture(&[
            "set-option",
            "-t",
            session,
            "@ezm_runtime_agent_command",
            agent_command,
        ])
        .expect("set fake session agent command");
}

fn toml_string(value: &str) -> String {
    to_string(value).expect("runtime-auth fixture value should serialize")
}

fn teardown(harness: &FoundationHarness, session: &str) {
    let result = harness
        .run_ezm(&["__internal", "teardown", "--session", session], &[], 0)
        .expect("teardown should execute");
    assert_eq!(result.exit_code, 0, "teardown stderr: {}", result.stderr);
}

fn switch_mode(harness: &FoundationHarness, session: &str, slot: u8) {
    let result = switch_mode_in_dir(harness, harness.project_root(), session, slot, &[]);
    assert_eq!(result.exit_code, 0, "mode switch stderr: {}", result.stderr);
}

fn switch_mode_in_dir(
    harness: &FoundationHarness,
    project_dir: &std::path::Path,
    session: &str,
    slot: u8,
    env: &[(&str, &str)],
) -> support::foundation_harness::CmdOutput {
    let slot = slot.to_string();
    harness
        .run_ezm_in_dir(
            project_dir,
            &[
                "__internal",
                "mode",
                "--session",
                session,
                "--slot",
                &slot,
                "--mode",
                "agent",
            ],
            env,
            0,
        )
        .expect("mode switch should execute")
}

fn wait_for_sentinel(path: &std::path::Path, expected: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let values = fs::read_to_string(path)
            .unwrap_or_default()
            .split_terminator('\n')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if values == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for cache sentinel {}; expected {expected:?}, got {values:?}",
            path.display()
        );
        thread::sleep(Duration::from_millis(25));
    }
}
