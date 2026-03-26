use super::super::mode_adapter::{launch_tool_command, ModeToolFailurePolicy};
use super::command::{format_output_diagnostics, tmux_output, tmux_output_value, tmux_run};
use super::options::{
    required_pane_option, required_session_option, set_pane_option, set_session_option,
    show_session_option,
};
use super::slot_swap::validate_canonical_slot_registry;
use super::style::refresh_active_border_for_slot;
use super::SessionError;
use super::SlotMode;
use super::CANONICAL_SLOT_IDS;
use crate::session::{
    mode_launch_contract, resolve_remote_path, RemoteModeContext, SharedServerAttachConfig,
    TeardownHook,
};

pub(super) fn switch_slot_mode(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    remote_context: RemoteModeContext<'_>,
    shared_server: Option<&SharedServerAttachConfig>,
    opencode_theme: Option<&str>,
) -> Result<(), SessionError> {
    let startup = startup_mode_signal_present();
    switch_slot_mode_internal(
        session_name,
        slot_id,
        mode,
        remote_context,
        shared_server,
        opencode_theme,
        startup,
    )
}

pub(super) fn switch_slot_mode_for_repair(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    remote_context: RemoteModeContext<'_>,
    shared_server: Option<&SharedServerAttachConfig>,
    opencode_theme: Option<&str>,
) -> Result<(), SessionError> {
    switch_slot_mode_internal(
        session_name,
        slot_id,
        mode,
        remote_context,
        shared_server,
        opencode_theme,
        true,
    )
}

fn switch_slot_mode_internal(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    remote_context: RemoteModeContext<'_>,
    shared_server: Option<&SharedServerAttachConfig>,
    opencode_theme: Option<&str>,
    prefer_assigned_worktree_cwd: bool,
) -> Result<(), SessionError> {
    let startup_fast_path = use_startup_fast_path(prefer_assigned_worktree_cwd);

    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    if !startup_fast_path {
        validate_canonical_slot_registry(session_name)?;
    }

    let slot_pane_key = format!("@ezm_slot_{slot_id}_pane");
    let slot_worktree_key = format!("@ezm_slot_{slot_id}_worktree");
    let slot_cwd_key = format!("@ezm_slot_{slot_id}_cwd");
    let slot_mode_key = format!("@ezm_slot_{slot_id}_mode");

    let pane_id = required_session_option(session_name, &slot_pane_key)?;
    let worktree = required_session_option(session_name, &slot_worktree_key)?;
    let current_cwd = resolve_mode_switch_cwd(prefer_assigned_worktree_cwd, &worktree, || {
        capture_slot_cwd(session_name, slot_id, &pane_id, &slot_cwd_key, &worktree)
    })?;
    let previous = if startup_fast_path {
        None
    } else {
        Some(load_previous_mode_metadata(
            session_name,
            slot_id,
            &slot_cwd_key,
            &slot_mode_key,
            &pane_id,
        )?)
    };

    let contract = mode_launch_contract(mode);
    let launch_command = launch_command_for_mode(
        slot_id,
        mode,
        &contract.launch_command,
        &current_cwd,
        remote_context,
        shared_server,
        opencode_theme,
    )?;
    run_teardown_hooks(&pane_id, &contract.teardown_hooks)?;
    respawn_slot_mode(&pane_id, &current_cwd, &launch_command)?;

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
        if let Some(previous) = previous.as_ref() {
            return compensate_mode_metadata(
                session_name,
                slot_id,
                &slot_cwd_key,
                &slot_mode_key,
                &pane_id,
                previous,
                error,
            );
        }

        return Err(error);
    }

    if !startup_fast_path {
        if let Err(error) = verify_mode_metadata(
            session_name,
            slot_id,
            &slot_cwd_key,
            &slot_mode_key,
            &pane_id,
            &target,
        ) {
            let Some(previous) = previous.as_ref() else {
                return Err(error);
            };
            return compensate_mode_metadata(
                session_name,
                slot_id,
                &slot_cwd_key,
                &slot_mode_key,
                &pane_id,
                previous,
                error,
            );
        }
    }

    if !startup_fast_path {
        validate_canonical_slot_registry(session_name)?;
    }

    refresh_active_border_for_slot(session_name, slot_id)?;
    Ok(())
}

fn use_startup_fast_path(prefer_assigned_worktree_cwd: bool) -> bool {
    prefer_assigned_worktree_cwd
}

fn startup_mode_signal_present() -> bool {
    startup_mode_signal_enabled(std::env::var("EZM_STARTUP_SLOT_MODE").ok().as_deref())
}

fn startup_mode_signal_enabled(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .is_some_and(|value| matches!(value, "1" | "true" | "yes" | "on"))
}

fn load_previous_mode_metadata(
    session_name: &str,
    slot_id: u8,
    slot_cwd_key: &str,
    slot_mode_key: &str,
    pane_id: &str,
) -> Result<ModeMetadataState, SessionError> {
    let existing_mode = required_session_option(session_name, slot_mode_key)?;
    let existing_pane_cwd = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_cwd")?;
    let existing_pane_mode =
        required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_mode")?;
    let existing_pane_worktree =
        required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_worktree")?;
    let pane_slot_id = required_pane_option(session_name, slot_id, pane_id, "@ezm_slot_id")?;
    if pane_slot_id != slot_id.to_string() {
        return Err(SessionError::TmuxCommandFailed {
            command: format!("switch-slot-mode -t {session_name} --slot {slot_id}"),
            stderr: format!(
                "slot metadata mismatch: pane {pane_id} has @ezm_slot_id={pane_slot_id}"
            ),
        });
    }

    Ok(ModeMetadataState {
        session_cwd: required_session_option(session_name, slot_cwd_key)?,
        session_mode: existing_mode,
        pane_cwd: existing_pane_cwd,
        pane_mode: existing_pane_mode,
        pane_worktree: existing_pane_worktree,
    })
}

fn launch_command_for_mode(
    slot_id: u8,
    mode: SlotMode,
    launch_command: &str,
    cwd: &str,
    remote_context: RemoteModeContext<'_>,
    shared_server: Option<&SharedServerAttachConfig>,
    opencode_theme: Option<&str>,
) -> Result<String, SessionError> {
    match mode {
        SlotMode::Agent => match shared_server {
            Some(config) => launch_agent_attach_command(
                slot_id,
                cwd,
                remote_context.remote_path,
                config,
                opencode_theme,
            ),
            None => Ok(with_opencode_tui_config_env(
                launch_command.to_owned(),
                slot_id,
                opencode_theme,
            )),
        },
        SlotMode::Shell | SlotMode::Neovim | SlotMode::Lazygit => {
            launch_command_with_remote_dir_from_mapping(mode, launch_command, cwd, remote_context)
        }
    }
}

fn launch_agent_attach_command(
    slot_id: u8,
    cwd: &str,
    remote_path: Option<&str>,
    shared_server: &SharedServerAttachConfig,
    opencode_theme: Option<&str>,
) -> Result<String, SessionError> {
    let attach_url = shared_server.url.trim();
    if attach_url.is_empty() {
        return Err(SessionError::MissingSharedServerAttachConfig);
    }

    let attach_dir = resolve_remote_path(std::path::Path::new(cwd), remote_path)?.effective_path;
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

    let attach_invocation =
        with_opencode_tui_config_env(attach_invocation, slot_id, opencode_theme);

    Ok(launch_tool_command(
        "opencode",
        &attach_invocation,
        ModeToolFailurePolicy::ContinueToShell,
    ))
}

fn launch_command_with_remote_dir_from_mapping(
    mode: SlotMode,
    launch_command: &str,
    cwd: &str,
    remote_context: RemoteModeContext<'_>,
) -> Result<String, SessionError> {
    let resolved = resolve_remote_path(std::path::Path::new(cwd), remote_context.remote_path)?;

    if !resolved.remapped {
        return Ok(launch_command.to_owned());
    }

    if let Some(ssh_command) = ssh_wrapped_launch_command(
        mode,
        &resolved.effective_path.display().to_string(),
        launch_command,
        remote_context.remote_server_url,
    ) {
        return Ok(ssh_command);
    }

    let mut exports = vec![format!(
        "export EZM_REMOTE_DIR='{}'",
        escape_single_quotes(&resolved.effective_path.display().to_string())
    )];
    if let Some(server_url) = remote_context
        .remote_server_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        exports.push(format!(
            "export EZM_REMOTE_SERVER_URL='{}'",
            escape_single_quotes(server_url)
        ));
    }

    Ok(format!("{}; {launch_command}", exports.join("; ")))
}

fn ssh_wrapped_launch_command(
    mode: SlotMode,
    remote_dir: &str,
    launch_command: &str,
    remote_server_url: Option<&str>,
) -> Option<String> {
    if !matches!(mode, SlotMode::Shell | SlotMode::Neovim | SlotMode::Lazygit) {
        return None;
    }

    let server_url = remote_server_url
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let (target, port) = ssh_target_and_port(server_url);
    if target.is_empty() {
        return None;
    }

    let remote_script = format!(
        "cd '{}' && {launch_command}",
        escape_single_quotes(remote_dir)
    );
    let mut ssh_invocation = String::from("ssh -tt");
    if let Some(port) = port {
        ssh_invocation.push_str(&format!(" -p {port}"));
    }
    ssh_invocation.push_str(&format!(" '{}'", escape_single_quotes(&target)));
    ssh_invocation.push_str(&format!(" '{}'", escape_single_quotes(&remote_script)));
    Some(format!(
        "if {ssh_invocation}; then exit 0; fi; ssh_exit_code=$?; printf '%s\\n' \"ez-mux remote ssh launch failed with status $ssh_exit_code\" >&2; exec \"${{SHELL:-/bin/sh}}\" -l"
    ))
}

fn ssh_target_and_port(server_url: &str) -> (String, Option<u16>) {
    let normalized = normalize_ssh_authority(server_url);
    if normalized.is_empty() {
        return (String::new(), None);
    }

    parse_authority_host_and_port(normalized)
}

fn normalize_ssh_authority(server_url: &str) -> &str {
    let trimmed = server_url.trim();
    let without_scheme = trimmed
        .split_once("://")
        .map_or(trimmed, |(_, remainder)| remainder);

    without_scheme.split('/').next().unwrap_or_default().trim()
}

fn parse_authority_host_and_port(authority: &str) -> (String, Option<u16>) {
    if let Some((host, port)) = parse_bracketed_authority(authority) {
        return (host, port);
    }

    if let Some((host, port)) = authority.rsplit_once(':') {
        let parsed_port = port.parse::<u16>().ok();
        if !host.contains(':') && parsed_port.is_some() {
            return (host.to_owned(), parsed_port);
        }
    }

    (authority.to_owned(), None)
}

fn parse_bracketed_authority(authority: &str) -> Option<(String, Option<u16>)> {
    if !authority.starts_with('[') {
        return None;
    }

    let closing = authority.find(']')?;
    let host = authority[..=closing].to_owned();
    let remainder = authority[(closing + 1)..].trim();
    if remainder.is_empty() {
        return Some((host, None));
    }

    let port = remainder
        .strip_prefix(':')
        .and_then(|candidate| candidate.parse::<u16>().ok());
    Some((host, port))
}

#[cfg(test)]
mod tests;

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

fn resolve_mode_switch_cwd<F>(
    prefer_assigned_worktree_cwd: bool,
    assigned_worktree: &str,
    captured_cwd: F,
) -> Result<String, SessionError>
where
    F: FnOnce() -> Result<String, SessionError>,
{
    if prefer_assigned_worktree_cwd {
        return Ok(assigned_worktree.to_owned());
    }

    captured_cwd()
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

fn with_opencode_tui_config_env(
    command: String,
    slot_id: u8,
    opencode_theme: Option<&str>,
) -> String {
    let Some(theme) = opencode_theme
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return command;
    };

    match ensure_opencode_tui_config_directory(slot_id, theme) {
        Ok(directory) => {
            let directory_value = directory.display().to_string();
            let tui_config_path = directory.join("tui.json").display().to_string();
            format!(
                "export OPENCODE_CONFIG_DIR='{}'; export OPENCODE_TUI_CONFIG='{}'; export OPENCODE_TEST_MANAGED_CONFIG_DIR='{}'; {command}",
                escape_single_quotes(&directory_value),
                escape_single_quotes(&tui_config_path),
                escape_single_quotes(&directory_value)
            )
        }
        Err(source) => {
            eprintln!("warning: failed writing opencode tui config for slot {slot_id}: {source}");
            command
        }
    }
}

fn ensure_opencode_tui_config_directory(
    slot_id: u8,
    theme: &str,
) -> Result<std::path::PathBuf, std::io::Error> {
    let directory = std::env::temp_dir()
        .join("ez-mux")
        .join("opencode-tui")
        .join(format!("slot-{slot_id}"));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("tui.json");
    std::fs::write(path, render_opencode_tui_config(theme))?;
    Ok(directory)
}

fn render_opencode_tui_config(theme: &str) -> String {
    format!(
        "{{\n  \"$schema\": \"https://opencode.ai/tui.json\",\n  \"theme\": \"{}\"\n}}\n",
        escape_json_string(theme)
    )
}

fn escape_json_string(value: &str) -> String {
    use std::fmt::Write;

    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{000c}' => escaped.push_str("\\f"),
            c if c.is_control() => {
                let _ = write!(escaped, "\\u{:04x}", u32::from(c));
            }
            c => escaped.push(c),
        }
    }
    escaped
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
