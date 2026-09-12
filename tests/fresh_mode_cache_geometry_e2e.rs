#![allow(dead_code)]

mod support;

#[path = "core_session_e2e/core_support.rs"]
mod core_support;

use std::fs;

use core_support::{
    extract_stdout_field, pane_graph_stable, prepare_fresh_create_path, read_pane_graph,
    settle_snapshot,
};
use support::foundation_harness::FoundationHarness;

#[allow(clippy::too_many_lines)]
#[test]
fn fresh_mode_cache_allocations_do_not_consume_visible_geometry() {
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

    for slot_id in 2_u8..=5 {
        let switch = switch_mode(&harness, &project_dir, &session, slot_id, "agent");
        assert_eq!(
            switch.exit_code, 0,
            "slot {slot_id} agent switch failed: {}",
            switch.stderr
        );
    }

    let cache_session = format!("{session}__mode_cache");
    let cache_after_agents = cache_windows(&harness, &cache_session);
    assert_eq!(
        cache_after_agents
            .iter()
            .map(|(_, pane_count)| pane_count)
            .sum::<usize>(),
        4
    );

    let neovim = switch_mode(&harness, &project_dir, &session, 3, "neovim");
    assert_eq!(
        neovim.exit_code, 0,
        "slot 3 neovim switch failed: {}",
        neovim.stderr
    );
    let neovim_pane = session_option(&harness, &session, "@ezm_slot_3_pane");
    let neovim_backing_key = "@ezm_slot_3_backing_neovim_pane";

    let lazygit = switch_mode(&harness, &project_dir, &session, 3, "lazygit");
    assert_eq!(
        lazygit.exit_code, 0,
        "fresh slot 3 lazygit allocation failed: {}",
        lazygit.stderr
    );
    let lazygit_pane = session_option(&harness, &session, "@ezm_slot_3_pane");
    assert_ne!(lazygit_pane, neovim_pane);
    assert_eq!(
        session_option(&harness, &session, neovim_backing_key),
        neovim_pane
    );
    assert_eq!(
        session_option(&harness, &session, "@ezm_slot_3_mode"),
        "lazygit"
    );

    let neovim_again = switch_mode(&harness, &project_dir, &session, 3, "neovim");
    assert_eq!(
        neovim_again.exit_code, 0,
        "slot 3 neovim restore failed: {}",
        neovim_again.stderr
    );
    assert_eq!(
        session_option(&harness, &session, "@ezm_slot_3_pane"),
        neovim_pane
    );
    assert_eq!(
        session_option(&harness, &session, "@ezm_slot_3_mode"),
        "neovim"
    );

    let visible_after = read_pane_graph(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading final visible geometry: {error}"));
    assert!(pane_graph_stable(&visible_before, &visible_after));

    let cache_after_modes = cache_windows(&harness, &cache_session);
    assert_eq!(cache_after_modes.len(), 6);
    assert!(
        cache_after_modes
            .iter()
            .all(|(_, pane_count)| *pane_count == 1)
    );

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
        teardown.stdout.contains("helper_processes_removed=6"),
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
