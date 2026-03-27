use std::cell::RefCell;
use std::collections::BTreeSet;

use super::super::reconcile::{SplitDirection, reconcile_loaded_session_damage, recreate_plan};
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
        |_session_name, slot_id, _slot_metadata, missing_slots| {
            assert_eq!(slot_id, 4);
            assert_eq!(missing_slots, &BTreeSet::from([4_u8]));
            Ok(String::from("%44"))
        },
        |_session_name, slot_id, metadata| {
            persisted.borrow_mut().push((
                slot_id,
                metadata.worktree.clone(),
                metadata.cwd.clone(),
                metadata.mode.clone(),
            ));
            Ok(())
        },
        |_session_name| {
            *validated.borrow_mut() += 1;
            Ok(())
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
        |_session_name, slot_id, _slot_metadata, missing_slots| {
            assert_eq!(slot_id, 3);
            assert_eq!(missing_slots, &BTreeSet::from([3_u8]));
            Ok(String::from("%33"))
        },
        |_session_name, slot_id, _metadata| {
            persisted_slot_ids.borrow_mut().push(slot_id);
            Ok(())
        },
        |_session_name| Ok(()),
    )
    .expect("selective reconcile should succeed");

    assert_eq!(outcome.healthy_slots, vec![1, 2, 4, 5]);
    assert_eq!(outcome.recreated_slots, vec![3]);
    assert_eq!(persisted_slot_ids.into_inner(), vec![3]);
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
