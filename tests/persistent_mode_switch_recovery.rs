#![allow(dead_code)]

mod support;

use ez_mux::session::resolve_session_identity;
use support::foundation_harness::FoundationHarness;

const SLOT_ID: u8 = 3;

#[derive(Debug, Eq, PartialEq)]
struct VisibleSlotState {
    pane_id: String,
    session_cwd: String,
    session_mode: String,
    session_worktree: String,
    pane_cwd: String,
    pane_mode: String,
    pane_worktree: String,
    pane_slot_id: String,
    pane_window_id: String,
    pane_session_name: String,
    window_zoomed: String,
}

#[test]
fn post_swap_registry_update_failure_is_recovered_before_topology_changes() {
    let (harness, session) = launched_session("persistent-registry-recovery");
    let baseline = zoomed_slot_state(&harness, &session);
    let target_mode = different_mode(&baseline.session_mode);
    let marker = harness.work_dir().join("fail-registry-once");
    let match_fragment = format!("set-option -t {session} @ezm_slot_{SLOT_ID}_pane ");

    let failed = switch_mode_with_failure(
        &harness,
        &session,
        target_mode,
        &match_fragment,
        marker.to_str().expect("marker path should be UTF-8"),
    );
    assert_ne!(failed.exit_code, 0, "registry failure should surface");
    assert!(failed.stderr.contains("injected tmux failure"));
    assert_eq!(read_slot_state(&harness, &session), baseline);

    let cached_pane = backing_pane(&harness, &session, target_mode);
    assert_eq!(display_value(&harness, &cached_pane, "#{pane_dead}"), "0");
    assert_eq!(
        tmux_value(
            &harness,
            &[
                "display-message",
                "-p",
                "-t",
                &cached_pane,
                "#{session_name}"
            ],
        ),
        format!("{session}__mode_cache")
    );

    let retry = switch_mode(&harness, &session, target_mode, &[]);
    assert_eq!(retry.exit_code, 0, "retry failed: {}", retry.stderr);
    let switched = read_slot_state(&harness, &session);
    assert_eq!(switched.pane_id, cached_pane);
    assert_eq!(switched.session_mode, target_mode);
    assert_eq!(switched.window_zoomed, "1");
}

#[test]
fn caller_metadata_failures_leave_the_previous_topology_and_cached_process_recoverable() {
    let (harness, session) = launched_session("persistent-metadata-recovery");
    let baseline = zoomed_slot_state(&harness, &session);
    let target_mode = different_mode(&baseline.session_mode);
    let failures = [
        (
            "cwd",
            format!("set-option -t {session} @ezm_slot_{SLOT_ID}_cwd "),
        ),
        (
            "mode",
            format!("set-option -t {session} @ezm_slot_{SLOT_ID}_mode "),
        ),
    ];
    let mut cached_pane = None;

    for (name, match_fragment) in failures {
        let marker = harness
            .work_dir()
            .join(format!("fail-metadata-{name}-once"));
        let failed = switch_mode_with_failure(
            &harness,
            &session,
            target_mode,
            &match_fragment,
            marker.to_str().expect("marker path should be UTF-8"),
        );
        assert_ne!(
            failed.exit_code, 0,
            "{name} metadata failure should surface"
        );
        assert!(failed.stderr.contains("injected tmux failure"));
        assert_eq!(read_slot_state(&harness, &session), baseline);

        let current_cached_pane = backing_pane(&harness, &session, target_mode);
        assert_eq!(
            display_value(&harness, &current_cached_pane, "#{pane_dead}"),
            "0"
        );
        if let Some(expected) = cached_pane.as_ref() {
            assert_eq!(&current_cached_pane, expected);
        } else {
            cached_pane = Some(current_cached_pane);
        }
    }

    let retry = switch_mode(&harness, &session, target_mode, &[]);
    assert_eq!(retry.exit_code, 0, "retry failed: {}", retry.stderr);
    let switched = read_slot_state(&harness, &session);
    assert_eq!(
        switched.pane_id,
        cached_pane.expect("cached pane should exist")
    );
    assert_eq!(switched.session_mode, target_mode);
    assert_eq!(switched.window_zoomed, "1");
}

fn launched_session(suite: &str) -> (FoundationHarness, String) {
    let harness = FoundationHarness::new_for_suite(suite)
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));
    harness
        .reset_scenario_state()
        .unwrap_or_else(|error| panic!("scenario reset failed: {error}"));
    let session = resolve_session_identity(harness.project_root())
        .expect("project should have a session identity")
        .session_name;
    let launch = harness
        .run_ezm(&[], &[], 0)
        .expect("session launch should execute");
    assert_eq!(
        launch.exit_code, 0,
        "session launch failed: {}",
        launch.stderr
    );
    (harness, session)
}

fn zoomed_slot_state(harness: &FoundationHarness, session: &str) -> VisibleSlotState {
    let pane_id = tmux_value(
        harness,
        &[
            "show-options",
            "-v",
            "-t",
            session,
            &format!("@ezm_slot_{SLOT_ID}_pane"),
        ],
    );
    harness
        .tmux_capture(&["resize-pane", "-Z", "-t", &pane_id])
        .expect("slot should zoom");
    let state = read_slot_state(harness, session);
    assert_eq!(state.window_zoomed, "1");
    state
}

fn read_slot_state(harness: &FoundationHarness, session: &str) -> VisibleSlotState {
    let pane_id = tmux_value(
        harness,
        &[
            "show-options",
            "-v",
            "-t",
            session,
            &format!("@ezm_slot_{SLOT_ID}_pane"),
        ],
    );
    VisibleSlotState {
        session_cwd: session_option(harness, session, "cwd"),
        session_mode: session_option(harness, session, "mode"),
        session_worktree: session_option(harness, session, "worktree"),
        pane_cwd: pane_option(harness, &pane_id, "@ezm_slot_cwd"),
        pane_mode: pane_option(harness, &pane_id, "@ezm_slot_mode"),
        pane_worktree: pane_option(harness, &pane_id, "@ezm_slot_worktree"),
        pane_slot_id: pane_option(harness, &pane_id, "@ezm_slot_id"),
        pane_window_id: display_value(harness, &pane_id, "#{window_id}"),
        pane_session_name: display_value(harness, &pane_id, "#{session_name}"),
        window_zoomed: display_value(harness, &pane_id, "#{window_zoomed_flag}"),
        pane_id,
    }
}

fn session_option(harness: &FoundationHarness, session: &str, suffix: &str) -> String {
    tmux_value(
        harness,
        &[
            "show-options",
            "-v",
            "-t",
            session,
            &format!("@ezm_slot_{SLOT_ID}_{suffix}"),
        ],
    )
}

fn pane_option(harness: &FoundationHarness, pane_id: &str, key: &str) -> String {
    tmux_value(harness, &["show-options", "-p", "-v", "-t", pane_id, key])
}

fn display_value(harness: &FoundationHarness, pane_id: &str, format: &str) -> String {
    tmux_value(harness, &["display-message", "-p", "-t", pane_id, format])
}

fn backing_pane(harness: &FoundationHarness, session: &str, mode: &str) -> String {
    tmux_value(
        harness,
        &[
            "show-options",
            "-v",
            "-t",
            session,
            &format!("@ezm_slot_{SLOT_ID}_backing_{mode}_pane"),
        ],
    )
}

fn tmux_value(harness: &FoundationHarness, args: &[&str]) -> String {
    harness
        .tmux_capture(args)
        .unwrap_or_else(|error| panic!("tmux {args:?} failed: {error}"))
        .trim()
        .to_owned()
}

fn different_mode(current: &str) -> &'static str {
    if current == "shell" {
        "lazygit"
    } else {
        "shell"
    }
}

fn switch_mode_with_failure(
    harness: &FoundationHarness,
    session: &str,
    mode: &str,
    match_fragment: &str,
    marker: &str,
) -> support::foundation_harness::CmdOutput {
    switch_mode(
        harness,
        session,
        mode,
        &[
            ("E2E_TMUX_FAIL_MATCH", match_fragment),
            ("E2E_TMUX_FAIL_ONCE", marker),
        ],
    )
}

fn switch_mode(
    harness: &FoundationHarness,
    session: &str,
    mode: &str,
    env: &[(&str, &str)],
) -> support::foundation_harness::CmdOutput {
    harness
        .run_ezm(
            &[
                "__internal",
                "mode",
                "--session",
                session,
                "--slot",
                &SLOT_ID.to_string(),
                "--mode",
                mode,
            ],
            env,
            0,
        )
        .expect("mode switch should execute")
}
