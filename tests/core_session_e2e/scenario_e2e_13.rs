use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, extract_stdout_field, inspect_layout, map_settle,
    pane_graph_stable, prepare_fresh_create_path, read_pane_graph, read_slot_snapshot, sample,
    settle_snapshot, slot_worktree_mapping_stable,
};

const REPEATED_ROUND_TRIPS: usize = 3;

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-13 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-13 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let baseline_graph = read_pane_graph(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-13 failed reading baseline pane graph: {error}"));
    let baseline_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-13 failed reading baseline slot snapshot: {error}"));

    let preset_args = vec![
        "__internal",
        "preset",
        "--session",
        &session,
        "--preset",
        "three-pane",
    ];

    let to_three_single = harness
        .run_ezm(&preset_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-13 single transition to 3-pane failed: {error}"));
    samples.push(sample(&preset_args, &to_three_single));
    let (single_three_layout, _) = inspect_layout(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-13 failed reading single 3-pane layout: {error}"));

    let to_five_single = harness
        .run_ezm(&preset_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-13 single transition back to 5-pane failed: {error}"));
    samples.push(sample(&preset_args, &to_five_single));

    let graph_after_single = read_pane_graph(harness, &session).unwrap_or_else(|error| {
        panic!("E2E-13 failed reading single round-trip pane graph: {error}")
    });
    let slots_after_single = read_slot_snapshot(harness, &session).unwrap_or_else(|error| {
        panic!("E2E-13 failed reading single round-trip slot snapshot: {error}")
    });

    let single_pane_graph_stable = pane_graph_stable(&baseline_graph, &graph_after_single);
    let single_slot_mapping_stable =
        slot_worktree_mapping_stable(&baseline_slots, &slots_after_single);

    let mut repeated_three_layouts_ok = true;
    let mut repeated_transition_commands_ok = true;
    for _ in 0..REPEATED_ROUND_TRIPS {
        let to_three = harness
            .run_ezm(&preset_args, &[], 0)
            .unwrap_or_else(|error| panic!("E2E-13 repeated transition to 3-pane failed: {error}"));
        repeated_transition_commands_ok &= to_three.exit_code == 0;
        samples.push(sample(&preset_args, &to_three));

        let (three_layout, _) = inspect_layout(harness, &session).unwrap_or_else(|error| {
            panic!("E2E-13 failed reading repeated 3-pane layout: {error}")
        });
        repeated_three_layouts_ok &= three_layout.pane_count == 3
            && three_layout.left_column_panes == 1
            && three_layout.center_column_panes == 1
            && three_layout.right_column_panes == 1
            && three_layout.three_pane_within_tolerance;

        let to_five = harness
            .run_ezm(&preset_args, &[], 0)
            .unwrap_or_else(|error| {
                panic!("E2E-13 repeated transition back to 5-pane failed: {error}")
            });
        repeated_transition_commands_ok &= to_five.exit_code == 0;
        samples.push(sample(&preset_args, &to_five));
    }

    let graph_after_repeated = read_pane_graph(harness, &session).unwrap_or_else(|error| {
        panic!("E2E-13 failed reading repeated round-trip pane graph: {error}")
    });
    let slots_after_repeated = read_slot_snapshot(harness, &session).unwrap_or_else(|error| {
        panic!("E2E-13 failed reading repeated round-trip slot snapshot: {error}")
    });

    let repeated_pane_graph_stable = pane_graph_stable(&baseline_graph, &graph_after_repeated);
    let repeated_slot_mapping_stable =
        slot_worktree_mapping_stable(&baseline_slots, &slots_after_repeated);

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "single 5->3 transition reached three-pane tolerance = {}",
        single_three_layout.pane_count == 3
            && single_three_layout.left_column_panes == 1
            && single_three_layout.center_column_panes == 1
            && single_three_layout.right_column_panes == 1
            && single_three_layout.three_pane_within_tolerance
    ));
    assertions.push(format!(
        "single round-trip pane graph stable = {single_pane_graph_stable}"
    ));
    assertions.push(format!(
        "single round-trip slot mapping stable = {single_slot_mapping_stable}"
    ));
    assertions.push(format!("repeated 5->3->5 rounds = {REPEATED_ROUND_TRIPS}"));
    assertions.push(format!(
        "repeated transitions reached valid three-pane layout each round = {repeated_three_layouts_ok}"
    ));
    assertions.push(format!(
        "repeated round-trip pane graph stable = {repeated_pane_graph_stable}"
    ));
    assertions.push(format!(
        "repeated round-trip slot mapping stable = {repeated_slot_mapping_stable}"
    ));

    let settle = settle_snapshot(harness, "E2E-13");
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && to_three_single.exit_code == 0
        && to_five_single.exit_code == 0
        && single_three_layout.pane_count == 3
        && single_three_layout.left_column_panes == 1
        && single_three_layout.center_column_panes == 1
        && single_three_layout.right_column_panes == 1
        && single_three_layout.three_pane_within_tolerance
        && graph_after_single.len() == 5
        && single_slot_mapping_stable
        && repeated_transition_commands_ok
        && repeated_three_layouts_ok
        && graph_after_repeated.len() == 5
        && repeated_slot_mapping_stable
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-13"),
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
        slots: Some(slots_after_repeated),
        remote_path: None,
        helper_state: None,
    }
}
