use std::time::Duration;

use crate::support::foundation_harness::FoundationHarness;

use super::evidence::CaseEvidence;
use super::foundation_support::{map_settle, sample};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let success = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-18 success invocation failed: {error}"));
    samples.push(sample(&[], &success));

    let usage_failure = harness
        .run_ezm(&["unknown-subcommand"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-18 usage failure invocation failed: {error}"));
    samples.push(sample(&["unknown-subcommand"], &usage_failure));

    let runtime_failure = harness
        .run_ezm(&["logs", "open-latest"], &[], 17)
        .unwrap_or_else(|error| panic!("E2E-18 runtime failure invocation failed: {error}"));
    samples.push(sample(&["logs", "open-latest"], &runtime_failure));

    assertions.push(format!(
        "success exit code is 0: {}",
        success.exit_code == 0
    ));
    assertions.push(format!(
        "success stdout remains empty by default: {}",
        success.stdout.trim().is_empty()
    ));
    assertions.push(format!(
        "success stderr omits active-log banner by default: {}",
        !success.stderr.contains("active log file:")
    ));

    assertions.push(format!(
        "usage failure exit code is 2: {}",
        usage_failure.exit_code == 2
    ));
    assertions.push(format!(
        "usage failure stdout empty: {}",
        usage_failure.stdout.trim().is_empty()
    ));
    assertions.push(format!(
        "usage failure stderr contains clap error: {}",
        usage_failure.stderr.contains("error:")
    ));

    assertions.push(format!(
        "runtime failure exit code is 1: {}",
        runtime_failure.exit_code == 1
    ));
    assertions.push(format!(
        "runtime failure stdout empty: {}",
        runtime_failure.stdout.trim().is_empty()
    ));
    assertions.push(format!(
        "runtime failure stderr contains open error: {}",
        runtime_failure.stderr.contains("failed opening log file")
    ));

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-18 settle evidence failed: {error}"));

    let pass = success.exit_code == 0
        && success.stdout.trim().is_empty()
        && !success.stderr.contains("active log file:")
        && usage_failure.exit_code == 2
        && usage_failure.stdout.trim().is_empty()
        && usage_failure.stderr.contains("error:")
        && runtime_failure.exit_code == 1
        && runtime_failure.stdout.trim().is_empty()
        && runtime_failure.stderr.contains("failed opening log file")
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-18"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
    }
}
