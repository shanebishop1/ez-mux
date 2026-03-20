use std::fs;

use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, DEFAULT_POLL_INTERVAL, DEFAULT_TIMEOUT, SessionSnapshot, extract_stdout_field,
    map_settle, normalize_existing_path, paths_equivalent, poll_until, prepare_fresh_create_path,
    read_slot_snapshot, sample, settle_snapshot,
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

    let slot_id = 3_u8;
    let before_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-06 failed reading pre-mode slot snapshot: {error}"));
    let slot_pane = before_slots
        .iter()
        .find(|slot| slot.slot_id == slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();
    let slot_worktree = before_slots
        .iter()
        .find(|slot| slot.slot_id == slot_id)
        .map(|slot| slot.worktree.clone())
        .unwrap_or_default();

    let preserved_cwd = harness.work_dir().join("e2e06-preserved-cwd");
    fs::create_dir_all(&preserved_cwd)
        .unwrap_or_else(|error| panic!("E2E-06 failed creating cwd fixture: {error}"));
    let expected_preserved_cwd = normalize_existing_path(&preserved_cwd)
        .unwrap_or_else(|| preserved_cwd.display().to_string());
    let shell_launch = String::from("sh -lc 'exec \"${SHELL:-sh}\" -l'");
    harness
        .tmux_capture(&[
            "respawn-pane",
            "-k",
            "-t",
            &slot_pane,
            "-c",
            &expected_preserved_cwd,
            &shell_launch,
        ])
        .unwrap_or_else(|error| panic!("E2E-06 failed forcing slot cwd: {error}"));

    let pane_ready = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        let current = harness
            .tmux_capture(&[
                "display-message",
                "-p",
                "-t",
                &slot_pane,
                "#{pane_current_path}",
            ])?
            .trim()
            .to_owned();
        Ok(paths_equivalent(&current, &expected_preserved_cwd))
    })
    .unwrap_or_else(|error| panic!("E2E-06 failed polling pane cwd stabilization: {error}"));

    let captured_cwd = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &slot_pane,
            "#{pane_current_path}",
        ])
        .unwrap_or_else(|error| panic!("E2E-06 failed reading captured cwd: {error}"))
        .trim()
        .to_owned();
    let pre_transition_matches_fixture = paths_equivalent(&captured_cwd, &expected_preserved_cwd);
    let pre_transition_differs_from_slot_worktree = if slot_worktree.is_empty() {
        true
    } else {
        !paths_equivalent(&captured_cwd, &slot_worktree)
    };
    let non_default_cwd_confirmed = pane_ready
        && pre_transition_matches_fixture
        && (slot_worktree.is_empty() || pre_transition_differs_from_slot_worktree);

    assertions.push(format!(
        "custom fixture cwd target = {expected_preserved_cwd}"
    ));
    assertions.push(format!("captured cwd before transitions = {captured_cwd}"));
    assertions.push(format!("slot worktree = {slot_worktree}"));
    assertions.push(format!("pre-transition pane cwd settled = {pane_ready}"));
    assertions.push(format!(
        "pre-transition cwd matches custom fixture = {pre_transition_matches_fixture}"
    ));
    assertions.push(format!(
        "pre-transition cwd differs from slot worktree baseline = {}",
        if slot_worktree.is_empty() {
            String::from("n/a (slot worktree unavailable)")
        } else {
            pre_transition_differs_from_slot_worktree.to_string()
        }
    ));

    let transitions = ["neovim", "lazygit", "agent", "shell"];
    let mut transition_success = true;
    let mut mode_context_stable = true;
    let mut pane_identity_stable = true;

    for mode in transitions {
        let slot_id_text = slot_id.to_string();
        let args = vec![
            "__internal",
            "mode",
            "--session",
            &session,
            "--slot",
            &slot_id_text,
            "--mode",
            mode,
        ];
        let output = harness.run_ezm(&args, &[], 0).unwrap_or_else(|error| {
            panic!("E2E-06 transition to {mode} failed to execute: {error}")
        });
        samples.push(sample(&args, &output));

        if output.exit_code != 0 {
            transition_success = false;
        }

        let session_mode_key = format!("@ezm_slot_{slot_id}_mode");
        let session_cwd_key = format!("@ezm_slot_{slot_id}_cwd");
        let session_pane_key = format!("@ezm_slot_{slot_id}_pane");

        let runtime_mode = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &session_mode_key])
            .unwrap_or_else(|error| panic!("E2E-06 failed reading mode after {mode}: {error}"))
            .trim()
            .to_owned();
        let runtime_cwd = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &session_cwd_key])
            .unwrap_or_else(|error| panic!("E2E-06 failed reading cwd after {mode}: {error}"))
            .trim()
            .to_owned();
        let runtime_pane = harness
            .tmux_capture(&["show-options", "-v", "-t", &session, &session_pane_key])
            .unwrap_or_else(|error| {
                panic!("E2E-06 failed reading pane mapping after {mode}: {error}")
            })
            .trim()
            .to_owned();
        let pane_mode = harness
            .tmux_capture(&[
                "show-options",
                "-p",
                "-v",
                "-t",
                &slot_pane,
                "@ezm_slot_mode",
            ])
            .unwrap_or_else(|error| panic!("E2E-06 failed reading pane mode after {mode}: {error}"))
            .trim()
            .to_owned();
        let pane_cwd = harness
            .tmux_capture(&[
                "show-options",
                "-p",
                "-v",
                "-t",
                &slot_pane,
                "@ezm_slot_cwd",
            ])
            .unwrap_or_else(|error| panic!("E2E-06 failed reading pane cwd after {mode}: {error}"))
            .trim()
            .to_owned();
        let pane_current = harness
            .tmux_capture(&[
                "display-message",
                "-p",
                "-t",
                &slot_pane,
                "#{pane_current_path}",
            ])
            .unwrap_or_else(|error| {
                panic!("E2E-06 failed reading pane current path after {mode}: {error}")
            })
            .trim()
            .to_owned();

        let session_cwd_preserved = paths_equivalent(&runtime_cwd, &expected_preserved_cwd);
        let pane_cwd_preserved = paths_equivalent(&pane_cwd, &expected_preserved_cwd);
        let pane_current_preserved = paths_equivalent(&pane_current, &expected_preserved_cwd);
        let transition_cwd_preserved =
            session_cwd_preserved && pane_cwd_preserved && pane_current_preserved;

        if runtime_mode != mode || pane_mode != mode {
            mode_context_stable = false;
        }
        if !transition_cwd_preserved {
            mode_context_stable = false;
        }
        if runtime_pane != slot_pane {
            pane_identity_stable = false;
        }

        assertions.push(format!(
            "mode transition `{mode}` exit_code={} runtime_mode={runtime_mode} pane_mode={pane_mode}",
            output.exit_code
        ));
        assertions.push(format!(
            "mode transition `{mode}` cwd session={runtime_cwd} pane={pane_cwd} current={pane_current} expected={expected_preserved_cwd} preserved_non_default={transition_cwd_preserved}"
        ));
        assertions.push(format!(
            "mode transition `{mode}` pane identity preserved = {}",
            runtime_pane == slot_pane
        ));
    }

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
        && transition_success
        && non_default_cwd_confirmed
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
