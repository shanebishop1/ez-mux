use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::SlotMode;
use super::TmuxClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedServerAttachConfig {
    pub url: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SlotModeLaunchContext<'a> {
    pub remote_context: RemoteModeContext<'a>,
    pub shared_server: Option<&'a SharedServerAttachConfig>,
    pub agent_command: Option<&'a str>,
    pub opencode_theme: Option<&'a str>,
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
    launch_context: SlotModeLaunchContext<'_>,
    tmux: &impl TmuxClient,
) -> Result<SlotModeSwitchOutcome, SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    tmux.switch_slot_mode(session_name, slot_id, mode, launch_context)?;

    Ok(SlotModeSwitchOutcome {
        session_name: session_name.to_owned(),
        slot_id,
        mode,
    })
}
