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
        && extract_stdout_field(&launch.stdout, "remote_path").is_none()
        && extract_stdout_field(&launch.stdout, "opencode_attach_url").is_none();

    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot: {error}"));
    let slot1_pane = slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();
    let slot1_start_command = read_pane_start_command(&harness, &slot1_pane)
        .unwrap_or_else(|error| panic!("failed reading slot 1 pane start command: {error}"));
    let startup_avoids_attach_invocation = !slot1_start_command.contains("opencode attach");

    let evidence = vec![
        format!("startup_exit_code={}", launch.exit_code),
        format!("session={session}"),
        format!("launch_action={launch_action}"),
        format!("routing_mode={routing_mode}"),
        format!("remote_routing_active={remote_routing_active}"),
        format!("remote_only_fields_suppressed={remote_only_fields_suppressed}"),
        format!("slot1_start_command={slot1_start_command}"),
        format!("startup_avoids_attach_invocation={startup_avoids_attach_invocation}"),
    ];
    write_green_cluster_evidence(&harness, "t1-5-local-default", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.5 local-default evidence: {error}"));

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && launch_action == "create"
        && routing_mode == "local"
        && remote_routing_active == "false"
        && remote_only_fields_suppressed
        && startup_avoids_attach_invocation;

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
            &[
                ("EZM_REMOTE_PATH", &remote_prefix),
                ("EZM_REMOTE_SERVER_URL", "https://shell.remote.example:7443"),
            ],
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
    let remote_path = extract_stdout_field(&launch.stdout, "remote_path").unwrap_or_default();
    let remote_path_source =
        extract_stdout_field(&launch.stdout, "remote_path_source").unwrap_or_default();
    let remote_fields_present = !remote_project_dir.is_empty()
        && !remote_path.is_empty()
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
        format!("remote_path={remote_path}"),
        format!("remote_path_source={remote_path_source}"),
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
        && remote_path_source == "env";

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
        "-v",
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
        .run_ezm(&args, &[("EZM_REMOTE_PATH", "/srv/remotes")], 0)
        .unwrap_or_else(|error| panic!("routing failure launch failed: {error}"));

    let active_log = extract_active_log_path(&fail.stderr).unwrap_or_default();
    let log_content = if active_log.is_empty() {
        String::new()
    } else {
        fs::read_to_string(&active_log)
            .unwrap_or_else(|error| panic!("failed reading launch log {active_log}: {error}"))
    };

    let stderr_omits_operator_requirement = !fail.stderr.contains("OPERATOR");
    let log_has_failure_event = log_content.contains("event=launch-failure");
    let log_omits_operator_requirement = !log_content.contains("OPERATOR");

    let evidence = vec![
        format!("exit_code={}", fail.exit_code),
        format!("stdout={}", fail.stdout.trim()),
        format!("stderr={}", fail.stderr.trim()),
        format!("active_log={active_log}"),
        format!("stderr_omits_operator_requirement={stderr_omits_operator_requirement}"),
        format!("log_has_failure_event={log_has_failure_event}"),
        format!("log_omits_operator_requirement={log_omits_operator_requirement}"),
    ];
    write_green_cluster_evidence(&harness, "t1-5-routing-failure-surfacing", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.5 routing-failure evidence: {error}"));

    let pass = fail.exit_code != 0
        && fail.stdout.trim().is_empty()
        && fail.stderr.contains("active log file:")
        && stderr_omits_operator_requirement
        && !active_log.is_empty()
        && log_has_failure_event
        && log_omits_operator_requirement;

    assert!(
        pass,
        "T-1.5 routing/connect failure surfacing contract failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn t1_5_tmux_runtime_env_sync_is_available_to_internal_mode_routing() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-5")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let fixture = create_remote_fixture(&harness)
        .unwrap_or_else(|error| panic!("failed creating remote fixture: {error}"));
    let remote_prefix = fixture.remote_prefix.display().to_string();
    let expected_attach_url = "http://devbox-ez-1:4096";
    let expected_password = "tmux-secret";

    let launch = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &[],
            &[
                ("EZM_REMOTE_PATH", &remote_prefix),
                ("EZM_REMOTE_SERVER_URL", "https://shell.remote.example:7443"),
                ("OPENCODE_SERVER_URL", expected_attach_url),
                ("OPENCODE_SERVER_PASSWORD", expected_password),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("runtime env sync launch failed: {error}"));

    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot: {error}"));
    let slot1_pane = slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    let tmux_remote_path = show_global_tmux_env_value(&harness, "EZM_REMOTE_PATH");
    let tmux_remote_server_url = show_global_tmux_env_value(&harness, "EZM_REMOTE_SERVER_URL");
    let tmux_server_url = show_global_tmux_env_value(&harness, "OPENCODE_SERVER_URL");
    let tmux_server_password = show_global_tmux_env_value(&harness, "OPENCODE_SERVER_PASSWORD");

    let ezm_bin = shell_single_quote(&harness.ezm_bin.display().to_string());
    let internal_mode_command = format!(
        "{ezm_bin} __internal mode --session {} --slot 1 --mode agent </dev/null >/dev/null 2>&1",
        shell_single_quote(&session)
    );
    let run_shell_status = harness
        .tmux_capture(&["run-shell", &internal_mode_command])
        .unwrap_or_else(|error| {
            panic!("failed running internal mode through tmux run-shell: {error}")
        });

    let slot1_start_command = read_pane_start_command(&harness, &slot1_pane)
        .unwrap_or_else(|error| panic!("failed reading slot1 pane start command: {error}"));
    let internal_routing_uses_synced_attach_url = slot1_start_command.contains("opencode attach")
        && slot1_start_command.contains(expected_attach_url);
    let internal_routing_uses_synced_password = slot1_start_command.contains("--password")
        && slot1_start_command.contains(expected_password);

    let evidence = vec![
        format!("launch_exit_code={}", launch.exit_code),
        format!("session={session}"),
        format!("tmux_EZM_REMOTE_PATH={tmux_remote_path:?}"),
        format!("tmux_EZM_REMOTE_SERVER_URL={tmux_remote_server_url:?}"),
        format!("tmux_OPENCODE_SERVER_URL={tmux_server_url:?}"),
        format!("tmux_OPENCODE_SERVER_PASSWORD={tmux_server_password:?}"),
        format!("run_shell_status={run_shell_status:?}"),
        format!("slot1_start_command={}", slot1_start_command.trim()),
        format!(
            "internal_routing_uses_synced_attach_url={internal_routing_uses_synced_attach_url}"
        ),
        format!("internal_routing_uses_synced_password={internal_routing_uses_synced_password}"),
    ];
    write_green_cluster_evidence(&harness, "t1-5-runtime-env-sync-routing", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.5 runtime-env-sync evidence: {error}"));

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && tmux_remote_path.as_deref() == Some(remote_prefix.as_str())
        && tmux_remote_server_url.as_deref() == Some("https://shell.remote.example:7443")
        && tmux_server_url.as_deref() == Some(expected_attach_url)
        && tmux_server_password.as_deref() == Some(expected_password)
        && run_shell_status.trim().is_empty()
        && internal_routing_uses_synced_attach_url
        && internal_routing_uses_synced_password;

    assert!(
        pass,
        "T-1.5 runtime tmux env sync routing contract failed:\n{}",
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

fn show_global_tmux_env_value(harness: &FoundationHarness, key: &str) -> Option<String> {
    let raw = harness
        .tmux_capture(&["show-environment", "-g", key])
        .ok()?
        .trim()
        .to_owned();
    let prefix = format!("{key}=");
    raw.strip_prefix(&prefix).map(str::to_owned)
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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
