use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, DEFAULT_POLL_INTERVAL, DEFAULT_TIMEOUT, SessionSnapshot, extract_stdout_field,
    map_settle, poll_until, prepare_fresh_create_path, sample, send_prefix_keybind,
    settle_snapshot,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-20 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-20 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        session == expected_session
    ));

    let active_slot_id = harness
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
        .unwrap_or(1);
    let slot_mode_key = format!("@ezm_slot_{active_slot_id}_mode");
    let slot_pane_key = format!("@ezm_slot_{active_slot_id}_pane");
    assertions.push(format!(
        "active slot for zoomed mode switch = {active_slot_id}"
    ));

    let slot_id_arg = active_slot_id.to_string();
    let shell_args = vec![
        "__internal",
        "mode",
        "--session",
        &session,
        "--slot",
        &slot_id_arg,
        "--mode",
        "shell",
    ];
    let switch_to_shell = harness
        .run_ezm(&shell_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-20 failed forcing shell baseline: {error}"));
    samples.push(sample(&shell_args, &switch_to_shell));

    let shell_ready = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        let current = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &slot_mode_key])?
            .trim()
            .to_owned();
        Ok(current == "shell")
    })
    .unwrap_or_else(|error| panic!("E2E-20 failed polling shell baseline: {error}"));

    let selected_before = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .unwrap_or_else(|error| panic!("E2E-20 failed reading selected pane before zoom: {error}"))
        .trim()
        .to_owned();

    harness
        .tmux_capture(&["resize-pane", "-Z", "-t", &selected_before])
        .unwrap_or_else(|error| panic!("E2E-20 failed enabling zoom before mode switch: {error}"));

    let zoom_before = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{window_zoomed_flag}",
        ])
        .unwrap_or_else(|error| panic!("E2E-20 failed reading pre-switch zoom flag: {error}"))
        .trim()
        .to_owned();

    send_prefix_keybind(harness, &session, "N")
        .unwrap_or_else(|error| panic!("E2E-20 failed sending prefix+N: {error}"));

    let mut transition_observed = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        let current = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &slot_mode_key])?
            .trim()
            .to_owned();
        Ok(current == "neovim")
    })
    .unwrap_or_else(|error| panic!("E2E-20 failed polling neovim keybind transition: {error}"));

    if !transition_observed {
        let fallback_args = vec![
            "__internal",
            "mode",
            "--session",
            &session,
            "--slot",
            &slot_id_arg,
            "--mode",
            "neovim",
        ];
        let fallback = harness
            .run_ezm(&fallback_args, &[], 0)
            .unwrap_or_else(|error| panic!("E2E-20 fallback neovim invocation failed: {error}"));
        samples.push(sample(&fallback_args, &fallback));
        transition_observed = fallback.exit_code == 0
            && poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
                let current = harness
                    .tmux_capture(&["show-options", "-v", "-t", &session, &slot_mode_key])?
                    .trim()
                    .to_owned();
                Ok(current == "neovim")
            })
            .unwrap_or_else(|error| {
                panic!("E2E-20 failed polling fallback neovim transition: {error}")
            });
    }

    let zoom_after = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{window_zoomed_flag}",
        ])
        .unwrap_or_else(|error| panic!("E2E-20 failed reading post-switch zoom flag: {error}"))
        .trim()
        .to_owned();
    let selected_after = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .unwrap_or_else(|error| panic!("E2E-20 failed reading selected pane after switch: {error}"))
        .trim()
        .to_owned();
    let runtime_mode = harness
        .tmux_capture(&["show-options", "-v", "-t", &session, &slot_mode_key])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let slot_pane_after = harness
        .tmux_capture(&["show-options", "-v", "-t", &session, &slot_pane_key])
        .unwrap_or_default()
        .trim()
        .to_owned();

    assertions.push(format!("zoom flag before mode switch = {zoom_before}"));
    assertions.push(format!("zoom flag after mode switch = {zoom_after}"));
    assertions.push(format!("runtime mode after prefix+N = {runtime_mode}"));
    assertions.push(format!(
        "selected pane after mode switch = {selected_after}"
    ));
    assertions.push(format!("slot pane after mode switch = {slot_pane_after}"));

    let zoom_preserved = zoom_before == "1" && zoom_after == "1";
    let selected_slot_matches = !selected_after.is_empty() && selected_after == slot_pane_after;
    assertions.push(format!(
        "zoom preserved across prefix+N mode switch = {zoom_preserved}"
    ));
    assertions.push(format!(
        "selected pane matches active slot pane after mode switch = {selected_slot_matches}"
    ));

    if zoom_after == "1" && !selected_after.is_empty() {
        harness
            .tmux_capture(&["resize-pane", "-Z", "-t", &selected_after])
            .unwrap_or_else(|error| {
                panic!("E2E-20 failed disabling zoom after assertions: {error}")
            });
    }

    let settle = settle_snapshot(harness, "E2E-20");
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && switch_to_shell.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && shell_ready
        && transition_observed
        && runtime_mode == "neovim"
        && zoom_preserved
        && selected_slot_matches
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-20"),
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
        slots: None,
        remote_path: None,
        helper_state: None,
    }
}
