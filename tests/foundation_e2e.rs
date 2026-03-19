mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use support::foundation_harness::{CmdOutput, FoundationHarness, TmuxSettleEvidence};

const FOUNDATION_IDS: [&str; 4] = ["E2E-00", "E2E-15", "E2E-17", "E2E-18"];

#[derive(Serialize)]
struct RunMetadata {
    run_id: String,
    commit_sha: String,
    os: String,
    shell: String,
    tmux_version: String,
    artifact_dir: String,
    test_ids: Vec<String>,
    pass_total: usize,
    fail_total: usize,
}

#[derive(Serialize)]
struct CommandSample {
    args: Vec<String>,
    exit_code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Serialize)]
struct SettleEvidence {
    attempts: u32,
    poll_interval_ms: u64,
    timeout_ms: u64,
    stable: bool,
    sessions: String,
    windows: String,
    panes: String,
}

#[derive(Serialize)]
struct CaseEvidence {
    id: String,
    pass: bool,
    assertions: Vec<String>,
    samples: Vec<CommandSample>,
    settle: SettleEvidence,
}

#[derive(Serialize)]
struct SuiteEvidence {
    metadata: RunMetadata,
    cases: Vec<CaseEvidence>,
}

#[test]
fn foundation_e2e_suite() {
    let harness =
        FoundationHarness::new().unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let cases = vec![
        case_e2e_00(&harness),
        case_e2e_15(&harness),
        case_e2e_17(&harness),
        case_e2e_18(&harness),
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

fn case_e2e_00(harness: &FoundationHarness) -> CaseEvidence {
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

fn case_e2e_15(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let run_one = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-15 first launch failed: {error}"));
    let first_log = extract_active_log_path(&run_one.stderr)
        .unwrap_or_else(|| panic!("E2E-15 first run missing active log path in stderr"));

    let run_two = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-15 second launch failed: {error}"));
    let second_log = extract_active_log_path(&run_two.stderr)
        .unwrap_or_else(|| panic!("E2E-15 second run missing active log path in stderr"));

    samples.push(sample(&[], &run_one));
    samples.push(sample(&[], &run_two));

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

    let expected_root = harness.work_dir().join("state").join("ez-mux").join("logs");
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
        .run_ezm(&["logs", "open-latest"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-15 open-latest failed: {error}"));
    samples.push(sample(&["logs", "open-latest"], &open_latest));

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

fn case_e2e_17(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let config_file = harness.work_dir().join("precedence").join("config.toml");
    FoundationHarness::write_file(&config_file, "operator = \"file-operator\"\n")
        .unwrap_or_else(|error| panic!("E2E-17 failed preparing config file: {error}"));

    let config_path = config_file.display().to_string();

    let cli_over_env = harness
        .run_ezm(
            &["--operator", "cli-operator"],
            &[("EZM_CONFIG", &config_path), ("OPERATOR", "env-operator")],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-17 cli-over-env invocation failed: {error}"));
    samples.push(sample(&["--operator", "cli-operator"], &cli_over_env));

    let env_over_file = harness
        .run_ezm(
            &[],
            &[("EZM_CONFIG", &config_path), ("OPERATOR", "env-operator")],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-17 env-over-file invocation failed: {error}"));
    samples.push(sample(&[], &env_over_file));

    let file_over_default = harness
        .run_ezm(&[], &[("EZM_CONFIG", &config_path)], 0)
        .unwrap_or_else(|error| panic!("E2E-17 file-over-default invocation failed: {error}"));
    samples.push(sample(&[], &file_over_default));

    let default_only = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-17 default invocation failed: {error}"));
    samples.push(sample(&[], &default_only));

    let cli_source = extract_operator_source(&cli_over_env.stdout);
    let env_source = extract_operator_source(&env_over_file.stdout);
    let file_source = extract_operator_source(&file_over_default.stdout);
    let default_source = extract_operator_source(&default_only.stdout);

    assertions.push(format!("CLI over env resolved source: {cli_source:?}"));
    assertions.push(format!("env over file resolved source: {env_source:?}"));
    assertions.push(format!(
        "file over default resolved source: {file_source:?}"
    ));
    assertions.push(format!("default-only resolved source: {default_source:?}"));

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-17 settle evidence failed: {error}"));

    let pass = cli_over_env.exit_code == 0
        && env_over_file.exit_code == 0
        && file_over_default.exit_code == 0
        && default_only.exit_code == 0
        && cli_source.as_deref() == Some("cli")
        && env_source.as_deref() == Some("env")
        && file_source.as_deref() == Some("file")
        && default_source.as_deref() == Some("default")
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-17"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
    }
}

fn case_e2e_18(harness: &FoundationHarness) -> CaseEvidence {
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
        "success stdout has user message: {}",
        success.stdout.contains("contract locked")
    ));
    assertions.push(format!(
        "success stderr has diagnostics: {}",
        success.stderr.contains("active log file:")
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
        && success.stdout.contains("contract locked")
        && success.stderr.contains("active log file:")
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

fn sample(args: &[&str], output: &CmdOutput) -> CommandSample {
    CommandSample {
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        exit_code: output.exit_code,
        stdout: output.stdout.clone(),
        stderr: output.stderr.clone(),
    }
}

fn map_settle(settle: TmuxSettleEvidence) -> SettleEvidence {
    SettleEvidence {
        attempts: settle.attempts,
        poll_interval_ms: settle.poll_interval_ms,
        timeout_ms: settle.timeout_ms,
        stable: settle.stable,
        sessions: settle.sessions,
        windows: settle.windows,
        panes: settle.panes,
    }
}

fn extract_active_log_path(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("active log file: "))
        .map(str::to_owned)
}

fn has_expected_log_name_shape(name: &str) -> bool {
    if !Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
    {
        return false;
    }

    let base = &name[..name.len() - 4];
    if base.len() < 17 {
        return false;
    }

    let timestamp = &base[..15];
    if timestamp.as_bytes().get(8) != Some(&b'-') {
        return false;
    }

    for (index, byte) in timestamp.as_bytes().iter().enumerate() {
        if index == 8 {
            continue;
        }
        if !byte.is_ascii_digit() {
            return false;
        }
    }

    base.as_bytes().get(15) == Some(&b'-')
}

fn extract_operator_source(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find_map(|line| line.split("operator source=").nth(1))
        .and_then(|tail| tail.split('.').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn read_commit_sha(project_root: &Path) -> String {
    let output = std::process::Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir(project_root)
        .output();

    match output {
        Ok(result) if result.status.success() => {
            String::from_utf8_lossy(&result.stdout).trim().to_owned()
        }
        _ => String::from("unknown"),
    }
}

fn write_case_artifacts(dir: &Path, cases: &[CaseEvidence]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| format!("failed creating case directory: {error}"))?;
    for case in cases {
        let path = dir.join(format!("{}.json", case.id));
        write_json(&path, case)?;
    }
    Ok(())
}

fn write_json(path: &PathBuf, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed serializing json for {path:?}: {error}"))?;
    fs::write(path, json).map_err(|error| format!("failed writing json {path:?}: {error}"))
}
