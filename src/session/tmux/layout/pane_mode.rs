use crate::session::three_pane_target_widths;

use super::super::command::tmux_run;
use super::SessionError;
use super::{
    LAYOUT_MODE_FIVE_PANE, LAYOUT_MODE_FOUR_PANE, LAYOUT_MODE_ONE_PANE, LAYOUT_MODE_THREE_PANE,
    LAYOUT_MODE_TWO_PANE,
};

const ACTIVE_SLOTS_ONE_PANE: [u8; 1] = [1];
const ACTIVE_SLOTS_TWO_PANE: [u8; 2] = [1, 2];
const ACTIVE_SLOTS_THREE_PANE: [u8; 3] = [1, 2, 3];
const ACTIVE_SLOTS_FOUR_PANE: [u8; 4] = [2, 3, 4, 5];
const ACTIVE_SLOTS_FIVE_PANE: [u8; 5] = [1, 2, 3, 4, 5];

const SUSPENDED_SLOTS_ONE_PANE: [u8; 4] = [2, 3, 4, 5];
const SUSPENDED_SLOTS_TWO_PANE: [u8; 3] = [3, 4, 5];
const SUSPENDED_SLOTS_THREE_PANE: [u8; 2] = [4, 5];
const SUSPENDED_SLOTS_FOUR_PANE: [u8; 1] = [1];
const SUSPENDED_SLOTS_FIVE_PANE: [u8; 0] = [];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PaneModeSpec {
    pub(super) layout_mode: &'static str,
    pub(super) active_slots: &'static [u8],
    pub(super) suspended_slots: &'static [u8],
}

pub(super) fn pane_mode_spec(pane_count: u8) -> PaneModeSpec {
    match pane_count {
        1 => PaneModeSpec {
            layout_mode: LAYOUT_MODE_ONE_PANE,
            active_slots: &ACTIVE_SLOTS_ONE_PANE,
            suspended_slots: &SUSPENDED_SLOTS_ONE_PANE,
        },
        2 => PaneModeSpec {
            layout_mode: LAYOUT_MODE_TWO_PANE,
            active_slots: &ACTIVE_SLOTS_TWO_PANE,
            suspended_slots: &SUSPENDED_SLOTS_TWO_PANE,
        },
        3 => PaneModeSpec {
            layout_mode: LAYOUT_MODE_THREE_PANE,
            active_slots: &ACTIVE_SLOTS_THREE_PANE,
            suspended_slots: &SUSPENDED_SLOTS_THREE_PANE,
        },
        4 => PaneModeSpec {
            layout_mode: LAYOUT_MODE_FOUR_PANE,
            active_slots: &ACTIVE_SLOTS_FOUR_PANE,
            suspended_slots: &SUSPENDED_SLOTS_FOUR_PANE,
        },
        _ => PaneModeSpec {
            layout_mode: LAYOUT_MODE_FIVE_PANE,
            active_slots: &ACTIVE_SLOTS_FIVE_PANE,
            suspended_slots: &SUSPENDED_SLOTS_FIVE_PANE,
        },
    }
}

pub(super) fn allowed_suspended_slots(layout_mode: &str) -> Option<&'static [u8]> {
    match layout_mode {
        LAYOUT_MODE_ONE_PANE => Some(&SUSPENDED_SLOTS_ONE_PANE),
        LAYOUT_MODE_TWO_PANE => Some(&SUSPENDED_SLOTS_TWO_PANE),
        LAYOUT_MODE_THREE_PANE => Some(&SUSPENDED_SLOTS_THREE_PANE),
        LAYOUT_MODE_FOUR_PANE => Some(&SUSPENDED_SLOTS_FOUR_PANE),
        LAYOUT_MODE_FIVE_PANE => Some(&SUSPENDED_SLOTS_FIVE_PANE),
        _ => None,
    }
}

pub(super) fn apply_startup_pane_mode(
    slot_pane_ids: &[String; 5],
    window_width: u16,
    pane_count: u8,
) -> Result<(), SessionError> {
    match pane_count {
        1 => apply_one_pane(slot_pane_ids),
        2 => apply_two_pane(slot_pane_ids, window_width),
        3 => apply_three_pane(slot_pane_ids, window_width),
        4 => apply_four_pane(slot_pane_ids, window_width),
        _ => Ok(()),
    }
}

fn apply_one_pane(slot_pane_ids: &[String; 5]) -> Result<(), SessionError> {
    kill_slots(slot_pane_ids, &[5, 4, 3, 2])
}

fn apply_two_pane(slot_pane_ids: &[String; 5], window_width: u16) -> Result<(), SessionError> {
    kill_slots(slot_pane_ids, &[5, 4, 3])?;
    let left = window_width / 2;
    let right = window_width.saturating_sub(left);
    resize_slot_width(slot_pane_ids, 2, left)?;
    resize_slot_width(slot_pane_ids, 1, right)
}

fn apply_three_pane(slot_pane_ids: &[String; 5], window_width: u16) -> Result<(), SessionError> {
    kill_slots(slot_pane_ids, &[5, 4])?;
    let (left, center, right) = three_pane_target_widths(window_width);
    resize_slot_width(slot_pane_ids, 2, left)?;
    resize_slot_width(slot_pane_ids, 1, center)?;
    resize_slot_width(slot_pane_ids, 3, right)
}

fn apply_four_pane(slot_pane_ids: &[String; 5], window_width: u16) -> Result<(), SessionError> {
    kill_slots(slot_pane_ids, &[1])?;
    let left = window_width / 2;
    let right = window_width.saturating_sub(left);
    resize_slot_width(slot_pane_ids, 2, left)?;
    resize_slot_width(slot_pane_ids, 3, right)
}

fn kill_slots(slot_pane_ids: &[String; 5], slot_ids: &[u8]) -> Result<(), SessionError> {
    for &slot_id in slot_ids {
        tmux_run(&["kill-pane", "-t", pane_for_slot(slot_pane_ids, slot_id)])?;
    }
    Ok(())
}

fn resize_slot_width(
    slot_pane_ids: &[String; 5],
    slot_id: u8,
    width: u16,
) -> Result<(), SessionError> {
    tmux_run(&[
        "resize-pane",
        "-t",
        pane_for_slot(slot_pane_ids, slot_id),
        "-x",
        &width.to_string(),
    ])
}

fn pane_for_slot(slot_pane_ids: &[String; 5], slot_id: u8) -> &str {
    slot_pane_ids[usize::from(slot_id - 1)].as_str()
}

#[cfg(test)]
mod tests;
