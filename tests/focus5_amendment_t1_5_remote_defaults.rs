#[path = "support/focus5_amendment_t1_1_red_support.rs"]
mod red_support;
mod support;

use std::fs;
use std::path::PathBuf;

use red_support::{extract_stdout_field, paths_equivalent, read_slot_snapshot};
use support::foundation_harness::FoundationHarness;

#[test]
fn t1_5_default_startup_is_local_first_and_omits_remote_only_diagnostics() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-5")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let routing_mode = extract_stdout_field(&launch.stdout, "routing_mode").unwrap_or_default();
    let remote_routing_active =
        extract_stdout_field(&launch.stdout, "remote_routing_active").unwrap_or_default();

    let remote_only_fields_suppressed = extract_stdout_field(&launch.stdout, "remote_project_dir")
        .is_none()
        && extract_stdout_field(&launch.stdout, "remote_dir_prefix").is_none()
        && extract_stdout_field(&launch.stdout, "opencode_attach_url").is_none()
        && extract_stdout_field(&launch.stdout, "opencode_server_host").is_none()
        && extract_stdout_field(&launch.stdout, "opencode_server_port").is_none();

    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot: {error}"));
    let slot1_pane = slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();
    let slot1_start_command = read_pane_start_command(&harness, &slot1_pane)
        .unwrap_or_else(|error| panic!("failed reading slot 1 pane start command: {error}"));
    let startup_uses_local_agent_launch = slot1_start_command.contains("opencode")
        && !slot1_start_command.contains("opencode attach");

    let evidence = vec![
        format!("startup_exit_code={}", launch.exit_code),
        format!("session={session}"),
        format!("launch_action={launch_action}"),
        format!("routing_mode={routing_mode}"),
        format!("remote_routing_active={remote_routing_active}"),
        format!("remote_only_fields_suppressed={remote_only_fields_suppressed}"),
        format!("slot1_start_command={slot1_start_command}"),
        format!("startup_uses_local_agent_launch={startup_uses_local_agent_launch}"),
    ];
    write_green_cluster_evidence(&harness, "t1-5-local-default", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.5 local-default evidence: {error}"));

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && launch_action == "create"
        && routing_mode == "local"
        && remote_routing_active == "false"
        && remote_only_fields_suppressed
        && startup_uses_local_agent_launch;

    assert!(
        pass,
        "T-1.5 local-first default diagnostics contract failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn t1_5_remote_diagnostics_emit_only_when_remote_routing_is_active() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-5")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let fixture = create_remote_fixture(&harness)
        .unwrap_or_else(|error| panic!("failed creating remote fixture: {error}"));
    let remote_prefix = fixture.remote_prefix.display().to_string();

    let launch = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &[],
            &[("OPENCODE_REMOTE_DIR_PREFIX", &remote_prefix)],
            0,
        )
        .unwrap_or_else(|error| panic!("remote routing launch failed: {error}"));

    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let routing_mode = extract_stdout_field(&launch.stdout, "routing_mode").unwrap_or_default();
    let remote_routing_active =
        extract_stdout_field(&launch.stdout, "remote_routing_active").unwrap_or_default();
    let remote_project_dir =
        extract_stdout_field(&launch.stdout, "remote_project_dir").unwrap_or_default();
    let remote_dir_prefix =
        extract_stdout_field(&launch.stdout, "remote_dir_prefix").unwrap_or_default();
    let remote_dir_prefix_source =
        extract_stdout_field(&launch.stdout, "remote_dir_prefix_source").unwrap_or_default();
    let remote_fields_present = !remote_project_dir.is_empty()
        && !remote_dir_prefix.is_empty()
        && extract_stdout_field(&launch.stdout, "opencode_attach_url").is_some();
    let mapped_path_matches_expected = paths_equivalent(
        &remote_project_dir,
        &fixture.expected_mapped_path.display().to_string(),
    );

    let evidence = vec![
        format!("startup_exit_code={}", launch.exit_code),
        format!("session={session}"),
        format!("launch_action={launch_action}"),
        format!("routing_mode={routing_mode}"),
        format!("remote_routing_active={remote_routing_active}"),
        format!("remote_project_dir={remote_project_dir}"),
        format!(
            "expected_remote_project_dir={}",
            fixture.expected_mapped_path.display()
        ),
        format!("remote_dir_prefix={remote_dir_prefix}"),
        format!("remote_dir_prefix_source={remote_dir_prefix_source}"),
        format!("remote_fields_present={remote_fields_present}"),
        format!("mapped_path_matches_expected={mapped_path_matches_expected}"),
    ];
    write_green_cluster_evidence(&harness, "t1-5-remote-diagnostics", &evidence).unwrap_or_else(
        |error| panic!("failed writing T-1.5 remote-diagnostics evidence: {error}"),
    );

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && launch_action == "create"
        && routing_mode == "remote"
        && remote_routing_active == "true"
        && remote_fields_present
        && mapped_path_matches_expected
        && remote_dir_prefix_source == "env";

    assert!(
        pass,
        "T-1.5 remote diagnostics gating contract failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn t1_5_routing_failures_surface_explicit_stderr_and_log_evidence() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-5")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let args = [
        "__internal",
        "mode",
        "--session",
        "ezm-test-session",
        "--slot",
        "4",
        "--mode",
        "shell",
    ];
    let fail = harness
        .run_ezm(&args, &[("OPENCODE_REMOTE_DIR_PREFIX", "/srv/remotes")], 0)
        .unwrap_or_else(|error| panic!("routing failure launch failed: {error}"));

    let active_log = extract_active_log_path(&fail.stderr).unwrap_or_default();
    let log_content = if active_log.is_empty() {
        String::new()
    } else {
        fs::read_to_string(&active_log)
            .unwrap_or_else(|error| panic!("failed reading launch log {active_log}: {error}"))
    };

    let stderr_has_explicit_routing_diagnostic = fail
        .stderr
        .contains("remote-prefix routing requires OPERATOR to be set");
    let log_has_failure_event = log_content.contains("event=launch-failure");
    let log_has_routing_diagnostic =
        log_content.contains("remote-prefix routing requires OPERATOR to be set");

    let evidence = vec![
        format!("exit_code={}", fail.exit_code),
        format!("stdout={}", fail.stdout.trim()),
        format!("stderr={}", fail.stderr.trim()),
        format!("active_log={active_log}"),
        format!("stderr_has_explicit_routing_diagnostic={stderr_has_explicit_routing_diagnostic}"),
        format!("log_has_failure_event={log_has_failure_event}"),
        format!("log_has_routing_diagnostic={log_has_routing_diagnostic}"),
    ];
    write_green_cluster_evidence(&harness, "t1-5-routing-failure-surfacing", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.5 routing-failure evidence: {error}"));

    let pass = fail.exit_code != 0
        && fail.stdout.trim().is_empty()
        && fail.stderr.contains("active log file:")
        && stderr_has_explicit_routing_diagnostic
        && !active_log.is_empty()
        && log_has_failure_event
        && log_has_routing_diagnostic;

    assert!(
        pass,
        "T-1.5 routing/connect failure surfacing contract failed:\n{}",
        evidence.join("\n")
    );
}

struct RemoteFixture {
    project_dir: PathBuf,
    remote_prefix: PathBuf,
    expected_mapped_path: PathBuf,
}

fn create_remote_fixture(harness: &FoundationHarness) -> Result<RemoteFixture, String> {
    let fixture_root = harness.work_dir().join("t1-5-remote-fixture");
    let repo_root = fixture_root.join("alpha");
    let project_dir = repo_root.join("worktrees").join("feature-x");
    let remote_prefix = std::env::temp_dir().join(format!("ezm-t1-5-remote-{}", harness.run_id));
    let expected_mapped_path = remote_prefix
        .join("alpha")
        .join("worktrees")
        .join("feature-x");

    if fixture_root.exists() {
        fs::remove_dir_all(&fixture_root).map_err(|error| {
            format!(
                "failed resetting remote fixture root {}: {error}",
                fixture_root.display()
            )
        })?;
    }
    if remote_prefix.exists() {
        fs::remove_dir_all(&remote_prefix).map_err(|error| {
            format!(
                "failed resetting remote fixture prefix {}: {error}",
                remote_prefix.display()
            )
        })?;
    }

    fs::create_dir_all(repo_root.join(".git")).map_err(|error| {
        format!(
            "failed creating fixture git root {}: {error}",
            repo_root.display()
        )
    })?;
    fs::create_dir_all(&project_dir).map_err(|error| {
        format!(
            "failed creating fixture project dir {}: {error}",
            project_dir.display()
        )
    })?;
    fs::create_dir_all(&expected_mapped_path).map_err(|error| {
        format!(
            "failed creating expected mapped path {}: {error}",
            expected_mapped_path.display()
        )
    })?;

    Ok(RemoteFixture {
        project_dir: project_dir
            .canonicalize()
            .map_err(|error| format!("failed canonicalizing fixture project: {error}"))?,
        remote_prefix: remote_prefix
            .canonicalize()
            .map_err(|error| format!("failed canonicalizing fixture prefix: {error}"))?,
        expected_mapped_path: expected_mapped_path
            .canonicalize()
            .map_err(|error| format!("failed canonicalizing expected mapped path: {error}"))?,
    })
}

fn read_pane_start_command(harness: &FoundationHarness, pane_id: &str) -> Result<String, String> {
    harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{pane_start_command}",
        ])
        .map(|value| value.trim().to_owned())
}

fn extract_active_log_path(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("active log file: "))
        .map(str::to_owned)
}

fn write_green_cluster_evidence(
    harness: &FoundationHarness,
    cluster: &str,
    evidence: &[String],
) -> Result<(), String> {
    let dir = harness.artifact_dir.join("triage-green");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed creating triage-green evidence directory: {error}"))?;
    fs::write(dir.join(format!("{cluster}.txt")), evidence.join("\n"))
        .map_err(|error| format!("failed writing triage-green evidence file: {error}"))
}
