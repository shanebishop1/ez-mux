use std::cell::RefCell;
use std::collections::BTreeSet;

use super::super::reconcile::{
    ReconcileOperations, SplitDirection, reconcile_loaded_session_damage,
    reconcile_loaded_session_damage_with_suspension, recreate_plan,
};
use super::canonical_slot_metadata;

#[test]
fn selective_reconcile_persists_context_only_for_recreated_slots() {
    let slot_metadata = canonical_slot_metadata();
    let live_panes = BTreeSet::from([
        String::from("%1"),
        String::from("%2"),
        String::from("%3"),
        String::from("%5"),
    ]);
    let persisted = RefCell::new(Vec::<(u8, String, String, String)>::new());
    let validated = RefCell::new(0_u8);

    let outcome = reconcile_loaded_session_damage(
        "ezm-session-ctx",
        slot_metadata,
        &live_panes,
        ReconcileOperations {
            recreate_slot: Box::new(|_session_name, slot_id, _slot_metadata, missing_slots| {
                assert_eq!(slot_id, 4);
                assert_eq!(missing_slots, &BTreeSet::from([4_u8]));
                Ok(String::from("%44"))
            }),
            persist_slot: Box::new(|_session_name, slot_id, metadata| {
                persisted.borrow_mut().push((
                    slot_id,
                    metadata.worktree.clone(),
                    metadata.cwd.clone(),
                    metadata.mode.clone(),
                ));
                Ok(())
            }),
            validate_slots: Box::new(|_session_name| {
                *validated.borrow_mut() += 1;
                Ok(())
            }),
        },
    )
    .expect("selective reconcile should succeed");

    assert_eq!(outcome.healthy_slots, vec![1, 2, 3, 5]);
    assert_eq!(outcome.recreated_slots, vec![4]);
    assert_eq!(
        persisted.into_inner(),
        vec![(
            4,
            String::from("wt-4"),
            String::from("/repo/slot-4"),
            String::from("lazygit"),
        )]
    );
    assert_eq!(validated.into_inner(), 1);
}

#[test]
fn selective_reconcile_keeps_dependent_healthy_slot_context_untouched() {
    let slot_metadata = canonical_slot_metadata();
    let live_panes = BTreeSet::from([
        String::from("%1"),
        String::from("%2"),
        String::from("%4"),
        String::from("%5"),
    ]);
    let persisted_slot_ids = RefCell::new(Vec::<u8>::new());

    let outcome = reconcile_loaded_session_damage(
        "ezm-session-ctx",
        slot_metadata,
        &live_panes,
        ReconcileOperations {
            recreate_slot: Box::new(|_session_name, slot_id, _slot_metadata, missing_slots| {
                assert_eq!(slot_id, 3);
                assert_eq!(missing_slots, &BTreeSet::from([3_u8]));
                Ok(String::from("%33"))
            }),
            persist_slot: Box::new(|_session_name, slot_id, _metadata| {
                persisted_slot_ids.borrow_mut().push(slot_id);
                Ok(())
            }),
            validate_slots: Box::new(|_session_name| Ok(())),
        },
    )
    .expect("selective reconcile should succeed");

    assert_eq!(outcome.healthy_slots, vec![1, 2, 4, 5]);
    assert_eq!(outcome.recreated_slots, vec![3]);
    assert_eq!(persisted_slot_ids.into_inner(), vec![3]);
}

#[test]
fn selective_reconcile_is_idempotent_when_all_slots_are_healthy() {
    let slot_metadata = canonical_slot_metadata();
    let live_panes = BTreeSet::from([
        String::from("%1"),
        String::from("%2"),
        String::from("%3"),
        String::from("%4"),
        String::from("%5"),
    ]);
    let recreate_calls = RefCell::new(0_u8);
    let persisted_slots = RefCell::new(Vec::new());
    let validation_calls = RefCell::new(0_u8);

    let outcome = reconcile_loaded_session_damage(
        "ezm-session-ctx",
        slot_metadata,
        &live_panes,
        ReconcileOperations {
            recreate_slot: Box::new(|_session_name, _slot_id, _slot_metadata, _missing_slots| {
                *recreate_calls.borrow_mut() += 1;
                Ok(String::from("%99"))
            }),
            persist_slot: Box::new(|_session_name, slot_id, _metadata| {
                persisted_slots.borrow_mut().push(slot_id);
                Ok(())
            }),
            validate_slots: Box::new(|_session_name| {
                *validation_calls.borrow_mut() += 1;
                Ok(())
            }),
        },
    )
    .expect("healthy reconcile should be a no-op");

    assert!(outcome.recreated_slots.is_empty());
    assert_eq!(*recreate_calls.borrow(), 0);
    assert!(persisted_slots.borrow().is_empty());
    assert_eq!(*validation_calls.borrow(), 1);
}

#[test]
fn no_op_reconcile_surfaces_pane_and_session_mode_disagreement() {
    let slot_metadata = canonical_slot_metadata();
    let live_panes = BTreeSet::from([
        String::from("%1"),
        String::from("%2"),
        String::from("%3"),
        String::from("%4"),
        String::from("%5"),
    ]);

    let error = reconcile_loaded_session_damage(
        "ezm-session-mode-mismatch",
        slot_metadata,
        &live_panes,
        ReconcileOperations {
            recreate_slot: Box::new(|_, _, _, _| Ok(String::from("%unused"))),
            persist_slot: Box::new(|_, _, _| Ok(())),
            validate_slots: Box::new(|_| {
                Err(crate::session::SessionError::TmuxCommandFailed {
                    command: String::from("validate-canonical-slot-registry"),
                    stderr: String::from("slot 2 pane mode mismatch session=agent pane=shell"),
                })
            }),
        },
    )
    .expect_err("no-op repair must not hide mode metadata disagreement");

    assert!(error.to_string().contains("mode mismatch"));
}

#[test]
fn ordinary_reconcile_preserves_suspended_slots_as_layout_state() {
    let mut slot_metadata = canonical_slot_metadata();
    slot_metadata.get_mut(&4).expect("slot 4").suspended = true;
    slot_metadata.get_mut(&5).expect("slot 5").suspended = true;
    let live_panes = BTreeSet::from([String::from("%1"), String::from("%2"), String::from("%3")]);
    let recreate_calls = RefCell::new(0_u8);

    let outcome = reconcile_loaded_session_damage_with_suspension(
        "ezm-session-ctx",
        slot_metadata,
        &live_panes,
        &BTreeSet::from([4_u8, 5]),
        &BTreeSet::new(),
        ReconcileOperations {
            recreate_slot: Box::new(|_session_name, _slot_id, _slot_metadata, _missing_slots| {
                *recreate_calls.borrow_mut() += 1;
                Ok(String::from("%99"))
            }),
            persist_slot: Box::new(|_session_name, _slot_id, _metadata| Ok(())),
            validate_slots: Box::new(|_session_name| Ok(())),
        },
    )
    .expect("ordinary reconcile should preserve suspension");

    assert!(outcome.recreated_slots.is_empty());
    assert_eq!(*recreate_calls.borrow(), 0);
}

#[test]
fn recreate_plan_prefers_existing_sibling_pane_for_top_slot_recovery() {
    let missing = BTreeSet::from([3_u8]);

    let plan = recreate_plan(3, &missing).expect("plan");

    assert_eq!(plan.target_slot, 5);
    assert_eq!(plan.direction, SplitDirection::Vertical);
    assert!(plan.place_before);
}

#[test]
fn recreate_plan_uses_center_slot_when_column_is_fully_missing() {
    let missing = BTreeSet::from([3_u8, 5_u8]);

    let plan = recreate_plan(3, &missing).expect("plan");

    assert_eq!(plan.target_slot, 1);
    assert_eq!(plan.direction, SplitDirection::Horizontal);
    assert!(!plan.place_before);
}

#[test]
fn recreate_plan_supports_active_center_slot_recovery() {
    let plan = recreate_plan(1, &BTreeSet::from([1_u8])).expect("slot 1 plan");

    assert_eq!(plan.target_slot, 2);
    assert_eq!(plan.direction, SplitDirection::Horizontal);
    assert!(!plan.place_before);
}
