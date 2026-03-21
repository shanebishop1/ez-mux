use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, RemotePathEvidence, SessionSnapshot, create_remote_remap_fixture,
    extract_stdout_field, map_settle, prepare_fresh_create_path, read_slot_snapshot, sample,
    settle_snapshot,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let fixture = create_remote_remap_fixture(harness)
        .unwrap_or_else(|error| panic!("E2E-10 remote fixture setup failed: {error}"));
    let remote_prefix = fixture.remote_prefix.display().to_string();
    let expected_mapped_path = fixture.expected_mapped_path.display().to_string();
    let expected_operator = String::from("e2e-operator");

    let expected_session = prepare_fresh_create_path(harness, &fixture.project_dir)
        .unwrap_or_else(|error| panic!("E2E-10 setup failed: {error}"));

    let launch = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &[],
            &[
                ("OPENCODE_REMOTE_DIR_PREFIX", &remote_prefix),
                ("OPERATOR", &expected_operator),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let effective_mapped_path =
        extract_stdout_field(&launch.stdout, "remote_project_dir").unwrap_or_default();
    let effective_remote_dir_prefix =
        extract_stdout_field(&launch.stdout, "remote_dir_prefix").unwrap_or_default();
    let remote_dir_prefix_source =
        extract_stdout_field(&launch.stdout, "remote_dir_prefix_source").unwrap_or_default();
    let opencode_attach_url =
        extract_stdout_field(&launch.stdout, "opencode_attach_url").unwrap_or_default();
    let opencode_server_url_source =
        extract_stdout_field(&launch.stdout, "opencode_server_url_source").unwrap_or_default();
    let opencode_server_host =
        extract_stdout_field(&launch.stdout, "opencode_server_host").unwrap_or_default();
    let opencode_server_host_source =
        extract_stdout_field(&launch.stdout, "opencode_server_host_source").unwrap_or_default();
    let opencode_server_port =
        extract_stdout_field(&launch.stdout, "opencode_server_port").unwrap_or_default();
    let opencode_server_port_source =
        extract_stdout_field(&launch.stdout, "opencode_server_port_source").unwrap_or_default();
    let opencode_server_password_set =
        extract_stdout_field(&launch.stdout, "opencode_server_password_set")
            .is_some_and(|value| value == "true");
    let opencode_server_password_source =
        extract_stdout_field(&launch.stdout, "opencode_server_password_source").unwrap_or_default();

    let expected_attach_url = String::from("http://127.0.0.1:4096");
    let remap_applied = effective_mapped_path == expected_mapped_path;
    let remote_prefix_source_is_env = remote_dir_prefix_source == "env";
    let remote_prefix_matches = effective_remote_dir_prefix == remote_prefix;
    let attach_url_matches_default = opencode_attach_url == expected_attach_url;
    let server_url_source_is_default = opencode_server_url_source == "default";
    let server_host_matches_default = opencode_server_host == "127.0.0.1";
    let server_host_source_is_default = opencode_server_host_source == "default";
    let server_port_matches_default = opencode_server_port == "4096";
    let server_port_source_is_default = opencode_server_port_source == "default";
    let password_source_is_default = opencode_server_password_source == "default";

    let shell_switch_args = vec![
        "__internal",
        "mode",
        "--session",
        &session,
        "--slot",
        "3",
        "--mode",
        "shell",
    ];
    let shell_switch_success = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &shell_switch_args,
            &[
                ("OPENCODE_REMOTE_DIR_PREFIX", &remote_prefix),
                ("OPERATOR", &expected_operator),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 shell switch failed to execute: {error}"));
    samples.push(sample(&shell_switch_args, &shell_switch_success));

    let slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-10 failed reading slot snapshot: {error}"));
    let slot_three_pane = slots
        .iter()
        .find(|slot| slot.slot_id == 3)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    let pane_start_command = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &slot_three_pane,
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-10 failed reading pane start command: {error}"));

    let agent_switch_args = vec![
        "__internal",
        "mode",
        "--session",
        &session,
        "--slot",
        "4",
        "--mode",
        "agent",
    ];
    let agent_switch_success = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &agent_switch_args,
            &[("OPENCODE_REMOTE_DIR_PREFIX", &remote_prefix)],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 agent switch failed to execute: {error}"));
    samples.push(sample(&agent_switch_args, &agent_switch_success));

    let slots_after_agent = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-10 failed reading post-agent slot snapshot: {error}"));
    let slot_four_pane = slots_after_agent
        .iter()
        .find(|slot| slot.slot_id == 4)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    let agent_pane_start_command = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &slot_four_pane,
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-10 failed reading agent pane start command: {error}"));

    let switch_fail = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &shell_switch_args,
            &[("OPENCODE_REMOTE_DIR_PREFIX", &remote_prefix)],
            0,
        )
        .unwrap_or_else(|error| {
            panic!("E2E-10 missing-operator branch failed to execute: {error}")
        });
    samples.push(sample(&shell_switch_args, &switch_fail));

    let fail_fast_non_zero = switch_fail.exit_code != 0;
    let fail_fast_diagnostic = switch_fail
        .stderr
        .contains("remote-prefix routing requires OPERATOR to be set");
    let shell_operator_matches = pane_start_command.contains(&expected_operator);
    let shell_remote_dir_matches = pane_start_command.contains(&expected_mapped_path);
    let agent_attach_url_matches = agent_pane_start_command.contains(&expected_attach_url);
    let agent_attach_dir_matches = agent_pane_start_command.contains(&expected_mapped_path);
    let agent_attach_uses_opencode = agent_pane_start_command.contains("opencode attach");
    let agent_password_flag_absent = !agent_pane_start_command.contains("--password");

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "launch effective mapped path = {effective_mapped_path}"
    ));
    assertions.push(format!(
        "launch remote prefix = {effective_remote_dir_prefix} (source={remote_dir_prefix_source})"
    ));
    assertions.push(format!(
        "launch attach url = {opencode_attach_url} (url_source={opencode_server_url_source})"
    ));
    assertions.push(format!(
        "launch server host = {opencode_server_host} (source={opencode_server_host_source})"
    ));
    assertions.push(format!(
        "launch server port = {opencode_server_port} (source={opencode_server_port_source})"
    ));
    assertions.push(format!(
        "launch password configured flag = {opencode_server_password_set} (source={opencode_server_password_source})"
    ));
    assertions.push(format!(
        "shell success branch mode switch exit_code = {}",
        shell_switch_success.exit_code
    ));
    assertions.push(format!(
        "shell success branch pane start command = {}",
        pane_start_command.trim()
    ));
    assertions.push(format!(
        "shell success branch effective operator token present in pane start command = {shell_operator_matches}"
    ));
    assertions.push(format!(
        "shell success branch effective remote dir token present in pane start command = {shell_remote_dir_matches}"
    ));
    assertions.push(format!(
        "shell success branch expected remote dir = {expected_mapped_path}"
    ));
    assertions.push(format!(
        "shell success branch effective operator matches configured operator = {shell_operator_matches}"
    ));
    assertions.push(format!(
        "shell success branch effective remote dir matches mapped path = {shell_remote_dir_matches}"
    ));
    assertions.push(format!(
        "agent success branch mode switch exit_code = {}",
        agent_switch_success.exit_code
    ));
    assertions.push(format!(
        "agent success branch pane start command = {}",
        agent_pane_start_command.trim()
    ));
    assertions.push(format!(
        "agent success branch includes opencode attach invocation = {agent_attach_uses_opencode}"
    ));
    assertions.push(format!(
        "agent success branch effective attach url matches default resolution = {agent_attach_url_matches}"
    ));
    assertions.push(format!(
        "agent success branch effective attach dir matches mapped path = {agent_attach_dir_matches}"
    ));
    assertions.push(format!(
        "agent success branch omits password flag when password is unset = {agent_password_flag_absent}"
    ));
    assertions.push(format!(
        "fail-fast branch exit_code={} non_zero={fail_fast_non_zero}",
        switch_fail.exit_code
    ));
    assertions.push(format!(
        "fail-fast branch stderr diagnostic surfaced = {fail_fast_diagnostic}"
    ));

    let settle = settle_snapshot(harness, "E2E-10");
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && remap_applied
        && remote_prefix_source_is_env
        && remote_prefix_matches
        && attach_url_matches_default
        && server_url_source_is_default
        && server_host_matches_default
        && server_host_source_is_default
        && server_port_matches_default
        && server_port_source_is_default
        && !opencode_server_password_set
        && password_source_is_default
        && shell_switch_success.exit_code == 0
        && shell_operator_matches
        && shell_remote_dir_matches
        && agent_switch_success.exit_code == 0
        && agent_attach_uses_opencode
        && agent_attach_url_matches
        && agent_attach_dir_matches
        && agent_password_flag_absent
        && fail_fast_non_zero
        && fail_fast_diagnostic
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-10"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: session,
            exists: session_exists,
            count: session_count,
        },
        layout: None,
        slots: Some(slots_after_agent),
        remote_path: Some(RemotePathEvidence {
            local_project_dir: fixture.project_dir.display().to_string(),
            remote_prefix,
            remote_dir_prefix_source,
            expected_mapped_path,
            effective_mapped_path,
            remap_applied,
            opencode_attach_url,
            opencode_server_url_source,
            opencode_server_host,
            opencode_server_host_source,
            opencode_server_port,
            opencode_server_port_source,
            opencode_server_password_set,
            opencode_server_password_source,
        }),
        helper_state: None,
    }
}
