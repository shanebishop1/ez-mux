use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, DEFAULT_POLL_INTERVAL, DEFAULT_TIMEOUT, SessionSnapshot, extract_stdout_field,
    map_settle, poll_until, prepare_fresh_create_path, read_slot_snapshot, sample, settle_snapshot,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-06 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-06 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        session == expected_session
    ));

    let before_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-06 failed reading pre-mode slot snapshot: {error}"));

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
        .unwrap_or(2);
    let slot_mode_key = format!("@ezm_slot_{active_slot_id}_mode");
    let slot_cwd_key = format!("@ezm_slot_{active_slot_id}_cwd");
    let slot_pane_key = format!("@ezm_slot_{active_slot_id}_pane");

    let baseline_cwd = harness
        .tmux_capture(&["show-options", "-v", "-t", &session, &slot_cwd_key])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let baseline_pane = harness
        .tmux_capture(&["show-options", "-v", "-t", &session, &slot_pane_key])
        .unwrap_or_default()
        .trim()
        .to_owned();
    assertions.push(format!(
        "active slot for keybind transitions = {active_slot_id}"
    ));
    assertions.push(format!("baseline slot cwd = {baseline_cwd}"));
    assertions.push(format!("baseline slot pane = {baseline_pane}"));

    let mode_keybind_matrix = [
        ("u", "prefix", "u", "__internal mode"),
        ("a", "prefix", "a", "--mode agent"),
        ("S", "prefix", "S", "--mode shell"),
        ("N", "prefix", "N", "--mode neovim"),
        ("G", "prefix", "G", "--mode lazygit"),
    ];
    let keybind_matrix_present = mode_keybind_matrix.iter().all(|(_, table, key, marker)| {
        harness
            .tmux_capture(&["list-keys", "-T", table, key])
            .unwrap_or_default()
            .contains(marker)
    });
    assertions.push(format!(
        "mode keybind matrix present for u/a/S/N/G = {keybind_matrix_present}"
    ));

    let transitions = [
        ("N", "neovim"),
        ("G", "lazygit"),
        ("a", "agent"),
        ("S", "shell"),
    ];
    let mut transition_success = true;
    let mut mode_context_stable = true;
    let mut pane_identity_stable = true;

    for (key, mode) in transitions {
        harness
            .tmux_capture(&["send-keys", "-t", &format!("{session}:0"), "C-b", key])
            .unwrap_or_else(|error| panic!("E2E-06 failed sending keybind for {mode}: {error}"));

        let transition_observed = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
            let current = harness
                .tmux_capture(&["show-options", "-v", "-t", &session, &slot_mode_key])?
                .trim()
                .to_owned();
            Ok(current == mode)
        })
        .unwrap_or_else(|error| panic!("E2E-06 failed polling mode transition {mode}: {error}"));

        if !transition_observed {
            transition_success = false;
        }

        let runtime_mode = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &slot_mode_key])
            .unwrap_or_default()
            .trim()
            .to_owned();
        let runtime_cwd = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &slot_cwd_key])
            .unwrap_or_default()
            .trim()
            .to_owned();
        let runtime_pane = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &slot_pane_key])
            .unwrap_or_default()
            .trim()
            .to_owned();

        if runtime_mode != mode {
            mode_context_stable = false;
        }
        if runtime_cwd != baseline_cwd {
            mode_context_stable = false;
        }
        if runtime_pane != baseline_pane {
            pane_identity_stable = false;
        }

        assertions.push(format!(
            "mode transition `{mode}` key={key} observed={transition_observed} runtime_mode={runtime_mode}"
        ));
        assertions.push(format!(
            "mode transition `{mode}` cwd preserved = {}",
            runtime_cwd == baseline_cwd
        ));
        assertions.push(format!(
            "mode transition `{mode}` pane identity preserved = {}",
            runtime_pane == baseline_pane
        ));
    }

    harness
        .tmux_capture(&["send-keys", "-t", &format!("{session}:0"), "C-b", "u"])
        .unwrap_or_else(|error| panic!("E2E-06 failed sending first toggle key: {error}"));
    let toggle_to_agent = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        let current = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &slot_mode_key])?
            .trim()
            .to_owned();
        Ok(current == "agent")
    })
    .unwrap_or_else(|error| panic!("E2E-06 failed polling toggle to agent: {error}"));

    harness
        .tmux_capture(&["send-keys", "-t", &format!("{session}:0"), "C-b", "u"])
        .unwrap_or_else(|error| panic!("E2E-06 failed sending second toggle key: {error}"));
    let toggle_back_to_shell = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        let current = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &slot_mode_key])?
            .trim()
            .to_owned();
        Ok(current == "shell")
    })
    .unwrap_or_else(|error| panic!("E2E-06 failed polling toggle back to shell: {error}"));
    assertions.push(format!(
        "toggle key u shell->agent->shell observed = {}",
        toggle_to_agent && toggle_back_to_shell
    ));

    let invalid_slot_args = vec![
        "__internal",
        "mode",
        "--session",
        &session,
        "--slot",
        "9",
        "--mode",
        "shell",
    ];
    let invalid_slot = harness
        .run_ezm(&invalid_slot_args, &[], 0)
        .unwrap_or_else(|error| {
            panic!("E2E-06 invalid slot transition failed to execute: {error}")
        });
    samples.push(sample(&invalid_slot_args, &invalid_slot));

    let invalid_slot_failed = invalid_slot.exit_code != 0;
    let invalid_slot_diagnostic = invalid_slot.stderr.contains("outside canonical range 1..5");
    assertions.push(format!(
        "invalid slot mode transition rejected = {invalid_slot_failed}"
    ));
    assertions.push(format!(
        "invalid slot diagnostic surfaced = {invalid_slot_diagnostic}"
    ));

    let settle = settle_snapshot(harness, "E2E-06");

    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && keybind_matrix_present
        && transition_success
        && toggle_to_agent
        && toggle_back_to_shell
        && mode_context_stable
        && pane_identity_stable
        && invalid_slot_failed
        && invalid_slot_diagnostic
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-06"),
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
        slots: Some(before_slots),
        remote_path: None,
        helper_state: None,
    }
}
