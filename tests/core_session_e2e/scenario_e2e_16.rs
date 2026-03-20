use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, extract_stdout_field, map_settle, prepare_fresh_create_path,
    read_slot_snapshot, sample, settle_snapshot,
};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-16 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-16 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let before_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-16 failed reading pre-repair slot snapshot: {error}"));
    let slot_four_pane = before_slots
        .iter()
        .find(|slot| slot.slot_id == 4)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    harness
        .tmux_capture(&["kill-pane", "-t", &slot_four_pane])
        .unwrap_or_else(|error| panic!("E2E-16 failed injecting pane damage: {error}"));

    let pre_repair_graph = capture_pane_graph(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-16 failed reading pre-repair pane graph: {error}"));

    let repair = harness
        .run_ezm(&["repair"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-16 repair command failed to execute: {error}"));
    samples.push(sample(&["repair"], &repair));

    let post_repair_graph = capture_pane_graph(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-16 failed reading post-repair pane graph: {error}"));
    let after_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-16 failed reading post-repair slot snapshot: {error}"));

    let repair_action = extract_stdout_field(&repair.stdout, "action").unwrap_or_default();
    let missing_visible_slots =
        extract_stdout_field(&repair.stdout, "missing_visible_slots").unwrap_or_default();
    let recreated_slots =
        extract_stdout_field(&repair.stdout, "recreated_slots").unwrap_or_default();

    let unaffected_slots_preserved = [1_u8, 2, 3, 5].iter().all(|slot_id| {
        let before = before_slots.iter().find(|slot| slot.slot_id == *slot_id);
        let after = after_slots.iter().find(|slot| slot.slot_id == *slot_id);
        match (before, after) {
            (Some(before), Some(after)) => {
                before.pane_id == after.pane_id && before.worktree == after.worktree
            }
            _ => false,
        }
    });

    let pre_graph_rendered = pre_repair_graph.join(" || ");
    let post_graph_rendered = post_repair_graph.join(" || ");
    let repair_action_log = format!(
        "action={repair_action}; missing_visible_slots={missing_visible_slots}; recreated_slots={recreated_slots}"
    );

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!("pre-repair pane graph = {pre_graph_rendered}"));
    assertions.push(format!("post-repair pane graph = {post_graph_rendered}"));
    assertions.push(format!("repair action log = {repair_action_log}"));
    assertions.push(format!(
        "unaffected slot pane/worktree context preserved = {unaffected_slots_preserved}"
    ));

    let pre_graph_is_damaged = pre_repair_graph.len() == 4;
    let post_graph_is_restored = post_repair_graph.len() == 5;
    let action_reports_reconcile = repair_action == "reconcile";
    let missing_slot_reported = comma_list_contains_slot(&missing_visible_slots, 4);
    let recreated_slot_reported = comma_list_contains_slot(&recreated_slots, 4);

    let settle = settle_snapshot(harness, "E2E-16");
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && repair.exit_code == 0
        && pre_graph_is_damaged
        && post_graph_is_restored
        && action_reports_reconcile
        && missing_slot_reported
        && recreated_slot_reported
        && unaffected_slots_preserved
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-16"),
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
        slots: Some(after_slots),
        remote_path: None,
        helper_state: None,
    }
}

fn capture_pane_graph(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<Vec<String>, String> {
    let graph = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session_name}:0"),
        "-F",
        "#{pane_id}|#{pane_left}|#{pane_top}|#{pane_width}|#{pane_height}",
    ])?;
    Ok(graph
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn comma_list_contains_slot(value: &str, slot_id: u8) -> bool {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty() && *entry != "none")
        .any(|entry| entry == slot_id.to_string())
}
