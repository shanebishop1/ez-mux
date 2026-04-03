use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, extract_stdout_field, map_settle, prepare_fresh_create_path,
    read_slot_snapshot, sample, settle_snapshot,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-08 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-08 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let open_args = vec![
        "__internal",
        "auxiliary",
        "--session",
        &session,
        "--action",
        "open",
    ];
    let open_first = harness
        .run_ezm(&open_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-08 first open failed to execute: {error}"));
    samples.push(sample(&open_args, &open_first));

    let open_second = harness
        .run_ezm(&open_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-08 second open failed to execute: {error}"));
    samples.push(sample(&open_args, &open_second));

    let window_name = "perles";
    let windows_after_open = harness
        .tmux_capture(&[
            "list-windows",
            "-t",
            &session,
            "-F",
            "#{window_id}|#{window_name}|#{window_flags}",
        ])
        .unwrap_or_default();
    let window_matches: Vec<&str> = windows_after_open
        .lines()
        .filter(|line| line.contains(&format!("|{window_name}|")))
        .collect();
    let remain_on_exit = window_matches.first().is_some_and(|line| {
        let window_id = line.split('|').next().unwrap_or_default();
        harness
            .tmux_capture(&[
                "show-options",
                "-w",
                "-v",
                "-t",
                window_id,
                "remain-on-exit",
            ])
            .map(|output| output.trim().to_owned())
            .unwrap_or_default()
            == "on"
    });

    let close_args = vec![
        "__internal",
        "auxiliary",
        "--session",
        &session,
        "--action",
        "close",
    ];
    let close = harness
        .run_ezm(&close_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-08 close failed to execute: {error}"));
    samples.push(sample(&close_args, &close));

    let windows_after_close = harness
        .tmux_capture(&["list-windows", "-t", &session, "-F", "#{window_name}"])
        .unwrap_or_default();
    let window_present_after_close = windows_after_close
        .lines()
        .any(|name| name.trim() == window_name);

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!("first open exit_code = {}", open_first.exit_code));
    assertions.push(format!("second open exit_code = {}", open_second.exit_code));
    assertions.push(format!("close exit_code = {}", close.exit_code));
    assertions.push(format!(
        "auxiliary window exists exactly once after repeated opens = {}",
        window_matches.len() == 1
    ));
    assertions.push(format!("auxiliary remain-on-exit = {remain_on_exit}"));
    assertions.push(format!(
        "auxiliary window removed after close = {}",
        !window_present_after_close
    ));

    let settle = settle_snapshot(harness, "E2E-08");

    let slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-08 failed reading slot snapshot: {error}"));
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && open_first.exit_code == 0
        && open_second.exit_code == 0
        && close.exit_code == 0
        && window_matches.len() == 1
        && remain_on_exit
        && !window_present_after_close
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-08"),
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
        helper_state: None,
    }
}
