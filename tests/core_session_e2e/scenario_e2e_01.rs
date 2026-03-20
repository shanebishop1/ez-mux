use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, extract_stdout_field, map_settle, sample, settle_snapshot,
};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let first = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-01 first launch failed: {error}"));
    let second = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-01 second launch failed: {error}"));

    samples.push(sample(&[], &first));
    samples.push(sample(&[], &second));

    let first_action = extract_stdout_field(&first.stdout, "session_action").unwrap_or_default();
    let second_action = extract_stdout_field(&second.stdout, "session_action").unwrap_or_default();
    let first_session = extract_stdout_field(&first.stdout, "session").unwrap_or_default();
    let second_session = extract_stdout_field(&second.stdout, "session").unwrap_or_default();

    assertions.push(format!("first action = {first_action}"));
    assertions.push(format!("second action = {second_action}"));
    assertions.push(format!("first session = {first_session}"));
    assertions.push(format!("second session = {second_session}"));
    assertions.push(format!(
        "session names match = {}",
        first_session == second_session
    ));

    let settle = settle_snapshot(harness, "E2E-01");
    let sessions: Vec<&str> = settle
        .sessions
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let session_count = sessions
        .iter()
        .copied()
        .filter(|name| *name == second_session)
        .count();
    let session_exists = session_count == 1;

    assertions.push(format!(
        "session appears once in tmux snapshot = {session_exists}"
    ));

    let pass = first.exit_code == 0
        && second.exit_code == 0
        && first_action == "create"
        && second_action == "attach"
        && !first_session.is_empty()
        && first_session == second_session
        && session_exists
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-01"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: second_session,
            exists: session_exists,
            count: session_count,
        },
        layout: None,
        slots: None,
    }
}
