use std::collections::BTreeSet;

use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, extract_stdout_field, map_settle, prepare_fresh_create_path,
    read_slot_snapshot, sample, settle_snapshot, slot_snapshots_match,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();
    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-21 setup failed: {error}"));
    let launch_args = ["--verbose", "--panes", "1"];
    let launch = harness
        .run_ezm(&launch_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-21 launch failed: {error}"));
    samples.push(sample(&launch_args, &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let baseline_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-21 failed reading baseline slot metadata: {error}"));
    let stale_slot_one_pane = baseline_slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    harness
        .tmux_capture(&[
            "new-window",
            "-d",
            "-t",
            &session,
            "-n",
            "e2e21-unrelated",
            "sleep",
            "3600",
        ])
        .unwrap_or_else(|error| panic!("E2E-21 failed creating unrelated window: {error}"));
    let unrelated_window = find_window_by_name(harness, &session, "e2e21-unrelated")
        .unwrap_or_else(|| panic!("E2E-21 unrelated window was not created"));
    let canonical_before = session_option(harness, &session, "@ezm_canonical_window_id");

    harness
        .tmux_capture(&["kill-pane", "-t", &stale_slot_one_pane])
        .unwrap_or_else(|error| panic!("E2E-21 failed killing slot 1 pane: {error}"));

    let repair = harness
        .run_ezm(&["repair"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-21 repair failed to execute: {error}"));
    samples.push(sample(&["repair"], &repair));
    let repaired_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-21 failed reading repaired slot metadata: {error}"));
    let canonical_after = session_option(harness, &session, "@ezm_canonical_window_id");
    let windows_after_repair = list_windows(harness, &session);
    let managed_windows_after_repair =
        managed_workspace_windows(harness, &session, &repaired_slots);
    let unrelated_preserved =
        window_record_present(&windows_after_repair, &unrelated_window, "e2e21-unrelated");
    let replacement_slot_one = repaired_slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();
    let slot_one_marked = pane_option(harness, &replacement_slot_one, "@ezm_slot_id")
        .is_some_and(|value| value == "1");
    let stale_binding_replaced = !replacement_slot_one.is_empty()
        && replacement_slot_one != stale_slot_one_pane
        && pane_exists(harness, &replacement_slot_one);

    let second_repair = harness
        .run_ezm(&["repair"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-21 idempotence repair failed to execute: {error}"));
    samples.push(sample(&["repair"], &second_repair));
    let slots_after_second_repair = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-21 failed reading idempotence slot metadata: {error}"));
    let canonical_after_second = session_option(harness, &session, "@ezm_canonical_window_id");
    let windows_after_second_repair = list_windows(harness, &session);
    let managed_windows_after_second =
        managed_workspace_windows(harness, &session, &slots_after_second_repair);
    let idempotent = second_repair.exit_code == 0
        && canonical_after_second == canonical_after
        && slot_snapshots_match(&slots_after_second_repair, &repaired_slots)
        && windows_after_second_repair == windows_after_repair
        && managed_windows_after_second == managed_windows_after_repair;

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!("baseline canonical window = {canonical_before:?}"));
    assertions.push(format!("stale slot 1 pane = {stale_slot_one_pane}"));
    assertions.push(format!("repaired canonical window = {canonical_after:?}"));
    assertions.push(format!("repaired slot 1 pane = {replacement_slot_one}"));
    assertions.push(format!(
        "slot 1 binding replaced and pane exists = {stale_binding_replaced}"
    ));
    assertions.push(format!(
        "replacement slot 1 pane is coherently marked = {slot_one_marked}"
    ));
    assertions.push(format!(
        "exactly one managed workspace window after repair = {}",
        managed_windows_after_repair.len() == 1
    ));
    assertions.push(format!(
        "unrelated window preserved = {unrelated_preserved}"
    ));
    assertions.push(format!("repair is idempotent = {idempotent}"));

    let settle = settle_snapshot(harness, "E2E-21");
    let session_exists = !session.is_empty();
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && repair.exit_code == 0
        && stale_binding_replaced
        && slot_one_marked
        && canonical_after
            .as_deref()
            .is_some_and(|window| managed_windows_after_repair.contains(window))
        && managed_windows_after_repair.len() == 1
        && unrelated_preserved
        && idempotent
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-21"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: session,
            exists: session_exists,
            count: usize::from(session_exists),
        },
        layout: None,
        slots: Some(slots_after_second_repair),
        remote_path: None,
        helper_state: None,
    }
}

fn session_option(harness: &FoundationHarness, session: &str, key: &str) -> Option<String> {
    harness
        .tmux_capture(&["show-options", "-v", "-t", session, key])
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn pane_option(harness: &FoundationHarness, pane: &str, key: &str) -> Option<String> {
    harness
        .tmux_capture(&["show-options", "-p", "-q", "-v", "-t", pane, key])
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn pane_exists(harness: &FoundationHarness, pane: &str) -> bool {
    harness
        .tmux_capture(&["display-message", "-p", "-t", pane, "#{pane_id}"])
        .is_ok()
}

fn list_windows(harness: &FoundationHarness, session: &str) -> Vec<String> {
    let mut windows = harness
        .tmux_capture(&[
            "list-windows",
            "-t",
            session,
            "-F",
            "#{window_id}|#{window_name}",
        ])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    windows.sort();
    windows
}

fn find_window_by_name(harness: &FoundationHarness, session: &str, name: &str) -> Option<String> {
    list_windows(harness, session)
        .into_iter()
        .find(|window| window.split('|').nth(1) == Some(name))
        .and_then(|window| window.split('|').next().map(str::to_owned))
}

fn window_record_present(windows: &[String], window_id: &str, name: &str) -> bool {
    windows.iter().any(|window| {
        window.split('|').next() == Some(window_id) && window.split('|').nth(1) == Some(name)
    })
}

fn managed_workspace_windows(
    harness: &FoundationHarness,
    session: &str,
    slots: &[super::core_support::SlotSnapshot],
) -> BTreeSet<String> {
    let bound_panes = slots
        .iter()
        .map(|slot| slot.pane_id.as_str())
        .collect::<BTreeSet<_>>();
    harness
        .tmux_capture(&[
            "list-panes",
            "-a",
            "-t",
            session,
            "-F",
            "#{window_id}|#{pane_id}",
        ])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut parts = line.trim().split('|');
            let window = parts.next()?;
            let pane = parts.next()?;
            bound_panes.contains(pane).then(|| window.to_owned())
        })
        .collect()
}
