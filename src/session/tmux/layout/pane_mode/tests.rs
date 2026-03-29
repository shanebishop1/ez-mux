use super::allowed_suspended_slots;
use super::pane_mode_spec;

#[test]
fn pane_mode_spec_maps_counts_to_layout_modes() {
    assert_eq!(pane_mode_spec(1).layout_mode, "one-pane");
    assert_eq!(pane_mode_spec(2).layout_mode, "two-pane");
    assert_eq!(pane_mode_spec(3).layout_mode, "three-pane");
    assert_eq!(pane_mode_spec(4).layout_mode, "four-pane");
    assert_eq!(pane_mode_spec(5).layout_mode, "five-pane");
}

#[test]
fn pane_mode_spec_declares_expected_active_slots() {
    assert_eq!(pane_mode_spec(1).active_slots, &[1]);
    assert_eq!(pane_mode_spec(2).active_slots, &[1, 2]);
    assert_eq!(pane_mode_spec(3).active_slots, &[1, 2, 3]);
    assert_eq!(pane_mode_spec(4).active_slots, &[2, 3, 4, 5]);
    assert_eq!(pane_mode_spec(5).active_slots, &[1, 2, 3, 4, 5]);
}

#[test]
fn allowed_suspended_slots_are_defined_for_known_layout_modes() {
    assert_eq!(
        allowed_suspended_slots("one-pane"),
        Some([2, 3, 4, 5].as_ref())
    );
    assert_eq!(
        allowed_suspended_slots("two-pane"),
        Some([3, 4, 5].as_ref())
    );
    assert_eq!(allowed_suspended_slots("three-pane"), Some([4, 5].as_ref()));
    assert_eq!(allowed_suspended_slots("four-pane"), Some([1].as_ref()));
    assert_eq!(allowed_suspended_slots("five-pane"), Some([].as_ref()));
    assert!(allowed_suspended_slots("unknown").is_none());
}
