use super::PanePosition;
use super::pick_focus_anchor_pane;
use crate::session::tmux::layout::{
    LAYOUT_MODE_FIVE_PANE, LAYOUT_MODE_FOUR_PANE, LAYOUT_MODE_THREE_PANE, LAYOUT_MODE_TWO_PANE,
};

#[test]
fn two_pane_focus_anchor_stays_on_startup_main_pane() {
    let pane_positions = vec![
        PanePosition {
            pane_id: String::from("%2"),
            left: 0,
            top: 0,
            width: 60,
        },
        PanePosition {
            pane_id: String::from("%1"),
            left: 60,
            top: 0,
            width: 60,
        },
    ];

    assert_eq!(
        pick_focus_anchor_pane(LAYOUT_MODE_TWO_PANE, &pane_positions),
        Some("%1")
    );
}

#[test]
fn four_pane_focus_anchor_stays_on_startup_slot_one_position() {
    let pane_positions = vec![
        PanePosition {
            pane_id: String::from("%2"),
            left: 0,
            top: 0,
            width: 40,
        },
        PanePosition {
            pane_id: String::from("%1"),
            left: 40,
            top: 0,
            width: 60,
        },
        PanePosition {
            pane_id: String::from("%3"),
            left: 0,
            top: 30,
            width: 40,
        },
        PanePosition {
            pane_id: String::from("%4"),
            left: 40,
            top: 30,
            width: 60,
        },
    ];

    assert_eq!(
        pick_focus_anchor_pane(LAYOUT_MODE_FOUR_PANE, &pane_positions),
        Some("%1")
    );
}

#[test]
fn three_pane_focus_anchor_uses_center_column() {
    let pane_positions = vec![
        PanePosition {
            pane_id: String::from("%2"),
            left: 0,
            top: 0,
            width: 24,
        },
        PanePosition {
            pane_id: String::from("%1"),
            left: 24,
            top: 0,
            width: 52,
        },
        PanePosition {
            pane_id: String::from("%3"),
            left: 76,
            top: 0,
            width: 24,
        },
    ];

    assert_eq!(
        pick_focus_anchor_pane(LAYOUT_MODE_THREE_PANE, &pane_positions),
        Some("%1")
    );
}

#[test]
fn five_pane_focus_anchor_uses_center_column() {
    let pane_positions = vec![
        PanePosition {
            pane_id: String::from("%2"),
            left: 0,
            top: 0,
            width: 24,
        },
        PanePosition {
            pane_id: String::from("%1"),
            left: 24,
            top: 0,
            width: 52,
        },
        PanePosition {
            pane_id: String::from("%3"),
            left: 76,
            top: 0,
            width: 24,
        },
        PanePosition {
            pane_id: String::from("%4"),
            left: 0,
            top: 20,
            width: 24,
        },
        PanePosition {
            pane_id: String::from("%5"),
            left: 76,
            top: 20,
            width: 24,
        },
    ];

    assert_eq!(
        pick_focus_anchor_pane(LAYOUT_MODE_FIVE_PANE, &pane_positions),
        Some("%1")
    );
}
