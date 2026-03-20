use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, center_pane_from_geometry, extract_stdout_field, map_settle,
    prepare_fresh_create_path, read_pane_geometry, read_slot_snapshot, sample, settle_snapshot,
    slot_snapshots_match,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-05 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-05 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        session == expected_session
    ));

    let settle = settle_snapshot(harness, "E2E-05");

    let before_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-05 failed reading pre-zoom slots: {error}"));
    let before_geometry = read_pane_geometry(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-05 failed reading pre-zoom geometry: {error}"));
    let center_before = center_pane_from_geometry(&before_geometry);
    let swap_slot_id = 4_u8;
    let swap_slot_pane = before_slots
        .iter()
        .find(|slot| slot.slot_id == swap_slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    harness
        .tmux_capture(&["resize-pane", "-Z", "-t", &center_before])
        .unwrap_or_else(|error| panic!("E2E-05 failed to enable zoom: {error}"));

    let zoom_before = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{window_zoomed_flag}",
        ])
        .unwrap_or_else(|error| panic!("E2E-05 failed reading pre-swap zoom flag: {error}"))
        .trim()
        .to_owned();
    assertions.push(format!("zoom flag before swap = {zoom_before}"));

    let swap_slot_id_text = swap_slot_id.to_string();
    let swap_args = vec![
        "__internal",
        "swap",
        "--session",
        &session,
        "--slot",
        &swap_slot_id_text,
    ];
    let swap = harness
        .run_ezm(&swap_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-05 swap invocation failed: {error}"));
    samples.push(sample(&swap_args, &swap));

    let zoom_after = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{window_zoomed_flag}",
        ])
        .unwrap_or_else(|error| panic!("E2E-05 failed reading post-swap zoom flag: {error}"))
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
        .unwrap_or_else(|error| panic!("E2E-05 failed reading selected pane after swap: {error}"))
        .trim()
        .to_owned();

    assertions.push(format!("zoom flag after swap = {zoom_after}"));
    assertions.push(format!(
        "selected pane after zoomed swap = {selected_after}"
    ));

    harness
        .tmux_capture(&["resize-pane", "-Z", "-t", &selected_after])
        .unwrap_or_else(|error| panic!("E2E-05 failed to disable zoom: {error}"));

    let after_geometry = read_pane_geometry(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-05 failed reading post-unzoom geometry: {error}"));
    let after_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-05 failed reading post-swap slots: {error}"));
    let center_after = center_pane_from_geometry(&after_geometry);

    let zoom_preserved = zoom_before == "1" && zoom_after == "1";
    let selected_target_slot = selected_after == swap_slot_pane;
    let swapped_slot_now_center = center_after == swap_slot_pane;

    assertions.push(format!(
        "zoom preserved across swap/select = {zoom_preserved}"
    ));
    assertions.push(format!(
        "selected pane is slot-{swap_slot_id} = {selected_target_slot}"
    ));
    assertions.push(format!(
        "slot-{swap_slot_id} moved to center after unzoom = {swapped_slot_now_center}"
    ));
    assertions.push(format!(
        "slot identity registry unchanged after zoomed swap = {}",
        slot_snapshots_match(&before_slots, &after_slots)
    ));

    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && settle.stable
        && swap.exit_code == 0
        && zoom_preserved
        && selected_target_slot
        && swapped_slot_now_center
        && slot_snapshots_match(&before_slots, &after_slots);

    CaseEvidence {
        id: String::from("E2E-05"),
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
