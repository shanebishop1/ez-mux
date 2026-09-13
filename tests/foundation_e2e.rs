mod support;

#[path = "foundation_e2e/evidence.rs"]
mod evidence;
#[path = "foundation_e2e/support.rs"]
mod foundation_support;
#[path = "foundation_e2e/scenario_e2e_00.rs"]
mod scenario_e2e_00;
#[path = "foundation_e2e/scenario_e2e_15.rs"]
mod scenario_e2e_15;
#[path = "foundation_e2e/scenario_e2e_17.rs"]
mod scenario_e2e_17;
#[path = "foundation_e2e/scenario_e2e_18.rs"]
mod scenario_e2e_18;
#[path = "foundation_e2e/scenario_e2e_19.rs"]
mod scenario_e2e_19;
#[path = "foundation_e2e/scenario_e2e_19_support.rs"]
mod scenario_e2e_19_support;
#[path = "foundation_e2e/scenario_e2e_20.rs"]
mod scenario_e2e_20;

use evidence::{
    CaseEvidence, FOUNDATION_IDS, RunMetadata, SuiteEvidence, read_commit_sha,
    write_case_artifacts, write_json,
};
use support::foundation_harness::FoundationHarness;

fn run_scenario(
    harness: &FoundationHarness,
    id: &str,
    run: impl FnOnce() -> CaseEvidence,
) -> CaseEvidence {
    harness
        .reset_scenario_state()
        .unwrap_or_else(|error| panic!("{id} failed restoring declared initial state: {error}"));
    let mut evidence = run();
    match harness.reset_scenario_state() {
        Ok(()) => evidence.assertions.push(String::from(
            "scenario-owned tmux state cleaned exactly = true",
        )),
        Err(error) => {
            evidence.pass = false;
            evidence.assertions.push(format!(
                "scenario-owned tmux state cleaned exactly = false ({error})"
            ));
        }
    }
    evidence
}

#[test]
fn foundation_e2e_suite() {
    let harness =
        FoundationHarness::new().unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let cases = vec![
        run_scenario(&harness, "E2E-00", || scenario_e2e_00::run(&harness)),
        run_scenario(&harness, "E2E-15", || scenario_e2e_15::run(&harness)),
        run_scenario(&harness, "E2E-17", || scenario_e2e_17::run(&harness)),
        run_scenario(&harness, "E2E-18", || scenario_e2e_18::run(&harness)),
        run_scenario(&harness, "E2E-19", || scenario_e2e_19::run(&harness)),
        run_scenario(&harness, "E2E-20", || scenario_e2e_20::run(&harness)),
    ];

    write_case_artifacts(&harness.artifact_dir.join("cases"), &cases)
        .unwrap_or_else(|error| panic!("failed writing case evidence artifacts: {error}"));

    let pass_total = cases.iter().filter(|case| case.pass).count();
    let fail_total = cases.len() - pass_total;

    let summary = SuiteEvidence {
        metadata: RunMetadata {
            run_id: harness.run_id.clone(),
            commit_sha: read_commit_sha(harness.project_root()),
            os: std::env::consts::OS.to_owned(),
            shell: harness.shell.clone(),
            tmux_version: harness
                .tmux_version()
                .unwrap_or_else(|error| format!("unknown ({error})")),
            artifact_dir: harness.artifact_dir.display().to_string(),
            test_ids: FOUNDATION_IDS.iter().map(|id| (*id).to_string()).collect(),
            pass_total,
            fail_total,
        },
        cases,
    };

    write_json(&harness.artifact_dir.join("summary.json"), &summary)
        .unwrap_or_else(|error| panic!("failed writing summary evidence: {error}"));

    assert_eq!(
        summary.metadata.fail_total, 0,
        "foundation E2E suite contains failures; inspect summary artifact"
    );
}
