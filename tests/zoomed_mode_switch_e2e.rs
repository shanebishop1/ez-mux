#![allow(dead_code)]

mod support;

#[path = "core_session_e2e/core_support.rs"]
mod core_support;
#[path = "core_session_e2e/scenario_e2e_20.rs"]
mod scenario_e2e_20;

use support::foundation_harness::FoundationHarness;

#[test]
fn zoomed_mode_switch_e2e() {
    let harness = FoundationHarness::new_for_suite("zoomed-mode-switch")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));
    let case = scenario_e2e_20::run(&harness);

    assert!(
        case.pass,
        "zoomed mode switch E2E failed; inspect {}",
        harness.artifact_dir.display()
    );
}
