use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::TmuxClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusSlotOutcome {
    pub session_name: String,
    pub slot_id: u8,
}

/// Moves one canonical slot pane into the main focus position.
///
/// # Errors
/// Returns an error when the slot is outside canonical range or tmux cannot
/// focus/swap the target pane.
pub fn focus_slot(
    session_name: &str,
    slot_id: u8,
    tmux: &impl TmuxClient,
) -> Result<FocusSlotOutcome, SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    tmux.focus_slot(session_name, slot_id)?;

    Ok(FocusSlotOutcome {
        session_name: session_name.to_owned(),
        slot_id,
    })
}
