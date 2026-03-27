use super::{
    SlotRestoreMetadata, is_three_pane_mode, restore_width_key, slot_restore_cwd_key,
    slot_restore_mode_key, slot_restore_pane_key, slot_restore_worktree_key, slot_suspended_key,
    validate_restored_slot_continuity,
};

#[test]
fn slot_restore_metadata_keys_are_stable() {
    assert_eq!(slot_suspended_key(4), "@ezm_slot_4_suspended");
    assert_eq!(slot_restore_pane_key(4), "@ezm_slot_4_restore_pane");
    assert_eq!(slot_restore_worktree_key(4), "@ezm_slot_4_restore_worktree");
    assert_eq!(slot_restore_cwd_key(4), "@ezm_slot_4_restore_cwd");
    assert_eq!(slot_restore_mode_key(4), "@ezm_slot_4_restore_mode");
    assert_eq!(restore_width_key(1), "@ezm_restore_width_slot_1");
    assert_eq!(restore_width_key(2), "@ezm_restore_width_slot_2");
    assert_eq!(restore_width_key(3), "@ezm_restore_width_slot_3");
}

#[test]
fn three_pane_mode_detection_is_explicit() {
    assert!(is_three_pane_mode("three-pane"));
    assert!(!is_three_pane_mode("five-pane"));
    assert!(!is_three_pane_mode(""));
}

#[test]
fn restored_slot_continuity_requires_original_slot_identity_and_metadata() {
    let metadata = SlotRestoreMetadata {
        pane_id: String::from("%4"),
        worktree: String::from("wt-4"),
        cwd: String::from("/repo/slot-4"),
        mode: String::from("lazygit"),
    };

    assert!(
        validate_restored_slot_continuity(4, "4", "wt-4", "/repo/slot-4", "lazygit", &metadata)
            .is_ok()
    );

    assert!(
        validate_restored_slot_continuity(4, "9", "wt-4", "/repo/slot-4", "lazygit", &metadata)
            .expect_err("restore path must preserve canonical slot id")
            .contains("@ezm_slot_id")
    );

    assert!(
        validate_restored_slot_continuity(
            4,
            "4",
            "wt-remapped",
            "/repo/slot-4",
            "lazygit",
            &metadata
        )
        .expect_err("restore path must reapply captured worktree")
        .contains("worktree mismatch")
    );

    assert!(
        validate_restored_slot_continuity(4, "4", "wt-4", "/repo/other", "lazygit", &metadata)
            .expect_err("restore path must reapply captured cwd")
            .contains("cwd mismatch")
    );

    assert!(
        validate_restored_slot_continuity(4, "4", "wt-4", "/repo/slot-4", "shell", &metadata)
            .expect_err("restore path must reapply captured mode")
            .contains("mode mismatch")
    );
}
