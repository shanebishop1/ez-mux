use super::super::SessionError;
use super::super::options::required_session_option;
use crate::config::{self, SessionRuntimeContext};
use crate::session::{
    RemoteModeContext, SharedServerAttachConfig, SlotMode, SlotModeLaunchContext,
};

#[derive(Debug, Clone)]
pub(super) struct RepairLaunchContext {
    pub(super) remote_path: Option<String>,
    pub(super) remote_server_url: Option<String>,
    pub(super) use_tssh: bool,
    pub(super) use_mosh: bool,
    pub(super) shared_server: Option<SharedServerAttachConfig>,
    pub(super) agent_command: Option<String>,
    pub(super) opencode_themes: config::OpencodeThemeRuntimeResolution,
}

pub(super) fn resolve_repair_launch_context(
    session_name: &str,
) -> Result<RepairLaunchContext, SessionError> {
    let context = super::super::remote_env::resolve_owned_session_runtime_context(session_name)?;
    Ok(repair_launch_context_from_session_context(context))
}

pub(super) fn repair_launch_context_from_session_context(
    context: SessionRuntimeContext,
) -> RepairLaunchContext {
    let remote_routing_active = context
        .remote_path
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && context
            .remote_server_url
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    let shared_server = if remote_routing_active {
        context
            .shared_server_url
            .as_ref()
            .map(|url| SharedServerAttachConfig { url: url.clone() })
    } else {
        None
    };

    RepairLaunchContext {
        remote_path: context.remote_path,
        remote_server_url: context.remote_server_url,
        use_tssh: context.use_tssh,
        use_mosh: context.use_mosh,
        shared_server,
        agent_command: context.agent_command,
        opencode_themes: config::OpencodeThemeRuntimeResolution {
            enabled: context.opencode_themes_enabled,
            themes_by_slot: context.opencode_themes_by_slot,
        },
    }
}

pub(super) fn restore_recreated_slot_modes(
    session_name: &str,
    recreated_slots: &[u8],
    launch_context: &RepairLaunchContext,
) -> Result<(), SessionError> {
    for slot_id in recreated_slots {
        let mode_value =
            required_session_option(session_name, &format!("@ezm_slot_{slot_id}_mode"))?;
        let mode = parse_slot_mode_label(*slot_id, &mode_value);
        let remote_context = RemoteModeContext {
            remote_path: launch_context.remote_path.as_deref(),
            remote_server_url: launch_context.remote_server_url.as_deref(),
            use_tssh: launch_context.use_tssh,
            use_mosh: launch_context.use_mosh,
        };
        let slot_launch_context = SlotModeLaunchContext {
            remote_context,
            shared_server: launch_context.shared_server.as_ref(),
            agent_command: launch_context.agent_command.as_deref(),
            opencode_theme: launch_context.opencode_themes.theme_for_slot(*slot_id),
        };
        super::super::mode_runtime::switch_slot_mode_for_repair(
            session_name,
            *slot_id,
            mode,
            slot_launch_context,
        )?;
    }

    Ok(())
}

pub(super) fn parse_slot_mode_label(slot_id: u8, value: &str) -> SlotMode {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "agent" | "opencode" | "claude" => SlotMode::Agent,
        "shell" | "sh" | "bash" | "zsh" | "fish" | "ubuntu" => SlotMode::Shell,
        "neovim" | "nvim" => SlotMode::Neovim,
        "lazygit" => SlotMode::Lazygit,
        _ => {
            eprintln!(
                "warning: slot {slot_id} has unknown mode metadata value `{value}`; defaulting to agent"
            );
            SlotMode::Agent
        }
    }
}
