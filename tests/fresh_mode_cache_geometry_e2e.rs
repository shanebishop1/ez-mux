#![allow(dead_code)]

mod support;

#[path = "core_session_e2e/core_support.rs"]
mod core_support;

use std::fs;

use core_support::{
    extract_stdout_field, pane_graph_stable, prepare_fresh_create_path, read_pane_graph,
    settle_snapshot,
};
use support::foundation_harness::{FoundationHarness, serial_test_guard};

const SLOT_MODES: [&str; 4] = ["agent", "shell", "neovim", "lazygit"];

#[derive(Debug, Eq, PartialEq)]
struct PaneIdentity {
    pane_id: String,
    process_id: String,
}

#[allow(clippy::too_many_lines)]
#[test]
fn fresh_mode_cache_allocations_do_not_consume_visible_geometry() {
    let _guard = serial_test_guard();
    let harness = FoundationHarness::new_for_suite("fresh-mode-cache-geometry")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));
    let project_dir = harness.work_dir().join("fresh-mode-cache-project");
    fs::create_dir_all(&project_dir).unwrap_or_else(|error| {
        panic!(
            "failed creating fresh mode-cache project {}: {error}",
            project_dir.display()
        )
    });
    let expected_session = prepare_fresh_create_path(&harness, &project_dir)
        .unwrap_or_else(|error| panic!("fresh mode-cache setup failed: {error}"));

    let launch = harness
        .run_ezm_in_dir(&project_dir, &["--verbose", "--no-worktrees"], &[], 0)
        .unwrap_or_else(|error| panic!("fresh mode-cache launch failed: {error}"));
    assert_eq!(launch.exit_code, 0, "launch stderr: {}", launch.stderr);
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    assert_eq!(session, expected_session);

    let dimensions = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{window_width}x#{window_height}",
        ])
        .unwrap_or_else(|error| panic!("failed reading fresh session dimensions: {error}"));
    assert_eq!(dimensions.trim(), "80x24");

    let visible_before = read_pane_graph(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading initial visible geometry: {error}"));
    assert_eq!(visible_before.len(), 5);

    let mut identities = Vec::new();
    for slot_id in 1_u8..=5 {
        let mut slot_identities = Vec::new();
        for mode in SLOT_MODES {
            assert_mode_switch_succeeded(&harness, &project_dir, &session, slot_id, mode);
            assert_eq!(
                session_option(&harness, &session, &format!("@ezm_slot_{slot_id}_mode")),
                mode,
                "slot {slot_id} did not activate mode {mode}"
            );
            let pane_id = session_option(&harness, &session, &format!("@ezm_slot_{slot_id}_pane"));
            let process_id = pane_pid(&harness, &pane_id);
            slot_identities.push(PaneIdentity {
                pane_id,
                process_id,
            });
        }
        identities.push(slot_identities);
    }

    let cache_session = format!("{session}__mode_cache");
    assert_cache_shape(&harness, &cache_session, 15);

    for (slot_index, slot_identities) in identities.iter().enumerate() {
        let slot_id = u8::try_from(slot_index + 1).expect("canonical slot index fits in u8");
        for (mode_index, mode) in SLOT_MODES.iter().enumerate() {
            assert_mode_switch_succeeded(&harness, &project_dir, &session, slot_id, mode);
            let pane_id = session_option(&harness, &session, &format!("@ezm_slot_{slot_id}_pane"));
            let process_id = pane_pid(&harness, &pane_id);
            assert_eq!(
                PaneIdentity {
                    pane_id,
                    process_id
                },
                slot_identities[mode_index],
                "slot {slot_id} mode {mode} did not reuse its pane and process"
            );
        }
    }

    let visible_after = read_pane_graph(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading final visible geometry: {error}"));
    assert!(pane_graph_stable(&visible_before, &visible_after));

    assert_cache_shape(&harness, &cache_session, 15);

    let teardown = harness
        .run_ezm(&["__internal", "teardown", "--session", &session], &[], 0)
        .unwrap_or_else(|error| panic!("mode-cache teardown failed: {error}"));
    assert_eq!(
        teardown.exit_code, 0,
        "teardown stderr: {}",
        teardown.stderr
    );
    assert!(
        teardown.stdout.contains("helper_sessions_removed=1"),
        "unexpected teardown output: {}",
        teardown.stdout
    );
    assert!(
        teardown.stdout.contains("helper_processes_removed=15"),
        "unexpected teardown output: {}",
        teardown.stdout
    );
    let remaining_sessions = harness
        .tmux_capture(&["list-sessions", "-F", "#{session_name}"])
        .unwrap_or_default();
    assert!(!remaining_sessions.lines().any(|name| {
        let name = name.trim();
        name == session || name.starts_with(&format!("{session}__"))
    }));

    let _ = settle_snapshot(&harness, "fresh mode-cache geometry");
}

fn assert_mode_switch_succeeded(
    harness: &FoundationHarness,
    project_dir: &std::path::Path,
    session: &str,
    slot_id: u8,
    mode: &str,
) {
    let switch = switch_mode(harness, project_dir, session, slot_id, mode);
    assert_eq!(
        switch.exit_code, 0,
        "slot {slot_id} {mode} switch failed: {}",
        switch.stderr
    );
}

fn switch_mode(
    harness: &FoundationHarness,
    project_dir: &std::path::Path,
    session: &str,
    slot_id: u8,
    mode: &str,
) -> support::foundation_harness::CmdOutput {
    let slot = slot_id.to_string();
    let args = [
        "__internal",
        "mode",
        "--session",
        session,
        "--slot",
        &slot,
        "--mode",
        mode,
    ];
    harness
        .run_ezm_in_dir(project_dir, &args, &[], 0)
        .unwrap_or_else(|error| panic!("slot {slot_id} {mode} invocation failed: {error}"))
}

fn session_option(harness: &FoundationHarness, session: &str, key: &str) -> String {
    harness
        .tmux_capture(&["show-options", "-v", "-t", session, key])
        .unwrap_or_else(|error| panic!("failed reading session option {key}: {error}"))
        .trim()
        .to_owned()
}

fn pane_pid(harness: &FoundationHarness, pane_id: &str) -> String {
    harness
        .tmux_capture(&["display-message", "-p", "-t", pane_id, "#{pane_pid}"])
        .unwrap_or_else(|error| panic!("failed reading process id for pane {pane_id}: {error}"))
        .trim()
        .to_owned()
}

fn assert_cache_shape(harness: &FoundationHarness, session: &str, expected_windows: usize) {
    let cache = cache_windows(harness, session);
    assert_eq!(cache.len(), expected_windows);
    assert!(
        cache.iter().all(|(_, pane_count)| *pane_count == 1),
        "mode cache windows must each own exactly one pane: {cache:?}"
    );
}

fn cache_windows(harness: &FoundationHarness, session: &str) -> Vec<(String, usize)> {
    let raw = harness
        .tmux_capture(&[
            "list-windows",
            "-t",
            session,
            "-F",
            "#{window_id}|#{window_name}",
        ])
        .unwrap_or_else(|error| panic!("failed listing mode-cache windows: {error}"));
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (window_id, _) = line
                .split_once('|')
                .unwrap_or_else(|| panic!("invalid mode-cache window row: {line}"));
            let pane_count = harness
                .tmux_capture(&["list-panes", "-t", window_id, "-F", "#{pane_id}"])
                .unwrap_or_else(|error| panic!("failed listing panes for {window_id}: {error}"))
                .lines()
                .filter(|pane| !pane.trim().is_empty())
                .count();
            (window_id.to_owned(), pane_count)
        })
        .collect()
}
