use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, DEFAULT_POLL_INTERVAL, DEFAULT_TIMEOUT, HelperLifecycleEvidence, SessionSnapshot,
    extract_stdout_field, map_settle, popup_helper_session_name, prepare_fresh_create_path,
    read_helper_state_snapshot, read_slot_snapshot, sample, settle_snapshot,
    wait_for_helper_pids_to_exit,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-11 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-11 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let popup_args = vec!["__internal", "popup", "--session", &session, "--slot", "4"];
    let popup_open = harness
        .run_ezm(&popup_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-11 popup open failed to execute: {error}"));
    samples.push(sample(&popup_args, &popup_open));

    let popup_session = popup_helper_session_name(&session, 4);
    let popup_present_before_teardown = harness
        .tmux_capture(&["has-session", "-t", &popup_session])
        .is_ok();

    let auxiliary_open_args = vec![
        "__internal",
        "auxiliary",
        "--session",
        &session,
        "--action",
        "open",
    ];
    let auxiliary_open = harness
        .run_ezm(&auxiliary_open_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-11 auxiliary open failed to execute: {error}"));
    samples.push(sample(&auxiliary_open_args, &auxiliary_open));

    let before_state = read_helper_state_snapshot(harness, &session);

    let teardown_args = vec!["__internal", "teardown", "--session", &session];
    let teardown = harness
        .run_ezm(&teardown_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-11 teardown command failed to execute: {error}"));
    samples.push(sample(&teardown_args, &teardown));

    let project_session_present_after_teardown = harness
        .tmux_capture(&["has-session", "-t", &session])
        .is_ok();
    let after_state = read_helper_state_snapshot(harness, &session);
    let leaked_helper_pids = wait_for_helper_pids_to_exit(
        &before_state.helper_pane_pids,
        DEFAULT_TIMEOUT,
        DEFAULT_POLL_INTERVAL,
    )
    .unwrap_or_else(|error| panic!("E2E-11 failed polling helper pid shutdown: {error}"));

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!("popup open exit_code = {}", popup_open.exit_code));
    assertions.push(format!(
        "auxiliary open exit_code = {}",
        auxiliary_open.exit_code
    ));
    assertions.push(format!("teardown exit_code = {}", teardown.exit_code));
    assertions.push(format!(
        "popup helper session exists before teardown = {popup_present_before_teardown}"
    ));
    assertions.push(format!(
        "helper sessions present before teardown = {}",
        !before_state.helper_sessions.is_empty()
    ));
    assertions.push(format!(
        "helper pane pids present before teardown = {}",
        !before_state.helper_pane_pids.is_empty()
    ));
    assertions.push(format!(
        "project session removed after teardown = {}",
        !project_session_present_after_teardown
    ));
    assertions.push(format!(
        "helper sessions removed after teardown = {}",
        after_state.helper_sessions.is_empty()
    ));
    assertions.push(format!(
        "helper pane pids removed after teardown = {}",
        after_state.helper_pane_pids.is_empty()
    ));
    assertions.push(format!(
        "tracked pre-teardown helper pids still alive after teardown = {}",
        leaked_helper_pids.len()
    ));

    let settle = settle_snapshot(harness, "E2E-11");
    let slots = read_slot_snapshot(harness, &session).unwrap_or_default();
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && popup_open.exit_code == 0
        && auxiliary_open.exit_code == 0
        && teardown.exit_code == 0
        && popup_present_before_teardown
        && !before_state.helper_sessions.is_empty()
        && !before_state.helper_pane_pids.is_empty()
        && !project_session_present_after_teardown
        && after_state.helper_sessions.is_empty()
        && after_state.helper_pane_pids.is_empty()
        && leaked_helper_pids.is_empty()
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-11"),
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
        slots: Some(slots),
        remote_path: None,
        helper_state: Some(HelperLifecycleEvidence {
            before: before_state,
            after: after_state,
            pre_helper_pids_alive_after_teardown: leaked_helper_pids,
        }),
    }
}
