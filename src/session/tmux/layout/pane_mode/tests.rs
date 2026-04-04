use super::allowed_suspended_slots;
use super::pane_mode_spec;
use super::startup_four_pane_target_dimensions;
use super::startup_three_pane_target_widths;
use super::startup_two_pane_target_widths;

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
    assert_eq!(pane_mode_spec(4).active_slots, &[1, 2, 3, 4]);
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
    assert_eq!(allowed_suspended_slots("four-pane"), Some([1, 5].as_ref()));
    assert_eq!(allowed_suspended_slots("five-pane"), Some([].as_ref()));
    assert!(allowed_suspended_slots("unknown").is_none());
}

#[test]
fn startup_three_pane_widths_keep_center_noticeably_wider() {
    let (left, center, right) = startup_three_pane_target_widths(100);

    assert_eq!(left + center + right, 100);
    assert!(center > left);
    assert!(center > right);
    assert!(center >= left + 20);
}

#[test]
fn startup_two_pane_widths_bias_main_pane() {
    let (left, right) = startup_two_pane_target_widths(100);

    assert_eq!(left + right, 100);
    assert_eq!(left, 25);
    assert_eq!(right, 75);
}

#[test]
fn startup_four_pane_dimensions_bias_main_pane_width_and_height() {
    let (left, right, top, bottom) = startup_four_pane_target_dimensions(100, 50);

    assert_eq!(left + right, 100);
    assert_eq!(top + bottom, 50);
    assert_eq!(left, 40);
    assert_eq!(right, 60);
    assert_eq!(top, 30);
    assert_eq!(bottom, 20);
}

#[test]
fn four_pane_mode_relabels_physical_slots_to_logical_one_through_four() {
    let spec = pane_mode_spec(4);

    assert_eq!(spec.logical_slot_for_physical(2), 2);
    assert_eq!(spec.logical_slot_for_physical(3), 1);
    assert_eq!(spec.logical_slot_for_physical(4), 3);
    assert_eq!(spec.logical_slot_for_physical(5), 4);
    assert_eq!(spec.logical_slot_for_physical(1), 5);

    assert_eq!(spec.physical_slot_for_logical(1), 3);
    assert_eq!(spec.physical_slot_for_logical(2), 2);
    assert_eq!(spec.physical_slot_for_logical(3), 4);
    assert_eq!(spec.physical_slot_for_logical(4), 5);
    assert_eq!(spec.physical_slot_for_logical(5), 1);
}
