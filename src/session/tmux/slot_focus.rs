use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::options::required_session_option;
use super::slot_swap::{select_pane_preserve_zoom, validate_canonical_slot_registry};
use super::style::refresh_active_border_for_slot;

pub(super) fn focus_slot(session_name: &str, slot_id: u8) -> Result<(), SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    validate_canonical_slot_registry(session_name)?;

    let slot_pane_key = format!("@ezm_slot_{slot_id}_pane");
    let slot_pane_id = required_session_option(session_name, &slot_pane_key)?;

    refresh_active_border_for_slot(session_name, slot_id)?;
    select_pane_preserve_zoom(&slot_pane_id)?;
    validate_canonical_slot_registry(session_name)?;

    Ok(())
}
