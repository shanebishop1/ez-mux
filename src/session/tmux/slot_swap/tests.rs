use super::SlotContinuitySnapshot;
use super::should_retry_without_zoom;
use super::validate_slot_suspension;
use super::validate_suspended_slot_restore_metadata;
use crate::session::tmux::layout::{
    LAYOUT_MODE_FIVE_PANE, LAYOUT_MODE_FOUR_PANE, LAYOUT_MODE_ONE_PANE, LAYOUT_MODE_THREE_PANE,
    LAYOUT_MODE_TWO_PANE,
};

#[test]
fn retries_only_for_zoom_attempts_with_status_one() {
    assert!(should_retry_without_zoom(
        "swap-pane",
        "swap-pane -Z -s %1 -t %2",
        "status=1; stdout=\"\"; stderr=\"unknown option -- Z\""
    ));
    assert!(!should_retry_without_zoom(
        "swap-pane",
        "swap-pane -s %1 -t %2",
        "status=1; stdout=\"\"; stderr=\"pane not found\""
    ));
    assert!(!should_retry_without_zoom(
        "swap-pane",
        "swap-pane -Z -s %1 -t %2",
        "status=127; stdout=\"\"; stderr=\"pane not found\""
    ));
}

#[test]
fn suspension_policy_follows_declared_layout_mode() {
    assert!(validate_slot_suspension(LAYOUT_MODE_ONE_PANE, 2, true).is_ok());
    assert!(validate_slot_suspension(LAYOUT_MODE_TWO_PANE, 3, true).is_ok());
    assert!(validate_slot_suspension(LAYOUT_MODE_THREE_PANE, 4, true).is_ok());
    assert!(validate_slot_suspension(LAYOUT_MODE_THREE_PANE, 5, true).is_ok());
    assert!(validate_slot_suspension(LAYOUT_MODE_FOUR_PANE, 1, true).is_ok());
    assert!(validate_slot_suspension(LAYOUT_MODE_FOUR_PANE, 5, true).is_ok());

    assert!(
        validate_slot_suspension(LAYOUT_MODE_THREE_PANE, 3, true)
            .expect_err("slot 3 must reject suspension")
            .contains("cannot be suspended")
    );
    assert!(
        validate_slot_suspension(LAYOUT_MODE_FIVE_PANE, 4, true)
            .expect_err("five-pane mode must reject suspension")
            .contains("cannot be suspended")
    );
    assert!(validate_slot_suspension(LAYOUT_MODE_FIVE_PANE, 4, false).is_ok());
}

#[test]
fn suspended_slots_require_restore_metadata_to_match_slot_identity() {
    assert!(
        validate_suspended_slot_restore_metadata(
            4,
            SlotContinuitySnapshot {
                worktree: "wt-4",
                cwd: "/repo/slot-4",
                mode: "lazygit",
            },
            SlotContinuitySnapshot {
                worktree: "wt-4",
                cwd: "/repo/slot-4",
                mode: "lazygit",
            }
        )
        .is_ok()
    );

    assert!(
        validate_suspended_slot_restore_metadata(
            4,
            SlotContinuitySnapshot {
                worktree: "wt-4",
                cwd: "/repo/slot-4",
                mode: "lazygit",
            },
            SlotContinuitySnapshot {
                worktree: "wt-4",
                cwd: "/repo/slot-4",
                mode: "lazygit",
            }
        )
        .is_ok()
    );

    assert!(
        validate_suspended_slot_restore_metadata(
            5,
            SlotContinuitySnapshot {
                worktree: "wt-5",
                cwd: "/repo/slot-5",
                mode: "shell",
            },
            SlotContinuitySnapshot {
                worktree: "wt-override",
                cwd: "/repo/slot-5",
                mode: "shell",
            }
        )
        .expect_err("suspended metadata must keep worktree")
        .contains("worktree mismatch")
    );

    assert!(
        validate_suspended_slot_restore_metadata(
            5,
            SlotContinuitySnapshot {
                worktree: "wt-5",
                cwd: "/repo/slot-5",
                mode: "shell",
            },
            SlotContinuitySnapshot {
                worktree: "wt-5",
                cwd: "/repo/other",
                mode: "shell",
            }
        )
        .expect_err("suspended metadata must keep cwd")
        .contains("cwd mismatch")
    );

    assert!(
        validate_suspended_slot_restore_metadata(
            5,
            SlotContinuitySnapshot {
                worktree: "wt-5",
                cwd: "/repo/slot-5",
                mode: "shell",
            },
            SlotContinuitySnapshot {
                worktree: "wt-5",
                cwd: "/repo/slot-5",
                mode: "agent",
            }
        )
        .expect_err("suspended metadata must keep mode")
        .contains("mode mismatch")
    );
}
