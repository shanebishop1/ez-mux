use super::SessionError;
use super::SlotMode;
use super::TmuxClient;

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
    tmux: &impl TmuxClient,
) -> Result<SlotModeSwitchOutcome, SessionError> {
    tmux.switch_slot_mode(session_name, slot_id, mode)?;

    Ok(SlotModeSwitchOutcome {
        session_name: session_name.to_owned(),
        slot_id,
        mode,
    })
}
