use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::support::foundation_harness::{CmdOutput, FoundationHarness};

use super::evidence::CaseEvidence;
use super::foundation_support::{
    expected_safe_log_root, extract_active_log_path, has_expected_log_name_shape, map_settle,
    sample,
};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let run_one = harness
        .run_ezm(&["--verbose"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-15 first launch failed: {error}"));
    let first_log = extract_active_log_path(&run_one.stderr)
        .unwrap_or_else(|| panic!("E2E-15 first run missing active log path in stderr"));

    let run_two = harness
        .run_ezm(&["--verbose"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-15 second launch failed: {error}"));
    let second_log = extract_active_log_path(&run_two.stderr)
        .unwrap_or_else(|| panic!("E2E-15 second run missing active log path in stderr"));

    samples.push(sample(&["--verbose"], &run_one));
    samples.push(sample(&["--verbose"], &run_two));

    assertions.push(format!("first active log: {first_log}"));
    assertions.push(format!("second active log: {second_log}"));
    assertions.push(format!(
        "active log paths differ: {}",
        first_log != second_log
    ));

    let first_name = Path::new(&first_log)
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
    let second_name = Path::new(&second_log)
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();

    let expected_root = expected_safe_log_root(harness);
    assertions.push(format!(
        "logs are in OS-safe root {}: {}",
        expected_root.display(),
        first_log.starts_with(&expected_root.display().to_string())
            && second_log.starts_with(&expected_root.display().to_string())
    ));
    assertions.push(format!(
        "first filename shape valid: {}",
        has_expected_log_name_shape(first_name)
    ));
    assertions.push(format!(
        "second filename shape valid: {}",
        has_expected_log_name_shape(second_name)
    ));

    let first_content = fs::read_to_string(&first_log)
        .unwrap_or_else(|error| panic!("E2E-15 failed reading first log file: {error}"));
    let second_content = fs::read_to_string(&second_log)
        .unwrap_or_else(|error| panic!("E2E-15 failed reading second log file: {error}"));
    assertions.push(format!(
        "first log has lifecycle entry: {}",
        first_content.contains("event=launch-log-created")
    ));
    assertions.push(format!(
        "second log has lifecycle entry: {}",
        second_content.contains("event=launch-log-created")
    ));

    let open_latest = harness
        .run_ezm(&["--verbose", "logs", "open-latest"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-15 open-latest failed: {error}"));
    samples.push(sample(&["--verbose", "logs", "open-latest"], &open_latest));

    let open_outcome = evaluate_open_latest(harness, &open_latest, &second_log);
    assertions.extend(open_outcome.assertions);

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-15 settle evidence failed: {error}"));

    let in_safe_root = first_log.starts_with(&expected_root.display().to_string())
        && second_log.starts_with(&expected_root.display().to_string());
    let pass = run_one.exit_code == 0
        && run_two.exit_code == 0
        && first_log != second_log
        && in_safe_root
        && has_expected_log_name_shape(first_name)
        && has_expected_log_name_shape(second_name)
        && first_content.contains("event=launch-log-created")
        && second_content.contains("event=launch-log-created")
        && open_outcome.passed
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-15"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
    }
}

struct OpenLatestOutcome {
    passed: bool,
    assertions: Vec<String>,
}

fn evaluate_open_latest(
    harness: &FoundationHarness,
    output: &CmdOutput,
    previous_log: &str,
) -> OpenLatestOutcome {
    let open_capture = fs::read_to_string(harness.open_capture_path())
        .unwrap_or_else(|error| panic!("E2E-15 opener capture was not written: {error}"));
    let latest_from_open = output
        .stdout
        .trim()
        .strip_prefix("opened latest log: ")
        .unwrap_or_default()
        .to_owned();
    let active_from_open = extract_active_log_path(&output.stderr).unwrap_or_default();

    let assertions = vec![
        format!("open-latest exit code = {}", output.exit_code),
        format!(
            "open-latest stdout reports path: {}",
            !latest_from_open.is_empty()
        ),
        format!(
            "open-latest path matches active log emission: {}",
            latest_from_open == active_from_open
        ),
        format!(
            "open-latest path lexicographically >= previous launch: {}",
            latest_from_open.as_str() >= previous_log
        ),
        format!(
            "opener received opened log path: {}",
            open_capture == latest_from_open
        ),
    ];

    OpenLatestOutcome {
        passed: output.exit_code == 0
            && latest_from_open == active_from_open
            && latest_from_open.as_str() >= previous_log
            && open_capture == latest_from_open,
        assertions,
    }
}
