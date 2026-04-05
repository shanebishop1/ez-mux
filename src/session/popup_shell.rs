use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::TmuxClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupShellAction {
    Opened,
    Closed,
}

impl PopupShellAction {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Opened => "opened",
            Self::Closed => "closed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopupShellOutcome {
    pub session_name: String,
    pub slot_id: u8,
    pub action: PopupShellAction,
    pub cwd: String,
    pub width_pct: u8,
    pub height_pct: u8,
}

/// Toggles the popup shell helper surface for one canonical slot.
///
/// # Errors
/// Returns an error when the slot is outside canonical range or tmux popup
/// orchestration fails.
pub fn toggle_popup_shell(
    session_name: &str,
    slot_id: u8,
    client_tty: Option<&str>,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    remote_use_mosh: bool,
    tmux: &impl TmuxClient,
) -> Result<PopupShellOutcome, SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    tmux.toggle_popup_shell(
        session_name,
        slot_id,
        client_tty,
        remote_path,
        remote_server_url,
        remote_use_mosh,
    )
}
