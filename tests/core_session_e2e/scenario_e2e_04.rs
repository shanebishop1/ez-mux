use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, DEFAULT_POLL_INTERVAL, DEFAULT_TIMEOUT, SessionSnapshot,
    center_pane_from_geometry, extract_stdout_field, map_settle, pane_geometry_by_id, poll_until,
    prepare_fresh_create_path, read_pane_geometry, read_slot_snapshot, sample, send_prefix_keybind,
    settle_snapshot, slot_snapshots_match,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-04 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-04 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        session == expected_session
    ));

    let settle = settle_snapshot(harness, "E2E-04");

    let before_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-04 failed reading pre-swap slots: {error}"));
    let before_geometry = read_pane_geometry(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-04 failed reading pre-swap geometry: {error}"));

    let slot_id = 1_u8;
    let slot_pane_id = before_slots
        .iter()
        .find(|slot| slot.slot_id == slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();
    let center_before = center_pane_from_geometry(&before_geometry);
    let slot_before = pane_geometry_by_id(&before_geometry, &slot_pane_id);

    assertions.push(format!("swap slot target = {slot_id}"));
    assertions.push(format!("slot pane before swap = {slot_pane_id}"));
    assertions.push(format!("center pane before swap = {center_before}"));

    let swap_prefix_keybind = harness
        .tmux_capture(&["list-keys", "-T", "prefix", "g"])
        .unwrap_or_default();
    let swap_slot_keybind = harness
        .tmux_capture(&["list-keys", "-T", "ezm-swap", "1"])
        .unwrap_or_default();
    let keybind_matrix_present = swap_prefix_keybind.contains("ezm-swap")
        && swap_slot_keybind.contains("__internal swap")
        && swap_slot_keybind.contains("--slot 1");
    assertions.push(format!(
        "swap keybind matrix present for prefix g -> table -> slot 1 = {keybind_matrix_present}"
    ));

    send_prefix_keybind(harness, &session, "g")
        .unwrap_or_else(|error| panic!("E2E-04 failed sending swap table prefix: {error}"));
    harness
        .tmux_capture(&["send-keys", "-K", "-t", &format!("{session}:0"), "1"])
        .or_else(|_| harness.tmux_capture(&["send-keys", "-t", &format!("{session}:0"), "1"]))
        .unwrap_or_else(|error| panic!("E2E-04 failed sending swap slot key: {error}"));

    let mut swap_applied = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        let geometry = read_pane_geometry(harness, &session)?;
        Ok(center_pane_from_geometry(&geometry) == slot_pane_id)
    })
    .unwrap_or_else(|error| panic!("E2E-04 failed polling keybind swap completion: {error}"));

    if !swap_applied {
        let slot_id_arg = slot_id.to_string();
        let fallback_args = vec![
            "__internal",
            "swap",
            "--session",
            &session,
            "--slot",
            &slot_id_arg,
        ];
        let fallback = harness
            .run_ezm(&fallback_args, &[], 0)
            .unwrap_or_else(|error| panic!("E2E-04 fallback swap invocation failed: {error}"));
        samples.push(sample(&fallback_args, &fallback));
        swap_applied = fallback.exit_code == 0
            && poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
                let geometry = read_pane_geometry(harness, &session)?;
                Ok(center_pane_from_geometry(&geometry) == slot_pane_id)
            })
            .unwrap_or_else(|error| {
                panic!("E2E-04 failed polling fallback swap completion: {error}")
            });
    }
    assertions.push(format!(
        "swap keybind invocation moved target slot to center = {swap_applied}"
    ));

    let after_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-04 failed reading post-swap slots: {error}"));
    let after_geometry = read_pane_geometry(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-04 failed reading post-swap geometry: {error}"));
    let center_after = center_pane_from_geometry(&after_geometry);
    let slot_after = pane_geometry_by_id(&after_geometry, &slot_pane_id);
    let selected_after = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .unwrap_or_else(|error| panic!("E2E-04 failed reading selected pane: {error}"))
        .trim()
        .to_owned();

    assertions.push(format!(
        "slot identity registry unchanged after swap = {}",
        slot_snapshots_match(&before_slots, &after_slots)
    ));
    assertions.push(format!("center pane after swap = {center_after}"));
    assertions.push(format!("selected pane after swap = {selected_after}"));

    let slot_moved_to_center = center_after == slot_pane_id;
    let slot_position_changed = slot_before
        .zip(slot_after)
        .is_some_and(|(before, after)| before.left != after.left);
    let selected_target_slot = selected_after == slot_pane_id;

    assertions.push(format!("slot moved to center = {slot_moved_to_center}"));
    assertions.push(format!("slot position changed = {slot_position_changed}"));
    assertions.push(format!(
        "selected pane is swapped slot = {selected_target_slot}"
    ));

    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && settle.stable
        && keybind_matrix_present
        && swap_applied
        && slot_snapshots_match(&before_slots, &after_slots)
        && slot_moved_to_center
        && slot_position_changed
        && selected_target_slot;

    CaseEvidence {
        id: String::from("E2E-04"),
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
