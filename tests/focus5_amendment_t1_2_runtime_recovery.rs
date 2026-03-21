#[path = "support/focus5_amendment_t1_1_red_support.rs"]
mod red_support;
mod support;

use red_support::{
    center_pane_id, create_worktree_fixture, extract_stdout_field, paths_equivalent,
    read_pane_widths, read_slot_snapshot, write_cluster_evidence,
};
use support::foundation_harness::FoundationHarness;

struct AttachProbeOutcome {
    probe_exit: i32,
    observed_attached_client: bool,
    attempts: Vec<(usize, i32, bool)>,
}

#[test]
fn t1_2_startup_attach_visibility_reports_non_interactive_and_observes_interactive_attach() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-2")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let create = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup create launch failed: {error}"));
    let create_action = extract_stdout_field(&create.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&create.stdout, "session").unwrap_or_default();
    let create_attach_visibility =
        extract_stdout_field(&create.stdout, "attach_visibility").unwrap_or_default();

    let pty_probe = run_attach_probe_with_retries(&harness, &session, 5)
        .unwrap_or_else(|error| panic!("interactive attach probe failed: {error}"));

    let attach = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup attach launch failed: {error}"));
    let attach_action = extract_stdout_field(&attach.stdout, "session_action").unwrap_or_default();
    let attach_session = extract_stdout_field(&attach.stdout, "session").unwrap_or_default();
    let attach_visibility =
        extract_stdout_field(&attach.stdout, "attach_visibility").unwrap_or_default();

    let mut evidence = vec![
        format!("create_exit_code={}", create.exit_code),
        format!("create_action={create_action}"),
        format!("create_session={session}"),
        format!("create_attach_visibility={create_attach_visibility}"),
        format!("pty_probe_exit={}", pty_probe.probe_exit),
        format!("pty_attach_observed={}", pty_probe.observed_attached_client),
        format!("pty_probe_attempts={}", pty_probe.attempts.len()),
        format!("attach_exit_code={}", attach.exit_code),
        format!("attach_action={attach_action}"),
        format!("attach_session={attach_session}"),
        format!("attach_visibility={attach_visibility}"),
    ];
    for (attempt, exit_code, observed) in &pty_probe.attempts {
        evidence.push(format!(
            "pty_probe_attempt_{attempt}_exit={exit_code} observed={observed}"
        ));
    }
    let pty_probe_attempted = !pty_probe.attempts.is_empty();
    evidence.push(format!("pty_probe_attempted={pty_probe_attempted}"));
    write_cluster_evidence(&harness, "t1-2-startup-attach-visibility", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.2 evidence: {error}"));

    let pass = create.exit_code == 0
        && create_action == "create"
        && !session.is_empty()
        && create_attach_visibility == "non-interactive"
        && pty_probe_attempted
        && pty_probe.observed_attached_client
        && attach.exit_code == 0
        && attach_action == "attach"
        && attach_session == session
        && attach_visibility == "non-interactive";

    assert!(
        pass,
        "T-1.2 startup attach visibility contract failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn t1_2_startup_populated_slots_launch_agent_and_unpopulated_slots_fallback_shell() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-2")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let fixture = create_worktree_fixture(&harness)
        .unwrap_or_else(|error| panic!("fixture setup failed: {error}"));
    let launch = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("fixture startup launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot: {error}"));
    let pane_widths = read_pane_widths(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading pane widths: {error}"));
    let center_pane = center_pane_id(&pane_widths).unwrap_or_default();
    let center_slot = slots
        .iter()
        .find(|slot| slot.pane_id == center_pane)
        .map(|slot| slot.slot_id);

    let slot1_worktree = slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .map(|slot| slot.worktree.clone())
        .unwrap_or_default();
    let slot1_matches_expected = paths_equivalent(
        &slot1_worktree,
        &fixture.expected_slot1_worktree.display().to_string(),
    );

    let mut per_slot = Vec::new();
    for slot_id in 1_u8..=5 {
        let slot = slots
            .iter()
            .find(|slot| slot.slot_id == slot_id)
            .unwrap_or_else(|| panic!("missing slot snapshot for slot {slot_id}"));
        let mode = read_slot_mode(&harness, &session, slot_id)
            .unwrap_or_else(|error| panic!("failed reading slot mode for slot {slot_id}: {error}"));
        let pane_start_command =
            read_pane_start_command(&harness, &slot.pane_id).unwrap_or_else(|error| {
                panic!(
                    "failed reading pane_start_command for slot {slot_id} pane {}: {error}",
                    slot.pane_id
                )
            });
        let should_be_agent = slot_id <= 3;
        let mode_matches = if should_be_agent {
            mode == "agent"
        } else {
            mode == "shell"
        };
        let command_matches = if should_be_agent {
            pane_start_command.contains("opencode")
        } else {
            !pane_start_command.contains("opencode")
        };

        per_slot.push((
            slot_id,
            mode,
            pane_start_command,
            mode_matches,
            command_matches,
        ));
    }

    let no_negative_worktrees = slots.iter().all(|slot| slot.worktree != "-1");
    let startup_slot_modes_correct = per_slot
        .iter()
        .all(|(_, _, _, mode_matches, _)| *mode_matches);
    let startup_commands_correct = per_slot
        .iter()
        .all(|(_, _, _, _, command_matches)| *command_matches);

    let mut evidence = vec![
        format!("exit_code={} session={session}", launch.exit_code),
        format!("center_pane={center_pane}"),
        format!("center_slot={center_slot:?}"),
        format!("slot1_worktree={slot1_worktree}"),
        format!(
            "expected_slot1_worktree={}",
            fixture.expected_slot1_worktree.display()
        ),
        format!("slot1_matches_expected={slot1_matches_expected}"),
        format!("no_negative_worktrees={no_negative_worktrees}"),
        format!("startup_slot_modes_correct={startup_slot_modes_correct}"),
        format!("startup_commands_correct={startup_commands_correct}"),
    ];

    for (slot_id, mode, command, mode_matches, command_matches) in &per_slot {
        evidence.push(format!("slot{slot_id}_mode={mode}"));
        evidence.push(format!("slot{slot_id}_pane_start_command={command}"));
        evidence.push(format!("slot{slot_id}_mode_matches={mode_matches}"));
        evidence.push(format!("slot{slot_id}_command_matches={command_matches}"));
    }
    write_cluster_evidence(&harness, "t1-2-startup-mode-launch", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.2 evidence: {error}"));

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && center_slot == Some(1)
        && slot1_matches_expected
        && no_negative_worktrees
        && startup_slot_modes_correct
        && startup_commands_correct;

    assert!(
        pass,
        "T-1.2 startup mode launch contract failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn t1_2_startup_worktree_assignment_stays_deterministic_across_restart_without_negative_slots() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-2")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let fixture = create_worktree_fixture(&harness)
        .unwrap_or_else(|error| panic!("fixture setup failed: {error}"));

    let first = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("first launch failed: {error}"));
    let second = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("second launch failed: {error}"));

    let first_action = extract_stdout_field(&first.stdout, "session_action").unwrap_or_default();
    let second_action = extract_stdout_field(&second.stdout, "session_action").unwrap_or_default();
    let first_session = extract_stdout_field(&first.stdout, "session").unwrap_or_default();
    let second_session = extract_stdout_field(&second.stdout, "session").unwrap_or_default();

    let first_slots = read_slot_snapshot(&harness, &first_session)
        .unwrap_or_else(|error| panic!("failed reading first slot snapshot: {error}"));
    let second_slots = read_slot_snapshot(&harness, &second_session)
        .unwrap_or_else(|error| panic!("failed reading second slot snapshot: {error}"));

    let no_negative_worktrees = first_slots
        .iter()
        .chain(second_slots.iter())
        .all(|slot| slot.worktree != "-1");
    let worktree_mapping_stable = slot_worktree_mapping_stable(&first_slots, &second_slots);

    let evidence = vec![
        format!("first_exit_code={}", first.exit_code),
        format!("second_exit_code={}", second.exit_code),
        format!("first_action={first_action}"),
        format!("second_action={second_action}"),
        format!("first_session={first_session}"),
        format!("second_session={second_session}"),
        format!("worktree_mapping_stable={worktree_mapping_stable}"),
        format!("no_negative_worktrees={no_negative_worktrees}"),
    ];
    write_cluster_evidence(&harness, "t1-2-worktree-determinism", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.2 evidence: {error}"));

    let pass = first.exit_code == 0
        && second.exit_code == 0
        && first_action == "create"
        && second_action == "attach"
        && !first_session.is_empty()
        && first_session == second_session
        && worktree_mapping_stable
        && no_negative_worktrees;

    assert!(
        pass,
        "T-1.2 startup worktree determinism contract failed:\n{}",
        evidence.join("\n")
    );
}

fn read_slot_mode(
    harness: &FoundationHarness,
    session: &str,
    slot_id: u8,
) -> Result<String, String> {
    harness
        .tmux_capture(&[
            "show-options",
            "-v",
            "-t",
            session,
            &format!("@ezm_slot_{slot_id}_mode"),
        ])
        .map(|value| value.trim().to_owned())
}

fn read_pane_start_command(harness: &FoundationHarness, pane_id: &str) -> Result<String, String> {
    harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{pane_start_command}",
        ])
        .map(|value| value.trim().to_owned())
}

fn slot_worktree_mapping_stable(
    left: &[red_support::SlotSnapshot],
    right: &[red_support::SlotSnapshot],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right.iter()).all(|(lhs, rhs)| {
            lhs.slot_id == rhs.slot_id && paths_equivalent(&lhs.worktree, &rhs.worktree)
        })
}

fn run_attach_probe_with_retries(
    harness: &FoundationHarness,
    session: &str,
    max_attempts: usize,
) -> Result<AttachProbeOutcome, String> {
    let attempts = max_attempts.max(1);
    let mut history = Vec::with_capacity(attempts);
    let mut final_exit_code = -1;
    let mut observed_attached_client = false;

    for attempt in 1..=attempts {
        let probe = harness
            .run_ezm_with_pty_attach_probe(harness.project_root(), &[], &[], 0, session)
            .map_err(|error| {
                format!(
                    "attempt {attempt}/{attempts} failed while probing interactive attach: {error}"
                )
            })?;

        final_exit_code = probe.exit_code;
        observed_attached_client = probe.observed_attached_client;
        history.push((attempt, probe.exit_code, probe.observed_attached_client));

        if observed_attached_client {
            break;
        }
    }

    Ok(AttachProbeOutcome {
        probe_exit: final_exit_code,
        observed_attached_client,
        attempts: history,
    })
}
