use super::super::super::mode_adapter::{ModeToolFailurePolicy, launch_tool_command};
use super::super::SessionError;
use super::super::SlotMode;
use super::opencode_theme::with_opencode_tui_config_env;
use super::remote_launch::{escape_single_quotes, launch_command_with_remote_dir_from_mapping};
use crate::session::{SharedServerAttachConfig, SlotModeLaunchContext, resolve_remote_path};

pub(super) fn launch_command_for_mode(
    slot_id: u8,
    mode: SlotMode,
    launch_command: &str,
    cwd: &str,
    launch_context: SlotModeLaunchContext<'_>,
) -> Result<String, SessionError> {
    let SlotModeLaunchContext {
        remote_context,
        shared_server,
        agent_command,
        opencode_theme,
    } = launch_context;

    match mode {
        SlotMode::Agent => {
            if let Some(command) = normalize_agent_command_override(agent_command) {
                return Ok(command.to_owned());
            }

            match shared_server {
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
            }
        }
        SlotMode::Shell | SlotMode::Neovim | SlotMode::Lazygit => {
            launch_command_with_remote_dir_from_mapping(mode, launch_command, cwd, remote_context)
        }
    }
}

pub(super) fn normalize_agent_command_override(agent_command: Option<&str>) -> Option<&str> {
    agent_command
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn launch_agent_attach_command(
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
