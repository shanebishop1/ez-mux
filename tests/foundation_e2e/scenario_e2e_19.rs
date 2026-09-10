use std::time::Duration;

use crate::support::foundation_harness::FoundationHarness;

use super::evidence::CaseEvidence;
use super::foundation_support::{map_settle, sample};
use super::scenario_e2e_19_support::{
    capture_session_auth, exercise_popup_parent_context, extract_session_auth_sessions,
    prepare_session_auth_projects, run_session_auth_scenarios, session_auth_assertions,
    session_auth_passes, set_global_session_auth_fixture,
};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let projects = prepare_session_auth_projects(harness);
    set_global_session_auth_fixture(harness);
    let runs = run_session_auth_scenarios(harness, &projects);
    let sessions = extract_session_auth_sessions(&runs);
    let auth = capture_session_auth(harness, &sessions);
    let helper_internal = exercise_popup_parent_context(harness, &sessions.a);

    let assertions = session_auth_assertions(&runs, &auth, &helper_internal);
    let samples = vec![
        sample(&["--verbose"], &runs.a),
        sample(&["--verbose"], &runs.b),
        sample(&["--verbose"], &runs.empty),
    ];
    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-19 settle evidence failed: {error}"));
    let pass = session_auth_passes(&runs, &auth, &helper_internal, settle.stable);

    CaseEvidence {
        id: String::from("E2E-19"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
    }
}
