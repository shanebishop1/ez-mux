use std::path::Path;
use std::time::Duration;

use crate::support::foundation_harness::FoundationHarness;

use super::evidence::CaseEvidence;
use super::foundation_support::{map_settle, sample};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let ezm_bin = harness.ezm_bin.display().to_string();
    assertions.push(format!("binary path discovered: {ezm_bin}"));
    let binary_exists = Path::new(&ezm_bin).exists();
    assertions.push(format!("binary exists: {binary_exists}"));

    let help = harness
        .run_ezm(&["--help"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-00 help invocation failed: {error}"));
    assertions.push(format!("--help exit code = {}", help.exit_code));
    assertions.push(format!(
        "help has Usage: {}",
        help.stdout.contains("Usage:")
    ));
    assertions.push(format!(
        "help has repair command: {}",
        help.stdout.contains("repair")
    ));
    assertions.push(format!(
        "help has logs command: {}",
        help.stdout.contains("logs")
    ));

    samples.push(sample(&["--help"], &help));

    let version = harness
        .run_ezm(&["--version"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-00 version invocation failed: {error}"));
    assertions.push(format!("--version exit code = {}", version.exit_code));
    assertions.push(format!(
        "version output contains `ezm`: {}",
        version.stdout.contains("ezm")
    ));
    samples.push(sample(&["--version"], &version));

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-00 settle evidence failed: {error}"));

    let pass = binary_exists
        && help.exit_code == 0
        && help.stdout.contains("Usage:")
        && help.stdout.contains("repair")
        && help.stdout.contains("logs")
        && version.exit_code == 0
        && version.stdout.contains("ezm")
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-00"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
    }
}
