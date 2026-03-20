use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, extract_stdout_field, inspect_layout, map_settle,
    prepare_fresh_create_path, sample, settle_snapshot,
};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-02 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-02 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        session == expected_session
    ));

    let settle = settle_snapshot(harness, "E2E-02");
    let (layout_snapshot, layout_assertions) = inspect_layout(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-02 failed to inspect layout: {error}"));
    assertions.extend(layout_assertions);

    let sessions: Vec<&str> = settle
        .sessions
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let session_count = sessions
        .iter()
        .copied()
        .filter(|name| *name == session)
        .count();
    let session_exists = session_count == 1;

    let pass = launch.exit_code == 0
        && launch_action == "create"
        && !session.is_empty()
        && session == expected_session
        && session_exists
        && settle.stable
        && layout_snapshot.pane_count == 5
        && layout_snapshot.center_column_panes == 1
        && layout_snapshot.left_column_panes == 2
        && layout_snapshot.right_column_panes == 2
        && layout_snapshot.center_within_tolerance;

    CaseEvidence {
        id: String::from("E2E-02"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: session,
            exists: session_exists,
            count: session_count,
        },
        layout: Some(layout_snapshot),
        slots: None,
        remote_path: None,
        helper_state: None,
    }
}
