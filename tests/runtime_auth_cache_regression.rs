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
    let slot = slot.to_string();
    let result = harness
        .run_ezm(
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
            &[],
            0,
        )
        .expect("mode switch should execute");
    assert_eq!(result.exit_code, 0, "mode switch stderr: {}", result.stderr);
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
