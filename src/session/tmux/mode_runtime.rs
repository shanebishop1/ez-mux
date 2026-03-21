use super::super::mode_adapter::{ModeToolFailurePolicy, launch_tool_command};
use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::SlotMode;
use super::command::{format_output_diagnostics, tmux_output, tmux_output_value, tmux_run};
use super::options::{
    required_pane_option, required_session_option, set_pane_option, set_session_option,
    show_session_option,
};
use super::slot_swap::validate_canonical_slot_registry;
use super::style::refresh_active_border_for_slot;
use crate::session::{
    SharedServerAttachConfig, TeardownHook, mode_launch_contract,
    resolve_operator_identity_for_remote_prefix, resolve_remote_path,
};

pub(super) fn switch_slot_mode(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    operator: Option<&str>,
    remote_prefix: Option<&str>,
    shared_server: Option<&SharedServerAttachConfig>,
) -> Result<(), SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    if matches!(mode, SlotMode::Shell | SlotMode::Neovim | SlotMode::Lazygit) {
        resolve_operator_identity_for_remote_prefix(remote_prefix, operator)?;
    }

    validate_canonical_slot_registry(session_name)?;
    let slot_pane_key = format!("@ezm_slot_{slot_id}_pane");
    let slot_worktree_key = format!("@ezm_slot_{slot_id}_worktree");
    let slot_cwd_key = format!("@ezm_slot_{slot_id}_cwd");
    let slot_mode_key = format!("@ezm_slot_{slot_id}_mode");

    let pane_id = required_session_option(session_name, &slot_pane_key)?;
    let worktree = required_session_option(session_name, &slot_worktree_key)?;
    let current_cwd = capture_slot_cwd(session_name, slot_id, &pane_id, &slot_cwd_key, &worktree)?;
    let existing_mode = required_session_option(session_name, &slot_mode_key)?;
    let existing_pane_cwd = required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_cwd")?;
    let existing_pane_mode =
        required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_mode")?;
    let existing_pane_worktree =
        required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_worktree")?;
    let pane_slot_id = required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_id")?;
    if pane_slot_id != slot_id.to_string() {
        return Err(SessionError::TmuxCommandFailed {
            command: format!("switch-slot-mode -t {session_name} --slot {slot_id}"),
            stderr: format!(
                "slot metadata mismatch: pane {pane_id} has @ezm_slot_id={pane_slot_id}"
            ),
        });
    }

    let contract = mode_launch_contract(mode);
    let launch_command = launch_command_for_mode(
        mode,
        &contract.launch_command,
        &current_cwd,
        remote_prefix,
        operator,
        shared_server,
    )?;
    run_teardown_hooks(&pane_id, &contract.teardown_hooks)?;
    respawn_slot_mode(&pane_id, &current_cwd, &launch_command)?;

    let previous = ModeMetadataState {
        session_cwd: required_session_option(session_name, &slot_cwd_key)?,
        session_mode: existing_mode,
        pane_cwd: existing_pane_cwd,
        pane_mode: existing_pane_mode,
        pane_worktree: existing_pane_worktree,
    };
    let target = ModeMetadataState {
        session_cwd: current_cwd.clone(),
        session_mode: mode.label().to_owned(),
        pane_cwd: current_cwd,
        pane_mode: mode.label().to_owned(),
        pane_worktree: worktree,
    };

    if let Err(error) = apply_mode_metadata(
        session_name,
        &slot_cwd_key,
        &slot_mode_key,
        &pane_id,
        &target,
    ) {
        return compensate_mode_metadata(
            session_name,
            slot_id,
            &slot_cwd_key,
            &slot_mode_key,
            &pane_id,
            &previous,
            error,
        );
    }

    if let Err(error) = verify_mode_metadata(
        session_name,
        slot_id,
        &slot_cwd_key,
        &slot_mode_key,
        &pane_id,
        &target,
    ) {
        return compensate_mode_metadata(
            session_name,
            slot_id,
            &slot_cwd_key,
            &slot_mode_key,
            &pane_id,
            &previous,
            error,
        );
    }

    validate_canonical_slot_registry(session_name)?;
    refresh_active_border_for_slot(session_name, slot_id)?;
    Ok(())
}

fn launch_command_for_mode(
    mode: SlotMode,
    launch_command: &str,
    cwd: &str,
    remote_prefix: Option<&str>,
    operator: Option<&str>,
    shared_server: Option<&SharedServerAttachConfig>,
) -> Result<String, SessionError> {
    match mode {
        SlotMode::Agent => launch_agent_attach_command(cwd, remote_prefix, shared_server),
        SlotMode::Shell | SlotMode::Neovim | SlotMode::Lazygit => {
            launch_command_with_remote_dir_from_mapping(
                launch_command,
                cwd,
                remote_prefix,
                operator,
            )
        }
    }
}

fn launch_agent_attach_command(
    cwd: &str,
    remote_prefix: Option<&str>,
    shared_server: Option<&SharedServerAttachConfig>,
) -> Result<String, SessionError> {
    let shared_server = shared_server.ok_or(SessionError::MissingSharedServerAttachConfig)?;
    let attach_url = shared_server.url.trim();
    if attach_url.is_empty() {
        return Err(SessionError::MissingSharedServerAttachConfig);
    }

    let attach_dir = resolve_remote_path(std::path::Path::new(cwd), remote_prefix)?.effective_path;
    let attach_dir = attach_dir.display().to_string();

    let attach_invocation = if let Some(password) = shared_server
        .password
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        format!(
            "opencode attach '{}' --dir '{}' --password '{}'",
            escape_single_quotes(attach_url),
            escape_single_quotes(&attach_dir),
            escape_single_quotes(password)
        )
    } else {
        format!(
            "opencode attach '{}' --dir '{}'",
            escape_single_quotes(attach_url),
            escape_single_quotes(&attach_dir)
        )
    };

    Ok(launch_tool_command(
        "opencode",
        &attach_invocation,
        ModeToolFailurePolicy::ContinueToShell,
    ))
}

fn launch_command_with_remote_dir_from_mapping(
    launch_command: &str,
    cwd: &str,
    remote_prefix: Option<&str>,
    operator: Option<&str>,
) -> Result<String, SessionError> {
    let resolved = resolve_remote_path(std::path::Path::new(cwd), remote_prefix)?;

    if !resolved.remapped {
        return Ok(launch_command.to_owned());
    }

    let resolved_operator = resolve_operator_identity_for_remote_prefix(remote_prefix, operator)?;
    let resolved_operator =
        resolved_operator.ok_or(SessionError::MissingOperatorForRemotePrefix)?;

    Ok(format!(
        "export EZM_REMOTE_DIR='{}'; export OPERATOR='{}'; {launch_command}",
        escape_single_quotes(&resolved.effective_path.display().to_string()),
        escape_single_quotes(&resolved_operator)
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        launch_agent_attach_command, launch_command_for_mode,
        launch_command_with_remote_dir_from_mapping,
    };
    use crate::session::{SharedServerAttachConfig, SlotMode};

    #[test]
    fn remote_prefix_injects_ezm_remote_dir_export() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("alpha");
        let nested = repo_root.join("worktrees").join("feature-x");
        std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
        std::fs::create_dir_all(&nested).expect("create nested");

        let command = launch_command_with_remote_dir_from_mapping(
            "exec \"${SHELL:-/bin/sh}\" -l",
            &nested.display().to_string(),
            Some("/srv/remotes"),
            Some("alice"),
        )
        .expect("command should resolve");

        assert!(command.contains("EZM_REMOTE_DIR='/srv/remotes/alpha/worktrees/feature-x'"));
        assert!(command.contains("OPERATOR='alice'"));
    }

    #[test]
    fn missing_remote_mapping_keeps_original_launch_command() {
        let command = launch_command_with_remote_dir_from_mapping(
            "exec \"${SHELL:-/bin/sh}\" -l",
            "/tmp/local-only",
            None,
            None,
        )
        .expect("command should resolve");

        assert_eq!(command, "exec \"${SHELL:-/bin/sh}\" -l");
    }

    #[test]
    fn remote_prefix_without_operator_fails_fast() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("alpha");
        std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");

        let error = launch_command_with_remote_dir_from_mapping(
            "exec \"${SHELL:-/bin/sh}\" -l",
            &repo_root.display().to_string(),
            Some("/srv/remotes"),
            None,
        )
        .expect_err("missing operator should fail");

        assert!(
            error
                .to_string()
                .contains("remote-prefix routing requires OPERATOR")
        );
    }

    #[test]
    fn agent_mode_uses_shared_server_attach_url_and_mapped_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("alpha");
        let nested = repo_root.join("worktrees").join("feature-x");
        std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
        std::fs::create_dir_all(&nested).expect("create nested");

        let command = launch_agent_attach_command(
            &nested.display().to_string(),
            Some("/srv/remotes"),
            Some(&SharedServerAttachConfig {
                url: String::from("http://127.0.0.1:4096"),
                password: None,
            }),
        )
        .expect("agent command should resolve");

        assert!(command.contains("opencode attach 'http://127.0.0.1:4096'"));
        assert!(command.contains("--dir '/srv/remotes/alpha/worktrees/feature-x'"));
    }

    #[test]
    fn agent_mode_password_is_included_when_configured() {
        let command = launch_agent_attach_command(
            "/tmp/local-only",
            None,
            Some(&SharedServerAttachConfig {
                url: String::from("http://127.0.0.1:4096"),
                password: Some(String::from("secret-token")),
            }),
        )
        .expect("agent command should resolve");

        assert!(command.contains("--password 'secret-token'"));
    }

    #[test]
    fn agent_mode_requires_shared_server_attach_config() {
        let error = launch_agent_attach_command("/tmp/local-only", None, None)
            .expect_err("missing shared server config should fail");

        assert!(
            error
                .to_string()
                .contains("agent mode requires shared-server attach configuration")
        );
    }

    #[test]
    fn agent_mode_does_not_require_operator_for_remote_prefix_mapping() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("alpha");
        std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");

        let command = launch_command_for_mode(
            SlotMode::Agent,
            "placeholder",
            &repo_root.display().to_string(),
            Some("/srv/remotes"),
            None,
            Some(&SharedServerAttachConfig {
                url: String::from("http://127.0.0.1:4096"),
                password: None,
            }),
        )
        .expect("agent mode should not require operator");

        assert!(command.contains("opencode attach 'http://127.0.0.1:4096'"));
    }
}

fn capture_slot_cwd(
    session_name: &str,
    slot_id: u8,
    pane_id: &str,
    slot_cwd_key: &str,
    fallback_worktree: &str,
) -> Result<String, SessionError> {
    let pane_path = tmux_output_value(&[
        "display-message",
        "-p",
        "-t",
        pane_id,
        "#{pane_current_path}",
    ])?;
    let pane_path = pane_path.trim();
    if !pane_path.is_empty() {
        return Ok(pane_path.to_owned());
    }

    if let Some(existing) = show_session_option(session_name, slot_cwd_key)? {
        if !existing.trim().is_empty() {
            return Ok(existing.trim().to_owned());
        }
    }

    if !fallback_worktree.trim().is_empty() {
        return Ok(fallback_worktree.to_owned());
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("capture-slot-cwd -t {session_name} --slot {slot_id}"),
        stderr: String::from("slot cwd capture returned empty path"),
    })
}

fn run_teardown_hooks(pane_id: &str, hooks: &[TeardownHook]) -> Result<(), SessionError> {
    for hook in hooks {
        match hook {
            TeardownHook::SendCtrlC => {
                tmux_run(&["send-keys", "-t", pane_id, "C-c"])?;
            }
        }
    }

    Ok(())
}

fn respawn_slot_mode(pane_id: &str, cwd: &str, launch_command: &str) -> Result<(), SessionError> {
    let shell_command = format!("sh -lc '{}'", escape_single_quotes(launch_command));
    let args = [
        "respawn-pane",
        "-k",
        "-t",
        pane_id,
        "-c",
        cwd,
        &shell_command,
    ];
    let output = tmux_output(&args)?;
    if output.status.success() {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("respawn-pane -k -t {pane_id} -c {cwd} <mode-launch-command>"),
        stderr: format_output_diagnostics(&output),
    })
}

fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

#[derive(Debug, Clone)]
struct ModeMetadataState {
    session_cwd: String,
    session_mode: String,
    pane_cwd: String,
    pane_mode: String,
    pane_worktree: String,
}

fn apply_mode_metadata(
    session_name: &str,
    slot_cwd_key: &str,
    slot_mode_key: &str,
    pane_id: &str,
    state: &ModeMetadataState,
) -> Result<(), SessionError> {
    set_session_option(session_name, slot_cwd_key, &state.session_cwd)?;
    set_session_option(session_name, slot_mode_key, &state.session_mode)?;
    set_pane_option(pane_id, "@ezm_slot_cwd", &state.pane_cwd)?;
    set_pane_option(pane_id, "@ezm_slot_mode", &state.pane_mode)?;
    set_pane_option(pane_id, "@ezm_slot_worktree", &state.pane_worktree)
}

fn verify_mode_metadata(
    session_name: &str,
    slot_id: u8,
    slot_cwd_key: &str,
    slot_mode_key: &str,
    pane_id: &str,
    expected: &ModeMetadataState,
) -> Result<(), SessionError> {
    let session_cwd = required_session_option(session_name, slot_cwd_key)?;
    let session_mode = required_session_option(session_name, slot_mode_key)?;
    let pane_cwd = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_cwd")?;
    let pane_mode = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_mode")?;
    let pane_worktree = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_worktree")?;

    if session_cwd == expected.session_cwd
        && session_mode == expected.session_mode
        && pane_cwd == expected.pane_cwd
        && pane_mode == expected.pane_mode
        && pane_worktree == expected.pane_worktree
    {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("switch-slot-mode-verify -t {session_name} --slot {slot_id}"),
        stderr: format!(
            "metadata verification failed: expected session_cwd={:?} session_mode={:?} pane_cwd={:?} pane_mode={:?} pane_worktree={:?}; got session_cwd={:?} session_mode={:?} pane_cwd={:?} pane_mode={:?} pane_worktree={:?}",
            expected.session_cwd,
            expected.session_mode,
            expected.pane_cwd,
            expected.pane_mode,
            expected.pane_worktree,
            session_cwd,
            session_mode,
            pane_cwd,
            pane_mode,
            pane_worktree
        ),
    })
}

fn compensate_mode_metadata(
    session_name: &str,
    slot_id: u8,
    slot_cwd_key: &str,
    slot_mode_key: &str,
    pane_id: &str,
    previous: &ModeMetadataState,
    original_error: SessionError,
) -> Result<(), SessionError> {
    match apply_mode_metadata(session_name, slot_cwd_key, slot_mode_key, pane_id, previous) {
        Ok(()) => Err(original_error),
        Err(compensation_error) => Err(SessionError::TmuxCommandFailed {
            command: format!("switch-slot-mode-compensate -t {session_name} --slot {slot_id}"),
            stderr: format!(
                "mode switch failed: {original_error}; rollback failed: {compensation_error}"
            ),
        }),
    }
}
