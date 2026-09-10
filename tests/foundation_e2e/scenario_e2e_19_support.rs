use std::fs;
use std::path::PathBuf;

use crate::support::foundation_harness::{CmdOutput, FoundationHarness};

use super::foundation_support::extract_session_name;

pub(crate) struct SessionAuthProjects {
    pub(crate) a: PathBuf,
    pub(crate) b: PathBuf,
    pub(crate) empty: PathBuf,
}

pub(crate) fn prepare_session_auth_projects(harness: &FoundationHarness) -> SessionAuthProjects {
    let root = harness.work_dir().join("session-auth");
    let project_a = root.join("project-a");
    let project_b = root.join("project-b");
    let project_empty = root.join("project-empty");
    fs::create_dir_all(&project_a).expect("E2E-19 project A");
    fs::create_dir_all(&project_b).expect("E2E-19 project B");
    fs::create_dir_all(&project_empty).expect("E2E-19 empty project");

    SessionAuthProjects {
        a: project_a,
        b: project_b,
        empty: project_empty,
    }
}

pub(crate) fn set_global_session_auth_fixture(harness: &FoundationHarness) {
    harness
        .tmux_capture(&[
            "set-environment",
            "-g",
            "OPENCODE_SERVER_PASSWORD",
            "global-secret-must-remain",
        ])
        .unwrap_or_else(|error| panic!("E2E-19 failed setting global fixture: {error}"));
}

pub(crate) struct SessionAuthRuns {
    pub(crate) a: CmdOutput,
    pub(crate) b: CmdOutput,
    pub(crate) empty: CmdOutput,
}

pub(crate) fn run_session_auth_scenarios(
    harness: &FoundationHarness,
    projects: &SessionAuthProjects,
) -> SessionAuthRuns {
    let project_a = harness
        .run_ezm_in_dir(
            &projects.a,
            &["--verbose"],
            &[
                ("EZM_REMOTE_PATH", "/srv/remotes"),
                ("EZM_REMOTE_SERVER_URL", "https://shell.example:7443"),
                ("OPENCODE_SERVER_URL", "http://opencode-a.example:4096"),
                ("OPENCODE_SERVER_PASSWORD", "session-password-a"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-19 project A failed: {error}"));
    let project_b = harness
        .run_ezm_in_dir(
            &projects.b,
            &["--verbose"],
            &[
                ("EZM_REMOTE_PATH", "/srv/remotes"),
                ("EZM_REMOTE_SERVER_URL", "https://shell.example:7443"),
                ("OPENCODE_SERVER_URL", "http://opencode-b.example:4096"),
                ("OPENCODE_SERVER_PASSWORD", "session-password-b"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-19 project B failed: {error}"));
    let project_empty = harness
        .run_ezm_in_dir(
            &projects.empty,
            &["--verbose"],
            &[
                ("EZM_REMOTE_PATH", "/srv/remotes"),
                ("EZM_REMOTE_SERVER_URL", "https://shell.example:7443"),
                ("OPENCODE_SERVER_URL", "http://opencode-empty.example:4096"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-19 empty project failed: {error}"));

    SessionAuthRuns {
        a: project_a,
        b: project_b,
        empty: project_empty,
    }
}

pub(crate) struct SessionAuthSessions {
    pub(crate) a: String,
    pub(crate) b: String,
    pub(crate) empty: String,
}

pub(crate) fn extract_session_auth_sessions(runs: &SessionAuthRuns) -> SessionAuthSessions {
    let a = extract_session_name(&runs.a.stdout)
        .unwrap_or_else(|| panic!("E2E-19 project A missing session name: {}", runs.a.stdout));
    let b = extract_session_name(&runs.b.stdout)
        .unwrap_or_else(|| panic!("E2E-19 project B missing session name: {}", runs.b.stdout));
    let empty = extract_session_name(&runs.empty.stdout).unwrap_or_else(|| {
        panic!(
            "E2E-19 empty project missing session name: {}",
            runs.empty.stdout
        )
    });

    SessionAuthSessions { a, b, empty }
}

pub(crate) struct SessionAuthValues {
    pub(crate) project_a: String,
    pub(crate) project_b: String,
    pub(crate) project_empty: String,
    pub(crate) global: String,
}

pub(crate) fn capture_session_auth(
    harness: &FoundationHarness,
    sessions: &SessionAuthSessions,
) -> SessionAuthValues {
    let project_a = capture_session_password(harness, &sessions.a, "project A auth");
    let project_b = capture_session_password(harness, &sessions.b, "project B auth");
    let project_empty = capture_session_password(harness, &sessions.empty, "empty-project auth");
    let global = harness
        .tmux_capture(&["show-environment", "-g", "OPENCODE_SERVER_PASSWORD"])
        .unwrap_or_else(|error| panic!("E2E-19 failed reading global auth: {error}"));

    SessionAuthValues {
        project_a,
        project_b,
        project_empty,
        global,
    }
}

fn capture_session_password(harness: &FoundationHarness, session: &str, context: &str) -> String {
    harness
        .tmux_capture(&[
            "show-environment",
            "-t",
            session,
            "OPENCODE_SERVER_PASSWORD",
        ])
        .unwrap_or_else(|error| panic!("E2E-19 failed reading {context}: {error}"))
}

pub(crate) fn exercise_popup_parent_context(
    harness: &FoundationHarness,
    session_a: &str,
) -> CmdOutput {
    let helper_session = format!("{session_a}__popup_slot_1");
    harness
        .tmux_capture(&["new-session", "-d", "-s", &helper_session, "sleep", "30"])
        .unwrap_or_else(|error| panic!("E2E-19 failed creating helper session: {error}"));
    harness
        .tmux_capture(&[
            "set-option",
            "-t",
            &helper_session,
            "@ezm_popup_origin_session",
            session_a,
        ])
        .unwrap_or_else(|error| panic!("E2E-19 failed recording helper parent: {error}"));
    harness
        .tmux_capture(&[
            "set-option",
            "-t",
            session_a,
            "@ezm_runtime_remote_server_url",
            "not a valid authority",
        ])
        .unwrap_or_else(|error| panic!("E2E-19 failed setting parent context fixture: {error}"));
    let helper_internal = harness
        .run_ezm(
            &[
                "__internal",
                "popup",
                "--session",
                &helper_session,
                "--slot",
                "1",
            ],
            &[],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-19 helper internal invocation failed: {error}"));
    let _ = harness.tmux_capture(&["kill-session", "-t", &helper_session]);
    helper_internal
}

pub(crate) fn session_auth_assertions(
    runs: &SessionAuthRuns,
    auth: &SessionAuthValues,
    helper_internal: &CmdOutput,
) -> Vec<String> {
    vec![
        format!(
            "project A session auth is distinct: {}",
            auth.project_a.trim() == "OPENCODE_SERVER_PASSWORD=session-password-a"
        ),
        format!(
            "project B session auth is distinct: {}",
            auth.project_b.trim() == "OPENCODE_SERVER_PASSWORD=session-password-b"
        ),
        format!(
            "passwords do not cross projects: {}",
            auth.project_a != auth.project_b
                && !auth.project_a.contains("session-password-b")
                && !auth.project_b.contains("session-password-a")
        ),
        format!(
            "empty project masks inherited auth: {}",
            auth.project_empty.trim() == "OPENCODE_SERVER_PASSWORD="
        ),
        format!(
            "global auth remains unchanged: {}",
            auth.global.trim() == "OPENCODE_SERVER_PASSWORD=global-secret-must-remain"
        ),
        format!(
            "diagnostics omit fake secrets: {}",
            !runs.a.stderr.contains("session-password-a")
                && !runs.b.stderr.contains("session-password-b")
        ),
        format!(
            "helper internal flow resolves parent context: {}",
            helper_internal
                .stderr
                .contains("invalid remote ssh authority")
        ),
    ]
}

pub(crate) fn session_auth_passes(
    runs: &SessionAuthRuns,
    auth: &SessionAuthValues,
    helper_internal: &CmdOutput,
    settle_stable: bool,
) -> bool {
    runs.a.exit_code == 0
        && runs.b.exit_code == 0
        && runs.empty.exit_code == 0
        && auth.project_a.trim() == "OPENCODE_SERVER_PASSWORD=session-password-a"
        && auth.project_b.trim() == "OPENCODE_SERVER_PASSWORD=session-password-b"
        && auth.project_empty.trim() == "OPENCODE_SERVER_PASSWORD="
        && auth.global.trim() == "OPENCODE_SERVER_PASSWORD=global-secret-must-remain"
        && !runs.a.stderr.contains("session-password-a")
        && !runs.b.stderr.contains("session-password-b")
        && helper_internal
            .stderr
            .contains("invalid remote ssh authority")
        && settle_stable
}
