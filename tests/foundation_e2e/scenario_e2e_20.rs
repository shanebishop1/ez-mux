use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::support::foundation_harness::{CmdOutput, FoundationHarness};

use super::evidence::CaseEvidence;
use super::foundation_support::{
    extract_remote_path_field, extract_session_name, extract_stdout_field, map_settle, sample,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let root = harness.work_dir().join("config-only");
    let project = root.join("config-project");
    fs::create_dir_all(&project).expect("E2E-20 project");

    let config_a = root.join("owner.toml");
    FoundationHarness::write_file(
        &config_a,
        "ezm_remote_path = \"/srv/config-remotes\"\nezm_remote_server_url = \"https://shell.config.example:7443\"\nezm_use_tssh = false\nezm_use_mosh = false\nperles_dir = \".config-perles\"\nperles_db = \"/srv/config-perles.db\"\n",
    )
    .unwrap_or_else(|error| panic!("E2E-20 failed preparing owner config: {error}"));
    let config_b = root.join("conflicting.toml");
    FoundationHarness::write_file(
        &config_b,
        "ezm_remote_path = \"/srv/contaminating-remotes\"\nezm_remote_server_url = \"https://shell.contaminating.example:7443\"\nezm_use_tssh = true\nezm_use_mosh = true\nperles_dir = \".other-perles\"\nperles_db = \"/srv/other-perles.db\"\n",
    )
    .unwrap_or_else(|error| panic!("E2E-20 failed preparing conflicting config: {error}"));

    let config_a_path = config_a.display().to_string();
    let config_b_path = config_b.display().to_string();
    let launch = harness
        .run_ezm_in_dir(
            &project,
            &["--verbose", "--no-worktrees"],
            &[("EZM_CONFIG", &config_a_path)],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-20 config-only launch failed: {error}"));
    let session = extract_session_name(&launch.stdout)
        .unwrap_or_else(|| panic!("E2E-20 launch missing session name: {}", launch.stdout));
    let expected_remote_dir = extract_stdout_field(&launch.stdout, "remote_project_dir")
        .unwrap_or_else(|| panic!("E2E-20 launch missing remote project directory"));

    let shell_command = run_config_only_internal(
        harness,
        &project,
        &config_b_path,
        &[
            "__internal",
            "mode",
            "--session",
            &session,
            "--slot",
            "1",
            "--mode",
            "shell",
        ],
        "mode switch",
    );
    let shell_start = slot_start_command(harness, &session, 1);

    let popup = run_config_only_internal(
        harness,
        &project,
        &config_b_path,
        &["__internal", "popup", "--session", &session, "--slot", "1"],
        "popup open",
    );
    let popup_session = format!("{session}__popup_slot_1");
    let popup_start = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{popup_session}:0.0"),
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-20 failed reading popup command: {error}"));

    let auxiliary = run_config_only_internal(
        harness,
        &project,
        &config_b_path,
        &[
            "__internal",
            "auxiliary",
            "--session",
            &session,
            "--action",
            "open",
        ],
        "auxiliary open",
    );
    let auxiliary_start = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:perles.0"),
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-20 failed reading auxiliary command: {error}"));

    let reopen = harness
        .run_ezm_in_dir(
            &project,
            &["--verbose", "--no-worktrees"],
            &[("EZM_CONFIG", &config_b_path)],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-20 config-only reopen failed: {error}"));
    let reopen_remote_path = extract_remote_path_field(&reopen.stdout);
    let reopen_remote_url = extract_stdout_field(&reopen.stdout, "ezm_remote_server_url");
    let reopen_path_source = extract_stdout_field(&reopen.stdout, "remote_path_source");
    let reopen_url_source = extract_stdout_field(&reopen.stdout, "ezm_remote_server_url_source");

    let shell_uses_owner = shell_start.contains("shell.config.example")
        && shell_start.contains(&expected_remote_dir)
        && !shell_start.contains("contaminating");
    let popup_uses_owner = popup_start.contains("shell.config.example")
        && popup_start.contains(&expected_remote_dir)
        && !popup_start.contains("contaminating");
    let auxiliary_uses_owner = auxiliary_start.contains("shell.config.example")
        && !auxiliary_start.contains("contaminating");
    let launch_uses_owner = launch.stdout.contains("remote_path_source=file")
        && launch.stdout.contains(&expected_remote_dir)
        && launch.stdout.contains("shell.config.example");
    let reopen_uses_owner = reopen_remote_path.as_deref() == Some("/srv/config-remotes")
        && reopen_remote_url.as_deref() == Some("https://shell.config.example:7443")
        && reopen_path_source.as_deref() == Some("session")
        && reopen_url_source.as_deref() == Some("session");

    let assertions = vec![
        format!("config-only launch exit code = {}", launch.exit_code),
        format!("config-only launch uses owner context = {launch_uses_owner}"),
        format!("mode uses owner context without env values = {shell_uses_owner}"),
        format!("popup uses owner context without env values = {popup_uses_owner}"),
        format!("auxiliary uses owner context without env values = {auxiliary_uses_owner}"),
        format!("reopen retains owning session context = {reopen_uses_owner}"),
        format!("conflicting config did not contaminate reopen = {reopen_uses_owner}"),
    ];
    let samples = vec![
        sample(&["--verbose", "--no-worktrees"], &launch),
        sample(&["__internal", "mode", "--mode", "shell"], &shell_command),
        sample(&["__internal", "popup", "--slot", "1"], &popup),
        sample(&["__internal", "auxiliary", "--action", "open"], &auxiliary),
        sample(&["--verbose", "--no-worktrees"], &reopen),
    ];
    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-20 settle evidence failed: {error}"));

    CaseEvidence {
        id: String::from("E2E-20"),
        pass: launch.exit_code == 0
            && shell_command.exit_code == 0
            && popup.exit_code == 0
            && auxiliary.exit_code == 0
            && reopen.exit_code == 0
            && launch_uses_owner
            && shell_uses_owner
            && popup_uses_owner
            && auxiliary_uses_owner
            && reopen_uses_owner
            && settle.stable,
        assertions,
        samples,
        settle: map_settle(settle),
    }
}

fn run_config_only_internal(
    harness: &FoundationHarness,
    project: &Path,
    config_path: &str,
    args: &[&str],
    operation: &str,
) -> CmdOutput {
    harness
        .run_ezm_in_dir(project, args, &[("EZM_CONFIG", config_path)], 0)
        .unwrap_or_else(|error| panic!("E2E-20 {operation} failed: {error}"))
}

fn slot_start_command(harness: &FoundationHarness, session: &str, slot: u8) -> String {
    let panes = harness
        .tmux_capture(&[
            "list-panes",
            "-t",
            session,
            "-F",
            "#{pane_id}|#{@ezm_slot_id}",
        ])
        .unwrap_or_else(|error| panic!("E2E-20 failed listing slot panes: {error}"));
    let pane_id = panes
        .lines()
        .find_map(|line| {
            let (pane_id, slot_id) = line.split_once('|')?;
            (slot_id.trim() == slot.to_string()).then_some(pane_id.trim())
        })
        .unwrap_or_else(|| panic!("E2E-20 slot {slot} pane was not found"));
    harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{pane_start_command}",
        ])
        .unwrap_or_else(|error| panic!("E2E-20 failed reading slot command: {error}"))
}
