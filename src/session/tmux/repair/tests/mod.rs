use std::collections::HashMap;

use super::metadata::SlotMetadata;

mod geometry;
mod metadata;
mod mode_parse;
mod reconcile;

pub(super) fn canonical_slot_metadata() -> HashMap<u8, SlotMetadata> {
    HashMap::from([
        (
            1_u8,
            SlotMetadata {
                pane_id: String::from("%1"),
                worktree: String::from("wt-1"),
                cwd: String::from("/repo/slot-1"),
                mode: String::from("agent"),
            },
        ),
        (
            2_u8,
            SlotMetadata {
                pane_id: String::from("%2"),
                worktree: String::from("wt-2"),
                cwd: String::from("/repo/slot-2"),
                mode: String::from("shell"),
            },
        ),
        (
            3_u8,
            SlotMetadata {
                pane_id: String::from("%3"),
                worktree: String::from("wt-3"),
                cwd: String::from("/repo/slot-3"),
                mode: String::from("neovim"),
            },
        ),
        (
            4_u8,
            SlotMetadata {
                pane_id: String::from("%4"),
                worktree: String::from("wt-4"),
                cwd: String::from("/repo/slot-4"),
                mode: String::from("lazygit"),
            },
        ),
        (
            5_u8,
            SlotMetadata {
                pane_id: String::from("%5"),
                worktree: String::from("wt-5"),
                cwd: String::from("/repo/slot-5"),
                mode: String::from("shell"),
            },
        ),
    ])
}
