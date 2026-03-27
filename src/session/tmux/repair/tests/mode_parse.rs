use super::super::launch_context::parse_slot_mode_label;
use crate::session::SlotMode;

#[test]
fn parse_slot_mode_label_accepts_canonical_mode_values() {
    assert_eq!(parse_slot_mode_label(1, "agent"), SlotMode::Agent);
    assert_eq!(parse_slot_mode_label(2, "shell"), SlotMode::Shell);
    assert_eq!(parse_slot_mode_label(3, "neovim"), SlotMode::Neovim);
    assert_eq!(parse_slot_mode_label(4, "lazygit"), SlotMode::Lazygit);
}

#[test]
fn parse_slot_mode_label_accepts_legacy_shell_and_agent_aliases() {
    assert_eq!(parse_slot_mode_label(2, "bash"), SlotMode::Shell);
    assert_eq!(parse_slot_mode_label(2, "ubuntu"), SlotMode::Shell);
    assert_eq!(parse_slot_mode_label(1, "opencode"), SlotMode::Agent);
    assert_eq!(parse_slot_mode_label(1, "claude"), SlotMode::Agent);
}

#[test]
fn parse_slot_mode_label_defaults_unknown_values_to_agent() {
    assert_eq!(parse_slot_mode_label(3, "unknown"), SlotMode::Agent);
}
