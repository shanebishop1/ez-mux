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
    let first_session = extract_stdout_field(&first.stdout, "session").unwrap_or_default();
    let second_probe = harness
        .run_ezm_with_pty_attach_probe(harness.project_root(), &[], &[], 0, &first_session)
        .unwrap_or_else(|error| panic!("E2E-01 second launch failed: {error}"));
    let second = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-01 third launch failed: {error}"));

    samples.push(sample(&[], &first));
    samples.push(sample(&[], &second));

    let first_action = extract_stdout_field(&first.stdout, "session_action").unwrap_or_default();
    let second_action = extract_stdout_field(&second.stdout, "session_action").unwrap_or_default();
    let second_session = extract_stdout_field(&second.stdout, "session").unwrap_or_default();

    assertions.push(format!("first action = {first_action}"));
    assertions.push(format!("second action = {second_action}"));
    assertions.push(format!("first session = {first_session}"));
    assertions.push(format!("second session = {second_session}"));
    assertions.push(format!(
        "session names match = {}",
        first_session == second_session
    ));
    assertions.push(format!(
        "interactive attach observed tmux client = {}",
        second_probe.observed_attached_client
    ));
    assertions.push(format!("pty probe exit code = {}", second_probe.exit_code));

    let create_keybind_present = keybind_matrix_present(harness);
    let attach_keybind_present = keybind_matrix_present(harness);
    assertions.push(format!(
        "keybind matrix present after create path = {create_keybind_present}"
    ));
    assertions.push(format!(
        "keybind matrix present after attach path = {attach_keybind_present}"
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

    let attach_probe_exit_acceptable = second_probe.exit_code == 0
        || (second_probe.exit_code == 1 && second_probe.observed_attached_client);
    assertions.push(format!(
        "pty probe exit acceptable = {attach_probe_exit_acceptable}"
    ));

    let pass = first.exit_code == 0
        && attach_probe_exit_acceptable
        && second.exit_code == 0
        && first_action == "create"
        && second_action == "attach"
        && !first_session.is_empty()
        && first_session == second_session
        && second_probe.observed_attached_client
        && create_keybind_present
        && attach_keybind_present
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
        remote_path: None,
        helper_state: None,
    }
}

fn keybind_matrix_present(harness: &FoundationHarness) -> bool {
    let key_checks = [
        ("prefix", "g", "ezm-swap"),
        ("prefix", "u", "__internal mode"),
        ("prefix", "a", "--mode agent"),
        ("prefix", "S", "--mode shell"),
        ("prefix", "N", "--mode neovim"),
        ("prefix", "G", "--mode lazygit"),
        ("prefix", "P", "__internal popup"),
        ("prefix", "M-3", "__internal preset"),
        ("ezm-swap", "1", "__internal swap"),
    ];

    key_checks.iter().all(|(table, key, marker)| {
        harness
            .tmux_capture(&["list-keys", "-T", table, key])
            .unwrap_or_default()
            .contains(marker)
    })
}
