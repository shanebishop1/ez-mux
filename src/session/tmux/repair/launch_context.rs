use super::super::SessionError;
use super::super::options::required_session_option;
use crate::config::{self, OperatingSystem, ProcessEnv};
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

pub(super) fn resolve_repair_launch_context() -> RepairLaunchContext {
    let env = ProcessEnv;
    let file_config = config::load_config(&env, OperatingSystem::current())
        .map(|loaded| loaded.values)
        .unwrap_or_default();
    let remote_runtime = config::resolve_remote_runtime(&env, &file_config).ok();
    let remote_path = remote_runtime
        .as_ref()
        .and_then(|runtime| runtime.remote_path.value.clone());
    let remote_server_url = remote_runtime
        .as_ref()
        .and_then(|runtime| runtime.remote_server_url.value.clone());
    let use_mosh = remote_runtime
        .as_ref()
        .is_some_and(|runtime| runtime.use_mosh.value);
    let use_tssh = remote_runtime
        .as_ref()
        .is_some_and(|runtime| runtime.use_tssh.value);
    let remote_routing_active = remote_path.is_some() && remote_server_url.is_some();
    let shared_server = if remote_routing_active {
        remote_runtime.as_ref().and_then(|runtime| {
            runtime
                .shared_server
                .url
                .value
                .as_ref()
                .map(|url| SharedServerAttachConfig {
                    url: url.clone(),
                    password: runtime.shared_server.password.value.clone(),
                })
        })
    } else {
        None
    };

    RepairLaunchContext {
        remote_path,
        remote_server_url,
        use_tssh,
        use_mosh,
        shared_server,
        agent_command: config::resolve_agent_command(&file_config),
        opencode_themes: config::resolve_opencode_theme_runtime(&file_config),
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
