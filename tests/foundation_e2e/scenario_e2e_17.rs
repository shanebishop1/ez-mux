use std::fs;
use std::time::Duration;

use crate::support::foundation_harness::FoundationHarness;

use super::evidence::CaseEvidence;
use super::foundation_support::{extract_remote_path_source, map_settle, sample};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let env_project = harness.work_dir().join("precedence").join("env-project");
    let file_project = harness.work_dir().join("precedence").join("file-project");
    let default_project = harness
        .work_dir()
        .join("precedence")
        .join("default-project");
    fs::create_dir_all(&env_project).expect("E2E-17 env project");
    fs::create_dir_all(&file_project).expect("E2E-17 file project");
    fs::create_dir_all(&default_project).expect("E2E-17 default project");

    let config_file = harness.work_dir().join("precedence").join("config.toml");
    FoundationHarness::write_file(
        &config_file,
        "ezm_remote_path = \"/srv/file-remotes\"\nezm_remote_server_url = \"https://shell.file.example:7443\"\n",
    )
    .unwrap_or_else(|error| panic!("E2E-17 failed preparing config file: {error}"));

    let config_path = config_file.display().to_string();

    let env_over_file = harness
        .run_ezm_in_dir(
            &env_project,
            &["--verbose"],
            &[
                ("EZM_CONFIG", &config_path),
                ("EZM_REMOTE_PATH", "/srv/env-remotes"),
                ("EZM_REMOTE_SERVER_URL", "https://shell.env.example:7443"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-17 env-over-file invocation failed: {error}"));
    samples.push(sample(&["--verbose"], &env_over_file));

    let file_over_default = harness
        .run_ezm_in_dir(
            &file_project,
            &["--verbose"],
            &[("EZM_CONFIG", &config_path)],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-17 file-over-default invocation failed: {error}"));
    samples.push(sample(&["--verbose"], &file_over_default));

    let default_only = harness
        .run_ezm_in_dir(&default_project, &["--verbose"], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-17 default invocation failed: {error}"));
    samples.push(sample(&["--verbose"], &default_only));

    let env_source = extract_remote_path_source(&env_over_file.stdout);
    let file_source = extract_remote_path_source(&file_over_default.stdout);
    let default_source = extract_remote_path_source(&default_only.stdout);

    assertions.push(format!("env over file resolved source: {env_source:?}"));
    assertions.push(format!(
        "file over default resolved source: {file_source:?}"
    ));
    assertions.push(format!("default-only resolved source: {default_source:?}"));

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-17 settle evidence failed: {error}"));

    let pass = env_over_file.exit_code == 0
        && file_over_default.exit_code == 0
        && default_only.exit_code == 0
        && env_source.as_deref() == Some("env")
        && file_source.as_deref() == Some("file")
        && default_source.is_none()
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-17"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
    }
}
