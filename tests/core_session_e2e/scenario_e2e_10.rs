use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, create_remote_remap_fixture, extract_stdout_field, map_settle,
    prepare_fresh_create_path, read_slot_snapshot, sample, settle_snapshot,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let fixture = create_remote_remap_fixture(harness)
        .unwrap_or_else(|error| panic!("E2E-10 remote fixture setup failed: {error}"));
    let remote_prefix = fixture.remote_prefix.display().to_string();
    let expected_mapped_path = fixture.expected_mapped_path.display().to_string();
    let expected_operator = String::from("e2e-operator");

    let expected_session = prepare_fresh_create_path(harness, &fixture.project_dir)
        .unwrap_or_else(|error| panic!("E2E-10 setup failed: {error}"));

    let launch = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &[],
            &[
                ("OPENCODE_REMOTE_DIR_PREFIX", &remote_prefix),
                ("OPERATOR", &expected_operator),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let switch_args = vec![
        "__internal",
        "mode",
        "--session",
        &session,
        "--slot",
        "3",
        "--mode",
        "shell",
    ];
    let switch_success = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &switch_args,
            &[
                ("OPENCODE_REMOTE_DIR_PREFIX", &remote_prefix),
                ("OPERATOR", &expected_operator),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 shell switch failed to execute: {error}"));
    samples.push(sample(&switch_args, &switch_success));

    let slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-10 failed reading slot snapshot: {error}"));
    let slot_three_pane = slots
        .iter()
        .find(|slot| slot.slot_id == 3)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    let pane_start_command = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &slot_three_pane,
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-10 failed reading pane start command: {error}"));

    let switch_fail = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &switch_args,
            &[("OPENCODE_REMOTE_DIR_PREFIX", &remote_prefix)],
            0,
        )
        .unwrap_or_else(|error| {
            panic!("E2E-10 missing-operator branch failed to execute: {error}")
        });
    samples.push(sample(&switch_args, &switch_fail));

    let fail_fast_non_zero = switch_fail.exit_code != 0;
    let fail_fast_diagnostic = switch_fail
        .stderr
        .contains("remote-prefix routing requires OPERATOR to be set");
    let operator_matches = pane_start_command.contains(&expected_operator);
    let remote_dir_matches = pane_start_command.contains(&expected_mapped_path);

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "success branch mode switch exit_code = {}",
        switch_success.exit_code
    ));
    assertions.push(format!(
        "success branch pane start command = {}",
        pane_start_command.trim()
    ));
    assertions.push(format!(
        "success branch effective operator token present in pane start command = {operator_matches}"
    ));
    assertions.push(format!(
        "success branch effective remote dir token present in pane start command = {remote_dir_matches}"
    ));
    assertions.push(format!(
        "success branch expected remote dir = {expected_mapped_path}"
    ));
    assertions.push(format!(
        "success branch effective operator matches configured operator = {operator_matches}"
    ));
    assertions.push(format!(
        "success branch effective remote dir matches mapped path = {remote_dir_matches}"
    ));
    assertions.push(format!(
        "fail-fast branch exit_code={} non_zero={fail_fast_non_zero}",
        switch_fail.exit_code
    ));
    assertions.push(format!(
        "fail-fast branch stderr diagnostic surfaced = {fail_fast_diagnostic}"
    ));

    let settle = settle_snapshot(harness, "E2E-10");
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && switch_success.exit_code == 0
        && operator_matches
        && remote_dir_matches
        && fail_fast_non_zero
        && fail_fast_diagnostic
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-10"),
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
    }
}
