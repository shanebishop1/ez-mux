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
    let remote_path = fixture.remote_prefix.display().to_string();
    let expected_mapped_path = fixture.expected_mapped_path.display().to_string();

    let expected_session = prepare_fresh_create_path(harness, &fixture.project_dir)
        .unwrap_or_else(|error| panic!("E2E-10 setup failed: {error}"));

    let launch = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &[],
            &[
                ("EZM_REMOTE_PATH", &remote_path),
                ("EZM_REMOTE_SERVER_URL", "https://shell.remote.example:7443"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let effective_mapped_path =
        extract_stdout_field(&launch.stdout, "remote_project_dir").unwrap_or_default();
    let effective_remote_path =
        extract_stdout_field(&launch.stdout, "remote_path").unwrap_or_default();
    let remote_path_source =
        extract_stdout_field(&launch.stdout, "remote_path_source").unwrap_or_default();
    let opencode_attach_url =
        extract_stdout_field(&launch.stdout, "opencode_attach_url").unwrap_or_default();
    let opencode_server_url_source =
        extract_stdout_field(&launch.stdout, "opencode_server_url_source").unwrap_or_default();
    let opencode_server_password_set =
        extract_stdout_field(&launch.stdout, "opencode_server_password_set")
            .is_some_and(|value| value == "true");
    let opencode_server_password_source =
        extract_stdout_field(&launch.stdout, "opencode_server_password_source").unwrap_or_default();

    let expected_attach_url = String::from("none");
    let remap_applied = effective_mapped_path == expected_mapped_path;
    let remote_path_source_is_env = remote_path_source == "env";
    let remote_path_matches = effective_remote_path == remote_path;
    let attach_url_matches_default = opencode_attach_url == expected_attach_url;
    let server_url_source_is_default = opencode_server_url_source == "default";
    let password_source_is_default = opencode_server_password_source == "default";

    let launch_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-10 failed reading launch slot snapshot: {error}"));
    let shell_slot_id = launch_slots
        .iter()
        .find(|slot| slot.worktree == fixture.project_dir.display().to_string())
        .map_or(3, |slot| slot.slot_id);
    let shell_slot = shell_slot_id.to_string();
    let agent_slot_id = if shell_slot_id == 4 { 5 } else { 4 };
    let agent_slot = agent_slot_id.to_string();

    let shell_switch_args = vec![
        "__internal",
        "mode",
        "--session",
        &session,
        "--slot",
        &shell_slot,
        "--mode",
        "shell",
    ];
    let mut shell_switch_success = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &shell_switch_args,
            &[
                ("EZM_REMOTE_PATH", &remote_path),
                ("EZM_REMOTE_SERVER_URL", "https://shell.remote.example:7443"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 shell switch failed to execute: {error}"));
    samples.push(sample(&shell_switch_args, &shell_switch_success));

    if shell_switch_success.exit_code != 0
        && shell_switch_success
            .stderr
            .contains("switch-slot-mode-verify")
    {
        let shell_switch_retry = harness
            .run_ezm_in_dir(
                &fixture.project_dir,
                &shell_switch_args,
                &[
                    ("EZM_REMOTE_PATH", &remote_path),
                    ("EZM_REMOTE_SERVER_URL", "https://shell.remote.example:7443"),
                ],
                0,
            )
            .unwrap_or_else(|error| panic!("E2E-10 shell switch retry failed: {error}"));
        samples.push(sample(&shell_switch_args, &shell_switch_retry));
        shell_switch_success = shell_switch_retry;
    }

    let slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-10 failed reading slot snapshot: {error}"));
    let shell_slot_pane = slots
        .iter()
        .find(|slot| slot.slot_id == shell_slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    let pane_start_command = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &shell_slot_pane,
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-10 failed reading pane start command: {error}"));

    let lazygit_switch_args = vec![
        "__internal",
        "mode",
        "--session",
        &session,
        "--slot",
        &shell_slot,
        "--mode",
        "lazygit",
    ];
    let lazygit_switch = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &lazygit_switch_args,
            &[
                ("EZM_REMOTE_PATH", &remote_path),
                ("EZM_REMOTE_SERVER_URL", "https://shell.remote.example:7443"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 lazygit switch failed to execute: {error}"));
    samples.push(sample(&lazygit_switch_args, &lazygit_switch));

    let slots_after_lazygit = read_slot_snapshot(harness, &session).unwrap_or_else(|error| {
        panic!("E2E-10 failed reading post-lazygit slot snapshot: {error}")
    });
    let lazygit_slot_pane = slots_after_lazygit
        .iter()
        .find(|slot| slot.slot_id == shell_slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    let lazygit_pane_start_command = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &lazygit_slot_pane,
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| {
            panic!("E2E-10 failed reading lazygit pane start command: {error}")
        });

    let neovim_switch_args = vec![
        "__internal",
        "mode",
        "--session",
        &session,
        "--slot",
        &shell_slot,
        "--mode",
        "neovim",
    ];
    let neovim_switch = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &neovim_switch_args,
            &[
                ("EZM_REMOTE_PATH", &remote_path),
                ("EZM_REMOTE_SERVER_URL", "https://shell.remote.example:7443"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 neovim switch failed to execute: {error}"));
    samples.push(sample(&neovim_switch_args, &neovim_switch));

    let slots_after_neovim = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-10 failed reading post-neovim slot snapshot: {error}"));
    let neovim_slot_pane = slots_after_neovim
        .iter()
        .find(|slot| slot.slot_id == shell_slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    let neovim_pane_start_command = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &neovim_slot_pane,
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-10 failed reading neovim pane start command: {error}"));

    let popup_open_args = vec![
        "__internal",
        "popup",
        "--session",
        &session,
        "--slot",
        &shell_slot,
    ];
    let popup_open = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &popup_open_args,
            &[
                ("EZM_REMOTE_PATH", &remote_path),
                ("EZM_REMOTE_SERVER_URL", "https://shell.remote.example:7443"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 popup open failed to execute: {error}"));
    samples.push(sample(&popup_open_args, &popup_open));

    let popup_session = format!("{session}__popup_slot_{shell_slot_id}");
    let popup_pane_start_command = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{popup_session}:0.0"),
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-10 failed reading popup pane start command: {error}"));

    let auxiliary_open_args = vec![
        "__internal",
        "auxiliary",
        "--session",
        &session,
        "--action",
        "open",
    ];
    let auxiliary_open = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &auxiliary_open_args,
            &[
                ("EZM_REMOTE_PATH", &remote_path),
                ("EZM_REMOTE_SERVER_URL", "https://shell.remote.example:7443"),
            ],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 auxiliary open failed to execute: {error}"));
    samples.push(sample(&auxiliary_open_args, &auxiliary_open));

    let auxiliary_pane_start_command = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:beads-viewer.0"),
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| {
            panic!("E2E-10 failed reading auxiliary pane start command: {error}")
        });

    let agent_switch_args = vec![
        "__internal",
        "mode",
        "--session",
        &session,
        "--slot",
        &agent_slot,
        "--mode",
        "agent",
    ];
    let agent_switch_success = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &agent_switch_args,
            &[("EZM_REMOTE_PATH", &remote_path)],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-10 agent switch failed to execute: {error}"));
    samples.push(sample(&agent_switch_args, &agent_switch_success));

    let slots_after_agent = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-10 failed reading post-agent slot snapshot: {error}"));
    let agent_slot_pane = slots_after_agent
        .iter()
        .find(|slot| slot.slot_id == agent_slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    let agent_pane_start_command = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &agent_slot_pane,
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-10 failed reading agent pane start command: {error}"));

    let shell_remote_dir_matches = pane_start_command.contains(&expected_mapped_path);
    let shell_uses_ssh_remote = pane_start_command.contains("ssh -tt")
        && pane_start_command.contains("shell.remote.example");
    let lazygit_uses_ssh_remote = lazygit_pane_start_command.contains("ssh -tt")
        && lazygit_pane_start_command.contains("shell.remote.example")
        && lazygit_pane_start_command.contains("lazygit");
    let lazygit_command_continues_to_shell = lazygit_pane_start_command
        .contains("lazygit; exit_code=$?;")
        && lazygit_pane_start_command.contains("; :; fi; fi;")
        && !lazygit_pane_start_command.contains("exit \\\"\\\\$exit_code\\\"");
    let neovim_uses_ssh_remote = neovim_pane_start_command.contains("ssh -tt")
        && neovim_pane_start_command.contains("shell.remote.example")
        && neovim_pane_start_command.contains("nvim");
    let popup_uses_ssh_remote = popup_pane_start_command.contains("ssh -tt")
        && popup_pane_start_command.contains("shell.remote.example")
        && popup_pane_start_command.contains(&expected_mapped_path);
    let auxiliary_uses_ssh_remote = auxiliary_pane_start_command.contains("if ssh -tt")
        && auxiliary_pane_start_command.contains("shell.remote.example")
        && auxiliary_pane_start_command.contains("command -v bv");
    let auxiliary_command_continues_to_shell = auxiliary_pane_start_command.contains("exec")
        && auxiliary_pane_start_command.contains("${SHELL:-/bin/sh}");
    let auxiliary_command_omits_beads_exports = !auxiliary_pane_start_command
        .contains("export BEADS_DIR=")
        && !auxiliary_pane_start_command.contains("export BEADS_DB=");
    let agent_attach_url_matches = !agent_pane_start_command.contains("opencode attach");
    let agent_launch_omits_attach_dir_flag = !agent_pane_start_command.contains("--dir");
    let agent_mode_avoids_opencode_attach = !agent_pane_start_command.contains("opencode attach");
    let agent_password_flag_absent = !agent_pane_start_command.contains("--password");

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "launch effective mapped path = {effective_mapped_path}"
    ));
    assertions.push(format!(
        "launch remote path = {effective_remote_path} (source={remote_path_source})"
    ));
    assertions.push(format!(
        "launch attach url = {opencode_attach_url} (url_source={opencode_server_url_source})"
    ));
    assertions.push(format!(
        "launch password configured flag = {opencode_server_password_set} (source={opencode_server_password_source})"
    ));
    assertions.push(format!(
        "shell success branch selected slot id = {shell_slot_id}"
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
        "shell success branch effective remote dir token present in pane start command = {shell_remote_dir_matches}"
    ));
    assertions.push(format!(
        "shell success branch expected remote dir = {expected_mapped_path}"
    ));
    assertions.push(format!(
        "shell success branch effective remote dir matches mapped path = {shell_remote_dir_matches}"
    ));
    assertions.push(format!(
        "shell success branch launch command routes via ssh remote target = {shell_uses_ssh_remote}"
    ));
    assertions.push(format!(
        "lazygit success branch mode switch exit_code = {}",
        lazygit_switch.exit_code
    ));
    assertions.push(format!(
        "lazygit success branch pane start command = {}",
        lazygit_pane_start_command.trim()
    ));
    assertions.push(format!(
        "lazygit success branch launch command routes via ssh remote target = {lazygit_uses_ssh_remote}"
    ));
    assertions.push(format!(
        "lazygit success branch launch command continues into remote login shell after non-zero tool exit = {lazygit_command_continues_to_shell}"
    ));
    assertions.push(format!(
        "neovim success branch mode switch exit_code = {}",
        neovim_switch.exit_code
    ));
    assertions.push(format!(
        "neovim success branch pane start command = {}",
        neovim_pane_start_command.trim()
    ));
    assertions.push(format!(
        "neovim success branch launch command routes via ssh remote target = {neovim_uses_ssh_remote}"
    ));
    assertions.push(format!(
        "popup success branch toggle exit_code = {}",
        popup_open.exit_code
    ));
    assertions.push(format!(
        "popup success branch pane start command = {}",
        popup_pane_start_command.trim()
    ));
    assertions.push(format!(
        "popup success branch launch command routes via ssh remote target = {popup_uses_ssh_remote}"
    ));
    assertions.push(format!(
        "auxiliary success branch open exit_code = {}",
        auxiliary_open.exit_code
    ));
    assertions.push(format!(
        "auxiliary success branch pane start command = {}",
        auxiliary_pane_start_command.trim()
    ));
    assertions.push(format!(
        "auxiliary success branch launch command routes via ssh remote target = {auxiliary_uses_ssh_remote}"
    ));
    assertions.push(format!(
        "auxiliary success branch launch command returns to shell context after bv exit = {auxiliary_command_continues_to_shell}"
    ));
    assertions.push(format!(
        "auxiliary success branch launch command omits local BEADS_* exports for remote ssh execution = {auxiliary_command_omits_beads_exports}"
    ));
    assertions.push(format!(
        "agent success branch selected slot id = {agent_slot_id}"
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
        "agent success branch avoids opencode attach invocation when shared server URL is unset = {agent_mode_avoids_opencode_attach}"
    ));
    assertions.push(format!(
        "agent success branch effective attach url matches default resolution = {agent_attach_url_matches}"
    ));
    assertions.push(format!(
        "agent success branch omits attach dir flag when shared server URL is unset = {agent_launch_omits_attach_dir_flag}"
    ));
    assertions.push(format!(
        "agent success branch omits password flag when password is unset = {agent_password_flag_absent}"
    ));
    let settle = settle_snapshot(harness, "E2E-10");
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && remap_applied
        && remote_path_source_is_env
        && remote_path_matches
        && attach_url_matches_default
        && server_url_source_is_default
        && !opencode_server_password_set
        && password_source_is_default
        && shell_switch_success.exit_code == 0
        && shell_remote_dir_matches
        && shell_uses_ssh_remote
        && lazygit_switch.exit_code == 0
        && lazygit_uses_ssh_remote
        && lazygit_command_continues_to_shell
        && neovim_switch.exit_code == 0
        && neovim_uses_ssh_remote
        && popup_open.exit_code == 0
        && popup_uses_ssh_remote
        && auxiliary_open.exit_code == 0
        && auxiliary_uses_ssh_remote
        && auxiliary_command_continues_to_shell
        && auxiliary_command_omits_beads_exports
        && agent_switch_success.exit_code == 0
        && agent_mode_avoids_opencode_attach
        && agent_attach_url_matches
        && agent_launch_omits_attach_dir_flag
        && agent_password_flag_absent
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
            remote_path,
            remote_path_source,
            expected_mapped_path,
            effective_mapped_path,
            remap_applied,
            opencode_attach_url,
            opencode_server_url_source,
            opencode_server_password_set,
            opencode_server_password_source,
        }),
        helper_state: None,
    }
}
