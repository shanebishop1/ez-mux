mod support;

#[path = "core_session_e2e/core_support.rs"]
mod core_support;
#[path = "core_session_e2e/scenario_e2e_01.rs"]
mod scenario_e2e_01;
#[path = "core_session_e2e/scenario_e2e_02.rs"]
mod scenario_e2e_02;
#[path = "core_session_e2e/scenario_e2e_03.rs"]
mod scenario_e2e_03;
#[path = "core_session_e2e/scenario_e2e_04.rs"]
mod scenario_e2e_04;
#[path = "core_session_e2e/scenario_e2e_05.rs"]
mod scenario_e2e_05;
#[path = "core_session_e2e/scenario_e2e_06.rs"]
mod scenario_e2e_06;
#[path = "core_session_e2e/scenario_e2e_07.rs"]
mod scenario_e2e_07;
#[path = "core_session_e2e/scenario_e2e_08.rs"]
mod scenario_e2e_08;
#[path = "core_session_e2e/scenario_e2e_09.rs"]
mod scenario_e2e_09;
#[path = "core_session_e2e/scenario_e2e_10.rs"]
mod scenario_e2e_10;
#[path = "core_session_e2e/scenario_e2e_11.rs"]
mod scenario_e2e_11;
#[path = "core_session_e2e/scenario_e2e_12.rs"]
mod scenario_e2e_12;
#[path = "core_session_e2e/scenario_e2e_13.rs"]
mod scenario_e2e_13;
#[path = "core_session_e2e/scenario_e2e_16.rs"]
mod scenario_e2e_16;
#[path = "core_session_e2e/scenario_e2e_19.rs"]
mod scenario_e2e_19;

use core_support::{
    CORE_IDS, RunMetadata, SuiteEvidence, read_commit_sha, write_case_artifacts, write_json,
};
use support::foundation_harness::FoundationHarness;

#[test]
fn core_session_e2e_suite() {
    let harness = FoundationHarness::new_for_suite("core-session-orchestration")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let cases = vec![
        scenario_e2e_01::run(&harness),
        scenario_e2e_02::run(&harness),
        scenario_e2e_03::run(&harness),
        scenario_e2e_04::run(&harness),
        scenario_e2e_05::run(&harness),
        scenario_e2e_06::run(&harness),
        scenario_e2e_07::run(&harness),
        scenario_e2e_08::run(&harness),
        scenario_e2e_09::run(&harness),
        scenario_e2e_10::run(&harness),
        scenario_e2e_11::run(&harness),
        scenario_e2e_12::run(&harness),
        scenario_e2e_13::run(&harness),
        scenario_e2e_16::run(&harness),
        scenario_e2e_19::run(&harness),
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
            test_ids: CORE_IDS.iter().map(|id| (*id).to_string()).collect(),
            pass_total,
            fail_total,
        },
        cases,
    };

    write_json(&harness.artifact_dir.join("summary.json"), &summary)
        .unwrap_or_else(|error| panic!("failed writing summary evidence: {error}"));

    assert_eq!(
        summary.metadata.fail_total, 0,
        "core session E2E suite contains failures; inspect summary artifact"
    );
}
