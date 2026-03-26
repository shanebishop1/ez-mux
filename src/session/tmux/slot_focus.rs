use super::slot_swap::swap_slot_with_center;
use super::SessionError;
use super::CANONICAL_SLOT_IDS;

pub(super) fn focus_slot(session_name: &str, slot_id: u8) -> Result<(), SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    swap_slot_with_center(session_name, slot_id)
}
