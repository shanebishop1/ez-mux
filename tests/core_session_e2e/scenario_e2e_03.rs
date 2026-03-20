use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, create_worktree_fixture, expected_worktree_cycle,
    extract_stdout_field, map_settle, prepare_fresh_create_path, read_slot_snapshot, sample,
    settle_snapshot, slot_snapshots_match,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let fixture = create_worktree_fixture(harness)
        .unwrap_or_else(|error| panic!("E2E-03 fixture setup failed: {error}"));
    assertions.push(format!(
        "fixture project = {}",
        fixture.canonical_project_dir.display()
    ));
    assertions.push(format!(
        "fixture extra worktrees = [{}]",
        fixture
            .extra_worktrees
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let expected_session = prepare_fresh_create_path(harness, &fixture.project_dir)
        .unwrap_or_else(|error| panic!("E2E-03 setup failed: {error}"));

    let first = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-03 first launch failed: {error}"));
    let second = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-03 second launch failed: {error}"));

    samples.push(sample(&[], &first));
    samples.push(sample(&[], &second));

    let first_action = extract_stdout_field(&first.stdout, "session_action").unwrap_or_default();
    let second_action = extract_stdout_field(&second.stdout, "session_action").unwrap_or_default();
    let first_session = extract_stdout_field(&first.stdout, "session").unwrap_or_default();
    let second_session = extract_stdout_field(&second.stdout, "session").unwrap_or_default();
    assertions.push(format!("first action = {first_action}"));
    assertions.push(format!("second action = {second_action}"));
    assertions.push(format!("first session = {first_session}"));
    assertions.push(format!("second session = {second_session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        second_session == expected_session
    ));

    let settle = settle_snapshot(harness, "E2E-03");

    let first_slots = read_slot_snapshot(harness, &first_session)
        .unwrap_or_else(|error| panic!("E2E-03 failed reading first slot snapshot: {error}"));
    let second_slots = read_slot_snapshot(harness, &second_session)
        .unwrap_or_else(|error| panic!("E2E-03 failed reading second slot snapshot: {error}"));

    assertions.push(format!(
        "canonical slot count first run = {}",
        first_slots.len()
    ));
    assertions.push(format!(
        "canonical slot count second run = {}",
        second_slots.len()
    ));
    assertions.push(format!(
        "slot mapping stable across relaunch = {}",
        slot_snapshots_match(&first_slots, &second_slots)
    ));

    let pane_count = first_slots
        .iter()
        .map(|slot| slot.pane_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let canonical_ids = first_slots
        .iter()
        .map(|slot| slot.slot_id)
        .collect::<Vec<_>>();
    let distinct_worktrees = first_slots
        .iter()
        .map(|slot| slot.worktree.clone())
        .collect::<std::collections::HashSet<_>>()
        .len();

    assertions.push(format!("unique pane ids in slot map = {pane_count}"));
    assertions.push(format!("canonical slot ids = {canonical_ids:?}"));
    assertions.push(format!(
        "distinct worktrees assigned on create path = {distinct_worktrees}"
    ));

    let expected_worktree_cycle = expected_worktree_cycle(&fixture);
    let deterministic_assignment_verified =
        expected_worktree_cycle
            .iter()
            .all(|(slot_id, expected_worktree)| {
                first_slots
                    .iter()
                    .find(|slot| slot.slot_id == *slot_id)
                    .is_some_and(|slot| slot.worktree == *expected_worktree)
            });
    assertions.push(format!(
        "deterministic slot/worktree cycle matches expected fixture mapping = {deterministic_assignment_verified}"
    ));

    let remap_target = second_slots
        .iter()
        .find(|slot| slot.slot_id == 2)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();
    assertions.push(format!(
        "remap target uses existing slot-2 pane = {remap_target}"
    ));

    let remap_attempt = harness.tmux_capture(&[
        "set-option",
        "-t",
        &second_session,
        "@ezm_slot_1_pane",
        &remap_target,
    ]);
    let remap_applied = remap_attempt.is_ok();
    assertions.push(format!(
        "direct remap overwrite accepted by tmux = {remap_applied}"
    ));

    let post_attempt_slots = read_slot_snapshot(harness, &second_session)
        .unwrap_or_else(|error| panic!("E2E-03 failed reading post-remap slots: {error}"));
    let slot1_overwritten = post_attempt_slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .is_some_and(|slot| slot.pane_id == remap_target);
    assertions.push(format!(
        "slot-1 option overwritten by remap attempt = {slot1_overwritten}"
    ));

    let third = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-03 remap validation launch failed: {error}"));
    samples.push(sample(&[], &third));

    let third_failed = third.exit_code != 0;
    let remap_diagnostic_reported = third.stderr.contains("canonical slot identity mismatch")
        || third.stdout.contains("canonical slot identity mismatch");
    assertions.push(format!(
        "remapped session rejected on relaunch = {third_failed}"
    ));
    assertions.push(format!(
        "canonical mismatch diagnostics surfaced = {remap_diagnostic_reported}"
    ));

    let session_exists = !second_session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = first.exit_code == 0
        && second.exit_code == 0
        && first_action == "create"
        && second_action == "attach"
        && !first_session.is_empty()
        && first_session == second_session
        && second_session == expected_session
        && settle.stable
        && first_slots.len() == 5
        && second_slots.len() == 5
        && pane_count == 5
        && slot_snapshots_match(&first_slots, &second_slots)
        && distinct_worktrees >= 3
        && deterministic_assignment_verified
        && remap_applied
        && slot1_overwritten
        && third_failed
        && remap_diagnostic_reported;

    CaseEvidence {
        id: String::from("E2E-03"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: second_session,
            exists: session_exists,
            count: session_count,
        },
        layout: None,
        slots: Some(first_slots),
        remote_path: None,
        helper_state: None,
    }
}
