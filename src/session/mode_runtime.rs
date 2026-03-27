use super::SessionError;
use super::SlotMode;
use super::TmuxClient;
use super::CANONICAL_SLOT_IDS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedServerAttachConfig {
    pub url: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemoteModeContext<'a> {
    pub remote_path: Option<&'a str>,
    pub remote_server_url: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotModeSwitchOutcome {
    pub session_name: String,
    pub slot_id: u8,
    pub mode: SlotMode,
}

/// Switches one canonical slot to a target runtime mode.
///
/// # Errors
/// Returns an error when the tmux backend cannot execute the switch.
pub fn switch_slot_mode(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    remote_context: RemoteModeContext<'_>,
    shared_server: Option<&SharedServerAttachConfig>,
    agent_command: Option<&str>,
    opencode_theme: Option<&str>,
    tmux: &impl TmuxClient,
) -> Result<SlotModeSwitchOutcome, SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    tmux.switch_slot_mode(
        session_name,
        slot_id,
        mode,
        remote_context,
        shared_server,
        agent_command,
        opencode_theme,
    )?;

    Ok(SlotModeSwitchOutcome {
        session_name: session_name.to_owned(),
        slot_id,
        mode,
    })
}
