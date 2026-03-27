use std::collections::{BTreeSet, HashMap};

use super::super::metadata::{apply_recovered_slot_pane_bindings, parse_live_slot_binding};
use super::canonical_slot_metadata;

#[test]
fn apply_recovered_slot_pane_bindings_updates_dead_slot_pointer_from_live_binding() {
    let mut slot_metadata = canonical_slot_metadata();
    slot_metadata.get_mut(&5).expect("slot 5").pane_id = String::from("%dead");
    let live_panes = BTreeSet::from([
        String::from("%1"),
        String::from("%2"),
        String::from("%3"),
        String::from("%4"),
        String::from("%55"),
    ]);
    let live_bindings = HashMap::from([(5_u8, String::from("%55"))]);

    let recovered =
        apply_recovered_slot_pane_bindings(&mut slot_metadata, &live_panes, &live_bindings);

    assert_eq!(recovered, vec![5]);
    assert_eq!(slot_metadata.get(&5).expect("slot 5").pane_id, "%55");
}

#[test]
fn apply_recovered_slot_pane_bindings_preserves_live_session_pointer() {
    let mut slot_metadata = canonical_slot_metadata();
    let live_panes = BTreeSet::from([
        String::from("%1"),
        String::from("%2"),
        String::from("%3"),
        String::from("%4"),
        String::from("%5"),
    ]);
    let live_bindings = HashMap::from([(5_u8, String::from("%55"))]);

    let recovered =
        apply_recovered_slot_pane_bindings(&mut slot_metadata, &live_panes, &live_bindings);

    assert!(recovered.is_empty());
    assert_eq!(slot_metadata.get(&5).expect("slot 5").pane_id, "%5");
}

#[test]
fn parse_live_slot_binding_accepts_only_canonical_slot_ids() {
    assert_eq!(parse_live_slot_binding("1"), Some(1));
    assert_eq!(parse_live_slot_binding("5"), Some(5));
    assert_eq!(parse_live_slot_binding("0"), None);
    assert_eq!(parse_live_slot_binding("6"), None);
    assert_eq!(parse_live_slot_binding("not-a-slot"), None);
}
