use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, extract_stdout_field, inspect_layout, map_settle,
    prepare_fresh_create_path, read_slot_snapshot, sample, settle_snapshot,
};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-12 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-12 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();

    let (pre_layout, pre_assertions) = inspect_layout(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-12 failed reading pre-preset layout: {error}"));
    assertions.extend(
        pre_assertions
            .into_iter()
            .map(|item| format!("pre: {item}")),
    );

    let keybind_dump = harness
        .tmux_capture(&["list-keys", "-T", "prefix", "M-3"])
        .unwrap_or_default();
    let keybind_has_internal_entrypoint =
        keybind_dump.contains("__internal preset") && keybind_dump.contains("three-pane");
    assertions.push(format!(
        "keybind M-3 routes to preset entrypoint = {keybind_has_internal_entrypoint}"
    ));

    let preset_args = vec![
        "__internal",
        "preset",
        "--session",
        &session,
        "--preset",
        "three-pane",
    ];
    let preset = harness
        .run_ezm(&preset_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-12 preset invocation failed: {error}"));
    samples.push(sample(&preset_args, &preset));

    let (post_layout, post_assertions) = inspect_layout(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-12 failed reading post-preset layout: {error}"));
    assertions.extend(
        post_assertions
            .into_iter()
            .map(|item| format!("post: {item}")),
    );
    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!("preset exit_code = {}", preset.exit_code));
    assertions.push(format!("pre pane count = {}", pre_layout.pane_count));
    assertions.push(format!("post pane count = {}", post_layout.pane_count));
    assertions.push(format!(
        "post three-pane tolerance satisfied = {}",
        post_layout.three_pane_within_tolerance
    ));

    let settle = settle_snapshot(harness, "E2E-12");
    let slots = read_slot_snapshot(harness, &session).unwrap_or_default();
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && pre_layout.pane_count == 5
        && keybind_has_internal_entrypoint
        && preset.exit_code == 0
        && post_layout.pane_count == 3
        && post_layout.left_column_panes == 1
        && post_layout.center_column_panes == 1
        && post_layout.right_column_panes == 1
        && post_layout.three_pane_within_tolerance
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-12"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: session,
            exists: session_exists,
            count: session_count,
        },
        layout: Some(post_layout),
        slots: Some(slots),
        remote_path: None,
        helper_state: None,
    }
}
