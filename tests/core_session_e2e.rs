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
#[path = "core_session_e2e/scenario_e2e_20.rs"]
mod scenario_e2e_20;
#[path = "core_session_e2e/scenario_e2e_21.rs"]
mod scenario_e2e_21;

use core_support::{
    CORE_IDS, CaseEvidence, RunMetadata, SuiteEvidence, read_commit_sha, write_case_artifacts,
    write_json,
};
use support::foundation_harness::FoundationHarness;

struct ScenarioStateGuard<'a> {
    harness: &'a FoundationHarness,
    active: bool,
}

impl<'a> ScenarioStateGuard<'a> {
    fn new(harness: &'a FoundationHarness, id: &str) -> Self {
        harness.reset_scenario_state().unwrap_or_else(|error| {
            panic!("{id} failed restoring declared initial state: {error}")
        });
        Self {
            harness,
            active: true,
        }
    }

    fn finish(mut self) -> Result<(), String> {
        let result = self.harness.reset_scenario_state();
        if result.is_ok() {
            self.active = false;
        }
        result
    }
}

impl Drop for ScenarioStateGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            if let Err(error) = self.harness.reset_scenario_state() {
                eprintln!("core_session_e2e: scenario cleanup retry failed: {error}");
            }
        }
    }
}

fn run_scenario(
    harness: &FoundationHarness,
    id: &str,
    run: impl FnOnce() -> CaseEvidence,
) -> CaseEvidence {
    let started = std::time::Instant::now();
    eprintln!("core_session_e2e: starting {id}");
    let state_guard = ScenarioStateGuard::new(harness, id);
    let mut evidence = run();
    match state_guard.finish() {
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
    eprintln!(
        "core_session_e2e: finished {id} in {} ms (pass={})",
        started.elapsed().as_millis(),
        evidence.pass
    );
    evidence
}

#[test]
fn core_session_e2e_suite() {
    let harness = FoundationHarness::new_for_suite("core-session-orchestration")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let cases = vec![
        run_scenario(&harness, "E2E-01", || scenario_e2e_01::run(&harness)),
        run_scenario(&harness, "E2E-02", || scenario_e2e_02::run(&harness)),
        run_scenario(&harness, "E2E-03", || scenario_e2e_03::run(&harness)),
        run_scenario(&harness, "E2E-04", || scenario_e2e_04::run(&harness)),
        run_scenario(&harness, "E2E-05", || scenario_e2e_05::run(&harness)),
        run_scenario(&harness, "E2E-06", || scenario_e2e_06::run(&harness)),
        run_scenario(&harness, "E2E-07", || scenario_e2e_07::run(&harness)),
        run_scenario(&harness, "E2E-08", || scenario_e2e_08::run(&harness)),
        run_scenario(&harness, "E2E-09", || scenario_e2e_09::run(&harness)),
        run_scenario(&harness, "E2E-10", || scenario_e2e_10::run(&harness)),
        run_scenario(&harness, "E2E-11", || scenario_e2e_11::run(&harness)),
        run_scenario(&harness, "E2E-12", || scenario_e2e_12::run(&harness)),
        run_scenario(&harness, "E2E-13", || scenario_e2e_13::run(&harness)),
        run_scenario(&harness, "E2E-16", || scenario_e2e_16::run(&harness)),
        run_scenario(&harness, "E2E-19", || scenario_e2e_19::run(&harness)),
        run_scenario(&harness, "E2E-20", || scenario_e2e_20::run(&harness)),
        run_scenario(&harness, "E2E-21", || scenario_e2e_21::run(&harness)),
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

    if summary.metadata.fail_total != 0 {
        let failed_ids = summary
            .cases
            .iter()
            .filter(|case| !case.pass)
            .map(|case| case.id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        panic!(
            "core session E2E suite failed scenario(s): {failed_ids}; summary artifact: {}",
            harness.artifact_dir.join("summary.json").display()
        );
    }
}

fn assert_focused_scenario(
    suite_name: &str,
    id: &str,
    run: impl FnOnce(&FoundationHarness) -> CaseEvidence,
) {
    let harness = FoundationHarness::new_for_suite(suite_name)
        .unwrap_or_else(|error| panic!("{id} focused harness setup failed: {error}"));
    let evidence = run_scenario(&harness, id, || run(&harness));
    assert!(
        evidence.pass,
        "{id} focused reproduction failed: {:?}",
        evidence.assertions
    );
}

#[test]
fn affected_e2e08_auxiliary_lifecycle() {
    assert_focused_scenario("core-session-e2e-08-focused", "E2E-08", |harness| {
        scenario_e2e_08::run(harness)
    });
}

#[test]
fn affected_e2e10_remote_command_rendering() {
    assert_focused_scenario("core-session-e2e-10-focused", "E2E-10", |harness| {
        scenario_e2e_10::run(harness)
    });
}

#[test]
fn affected_e2e19_owned_interrupt_cleanup() {
    assert_focused_scenario("core-session-e2e-19-focused", "E2E-19", |harness| {
        scenario_e2e_19::run(harness)
    });
}

#[test]
fn e2e21_one_pane_slot_one_recovery() {
    let harness = FoundationHarness::new_for_suite("core-session-e2e-21")
        .unwrap_or_else(|error| panic!("E2E-21 focused harness setup failed: {error}"));
    let evidence = run_scenario(&harness, "E2E-21", || scenario_e2e_21::run(&harness));

    assert!(
        evidence.pass,
        "E2E-21 focused reproduction failed: {:?}",
        evidence.assertions
    );
}
