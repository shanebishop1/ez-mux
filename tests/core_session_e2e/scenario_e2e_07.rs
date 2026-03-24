use std::fs;

use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, DEFAULT_POLL_INTERVAL, DEFAULT_TIMEOUT, SessionSnapshot, extract_stdout_field,
    map_settle, normalize_existing_path, paths_equivalent, poll_until, prepare_fresh_create_path,
    read_slot_snapshot, sample, send_prefix_keybind, settle_snapshot,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-07 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-07 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-07 failed reading slot snapshot: {error}"));
    let slot_pane = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let slot_id = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{@ezm_slot_id}",
        ])
        .ok()
        .and_then(|raw| raw.trim().parse::<u8>().ok())
        .filter(|slot| (1..=5).contains(slot))
        .unwrap_or(2);

    let popup_cwd_path = harness.work_dir().join("e2e07-popup-cwd");
    fs::create_dir_all(&popup_cwd_path)
        .unwrap_or_else(|error| panic!("E2E-07 failed creating popup cwd fixture: {error}"));
    let popup_cwd = normalize_existing_path(&popup_cwd_path)
        .unwrap_or_else(|| popup_cwd_path.display().to_string());
    let slot_cwd_key = format!("@ezm_slot_{slot_id}_cwd");
    harness
        .tmux_capture(&["set-option", "-t", &session, &slot_cwd_key, &popup_cwd])
        .unwrap_or_else(|error| panic!("E2E-07 failed setting slot cwd fixture: {error}"));

    let popup_keybind = harness
        .tmux_capture(&["list-keys", "-T", "prefix", "P"])
        .unwrap_or_default();
    let popup_keybind_present = popup_keybind.contains("__internal popup");
    assertions.push(format!(
        "popup keybind prefix+P routes to internal popup runtime = {popup_keybind_present}"
    ));

    send_prefix_keybind(harness, &session, "P")
        .unwrap_or_else(|error| panic!("E2E-07 failed sending popup open keybind: {error}"));

    let popup_session = format!("{session}__popup_slot_{slot_id}");
    let mut popup_exists_after_open = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        Ok(harness
            .tmux_capture(&["has-session", "-t", &popup_session])
            .is_ok())
    })
    .unwrap_or_else(|error| panic!("E2E-07 failed polling popup open state: {error}"));

    if !popup_exists_after_open {
        let slot_id_arg = slot_id.to_string();
        let fallback_open_args = vec![
            "__internal",
            "popup",
            "--session",
            &session,
            "--slot",
            &slot_id_arg,
        ];
        let fallback_open = harness
            .run_ezm(&fallback_open_args, &[], 0)
            .unwrap_or_else(|error| panic!("E2E-07 fallback popup open failed: {error}"));
        samples.push(sample(&fallback_open_args, &fallback_open));
        popup_exists_after_open = fallback_open.exit_code == 0
            && poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
                Ok(harness
                    .tmux_capture(&["has-session", "-t", &popup_session])
                    .is_ok())
            })
            .unwrap_or_else(|error| {
                panic!("E2E-07 failed polling fallback popup open state: {error}")
            });
    }
    let popup_recorded_cwd = harness
        .tmux_capture(&["show-options", "-v", "-t", &popup_session, "@ezm_popup_cwd"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let popup_pane_cwd = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{popup_session}:0.0"),
            "#{pane_current_path}",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let popup_pane_pid_after_open = harness
        .tmux_capture(&["list-panes", "-t", &popup_session, "-F", "#{pane_pid}"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let popup_visible_after_open = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        Ok(harness
            .tmux_capture(&[
                "display-message",
                "-p",
                "-t",
                &format!("{session}:0"),
                "#{popup_active}",
            ])
            .map(|value| value.trim() == "1")
            .unwrap_or(false))
    })
    .unwrap_or_else(|error| panic!("E2E-07 failed polling popup visibility state: {error}"));
    let popup_width = harness
        .tmux_capture(&["show-options", "-v", "-t", &session, "@ezm_popup_width_pct"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let popup_height = harness
        .tmux_capture(&[
            "show-options",
            "-v",
            "-t",
            &session,
            "@ezm_popup_height_pct",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();

    send_prefix_keybind(harness, &session, "P")
        .unwrap_or_else(|error| panic!("E2E-07 failed sending popup close keybind: {error}"));

    let popup_not_visible_after_close = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        Ok(harness
            .tmux_capture(&[
                "display-message",
                "-p",
                "-t",
                &format!("{session}:0"),
                "#{popup_active}",
            ])
            .map(|value| value.trim() != "1")
            .unwrap_or(true))
    })
    .unwrap_or_else(|error| panic!("E2E-07 failed polling popup close visibility state: {error}"));
    let popup_exists_after_close = harness
        .tmux_capture(&["has-session", "-t", &popup_session])
        .is_ok();
    let selected_after_close = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();

    send_prefix_keybind(harness, &session, "P")
        .unwrap_or_else(|error| panic!("E2E-07 failed sending popup reopen keybind: {error}"));
    let popup_exists_before_parent_kill =
        poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
            Ok(harness
                .tmux_capture(&["has-session", "-t", &popup_session])
                .is_ok())
        })
        .unwrap_or_else(|error| panic!("E2E-07 failed polling popup reopen state: {error}"));
    let popup_visible_after_reopen = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        Ok(harness
            .tmux_capture(&[
                "display-message",
                "-p",
                "-t",
                &format!("{session}:0"),
                "#{popup_active}",
            ])
            .map(|value| value.trim() == "1")
            .unwrap_or(false))
    })
    .unwrap_or_else(|error| panic!("E2E-07 failed polling popup reopen visibility: {error}"));
    let popup_pane_pid_after_reopen = harness
        .tmux_capture(&["list-panes", "-t", &popup_session, "-F", "#{pane_pid}"])
        .unwrap_or_default()
        .trim()
        .to_owned();

    harness
        .tmux_capture(&["kill-session", "-t", &session])
        .unwrap_or_else(|error| panic!("E2E-07 failed killing parent session: {error}"));

    let popup_removed_after_parent_kill =
        poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
            Ok(harness
                .tmux_capture(&["has-session", "-t", &popup_session])
                .is_err())
        })
        .unwrap_or_else(|error| {
            panic!("E2E-07 failed polling popup cleanup after parent kill: {error}")
        });

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "popup helper session exists after open = {popup_exists_after_open}"
    ));
    assertions.push(format!(
        "popup visibly opens after open = {popup_visible_after_open}"
    ));
    assertions.push(format!(
        "popup helper session persists after close = {popup_exists_after_close}"
    ));
    assertions.push(format!(
        "popup visibly closes after close = {popup_not_visible_after_close}"
    ));
    assertions.push(format!(
        "popup recorded cwd matches slot cwd fixture = {}",
        paths_equivalent(&popup_recorded_cwd, &popup_cwd)
    ));
    assertions.push(format!(
        "popup pane cwd matches slot cwd fixture (best effort) = {}",
        paths_equivalent(&popup_pane_cwd, &popup_cwd)
    ));
    assertions.push(format!("popup width pct = {popup_width}"));
    assertions.push(format!("popup height pct = {popup_height}"));
    assertions.push(format!(
        "focus returns to originating pane after close = {}",
        selected_after_close == slot_pane
    ));
    assertions.push(format!(
        "popup helper session exists before parent kill = {popup_exists_before_parent_kill}"
    ));
    assertions.push(format!(
        "popup visibly opens again after reopen = {popup_visible_after_reopen}"
    ));
    assertions.push(format!(
        "popup pane pid is stable across close/reopen = {}",
        !popup_pane_pid_after_open.is_empty()
            && popup_pane_pid_after_open == popup_pane_pid_after_reopen
    ));
    assertions.push(format!(
        "popup helper session removed after parent kill = {popup_removed_after_parent_kill}"
    ));

    let settle = settle_snapshot(harness, "E2E-07");

    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && popup_keybind_present
        && popup_exists_after_open
        && popup_exists_after_close
        && popup_not_visible_after_close
        && !popup_pane_pid_after_open.is_empty()
        && popup_width == "70"
        && popup_height == "70"
        && selected_after_close == slot_pane
        && popup_exists_before_parent_kill
        && popup_pane_pid_after_open == popup_pane_pid_after_reopen
        && popup_removed_after_parent_kill
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-07"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: session,
            exists: session_exists,
            count: session_count,
        },
        layout: None,
        slots: Some(slots),
        remote_path: None,
        helper_state: None,
    }
}
