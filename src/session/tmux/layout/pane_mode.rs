use super::super::command::tmux_run;
use super::SessionError;
use super::{
    LAYOUT_MODE_FIVE_PANE, LAYOUT_MODE_FOUR_PANE, LAYOUT_MODE_ONE_PANE, LAYOUT_MODE_THREE_PANE,
    LAYOUT_MODE_TWO_PANE,
};

const ACTIVE_SLOTS_ONE_PANE: [u8; 1] = [1];
const ACTIVE_SLOTS_TWO_PANE: [u8; 2] = [1, 2];
const ACTIVE_SLOTS_THREE_PANE: [u8; 3] = [1, 2, 3];
const ACTIVE_SLOTS_FOUR_PANE: [u8; 4] = [1, 2, 3, 4];
const ACTIVE_SLOTS_FIVE_PANE: [u8; 5] = [1, 2, 3, 4, 5];

const SUSPENDED_SLOTS_ONE_PANE: [u8; 4] = [2, 3, 4, 5];
const SUSPENDED_SLOTS_TWO_PANE: [u8; 3] = [3, 4, 5];
const SUSPENDED_SLOTS_THREE_PANE: [u8; 2] = [4, 5];
const SUSPENDED_SLOTS_FOUR_PANE: [u8; 1] = [5];
const SUSPENDED_SLOTS_FIVE_PANE: [u8; 0] = [];
const ALLOWED_SUSPENDED_SLOTS_FOUR_PANE: [u8; 2] = [1, 5];

const STARTUP_TWO_PANE_MAIN_TARGET_PCT: u16 = 75;
const STARTUP_THREE_PANE_SIDE_TARGET_PCT: u16 = 23;
const STARTUP_THREE_PANE_CENTER_TARGET_PCT: u16 = 54;
const STARTUP_FOUR_PANE_MAIN_WIDTH_TARGET_PCT: u16 = 60;
const STARTUP_FOUR_PANE_MAIN_HEIGHT_TARGET_PCT: u16 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PaneModeSpec {
    pane_count: u8,
    pub(super) layout_mode: &'static str,
    pub(super) active_slots: &'static [u8],
    pub(super) suspended_slots: &'static [u8],
}

impl PaneModeSpec {
    pub(super) fn logical_slot_for_physical(self, physical_slot_id: u8) -> u8 {
        match self.pane_count {
            4 => match physical_slot_id {
                1 => 5,
                2 => 2,
                3 => 1,
                4 => 3,
                5 => 4,
                _ => physical_slot_id,
            },
            _ => physical_slot_id,
        }
    }

    pub(super) fn physical_slot_for_logical(self, logical_slot_id: u8) -> u8 {
        match self.pane_count {
            4 => match logical_slot_id {
                1 => 3,
                2 => 2,
                3 => 4,
                4 => 5,
                5 => 1,
                _ => logical_slot_id,
            },
            _ => logical_slot_id,
        }
    }
}

pub(super) fn pane_mode_spec(pane_count: u8) -> PaneModeSpec {
    match pane_count {
        1 => PaneModeSpec {
            pane_count,
            layout_mode: LAYOUT_MODE_ONE_PANE,
            active_slots: &ACTIVE_SLOTS_ONE_PANE,
            suspended_slots: &SUSPENDED_SLOTS_ONE_PANE,
        },
        2 => PaneModeSpec {
            pane_count,
            layout_mode: LAYOUT_MODE_TWO_PANE,
            active_slots: &ACTIVE_SLOTS_TWO_PANE,
            suspended_slots: &SUSPENDED_SLOTS_TWO_PANE,
        },
        3 => PaneModeSpec {
            pane_count,
            layout_mode: LAYOUT_MODE_THREE_PANE,
            active_slots: &ACTIVE_SLOTS_THREE_PANE,
            suspended_slots: &SUSPENDED_SLOTS_THREE_PANE,
        },
        4 => PaneModeSpec {
            pane_count,
            layout_mode: LAYOUT_MODE_FOUR_PANE,
            active_slots: &ACTIVE_SLOTS_FOUR_PANE,
            suspended_slots: &SUSPENDED_SLOTS_FOUR_PANE,
        },
        _ => PaneModeSpec {
            pane_count: 5,
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
        LAYOUT_MODE_FOUR_PANE => Some(&ALLOWED_SUSPENDED_SLOTS_FOUR_PANE),
        LAYOUT_MODE_FIVE_PANE => Some(&SUSPENDED_SLOTS_FIVE_PANE),
        _ => None,
    }
}

pub(super) fn apply_startup_pane_mode(
    slot_pane_ids: &[String; 5],
    window_width: u16,
    window_height: u16,
    pane_count: u8,
) -> Result<(), SessionError> {
    match pane_count {
        1 => apply_one_pane(slot_pane_ids),
        2 => apply_two_pane(slot_pane_ids, window_width),
        3 => apply_three_pane(slot_pane_ids, window_width),
        4 => apply_four_pane(slot_pane_ids, window_width, window_height),
        _ => Ok(()),
    }
}

fn apply_one_pane(slot_pane_ids: &[String; 5]) -> Result<(), SessionError> {
    kill_slots(slot_pane_ids, &[5, 4, 3, 2])
}

fn apply_two_pane(slot_pane_ids: &[String; 5], window_width: u16) -> Result<(), SessionError> {
    kill_slots(slot_pane_ids, &[5, 4, 3])?;
    let (left, right) = startup_two_pane_target_widths(window_width);
    resize_slot_width(slot_pane_ids, 2, left)?;
    resize_slot_width(slot_pane_ids, 1, right)
}

fn startup_two_pane_target_widths(window_width: u16) -> (u16, u16) {
    biased_split(window_width, STARTUP_TWO_PANE_MAIN_TARGET_PCT)
}

fn apply_three_pane(slot_pane_ids: &[String; 5], window_width: u16) -> Result<(), SessionError> {
    kill_slots(slot_pane_ids, &[5, 4])?;
    let (left, center, right) = startup_three_pane_target_widths(window_width);
    resize_slot_width(slot_pane_ids, 2, left)?;
    resize_slot_width(slot_pane_ids, 1, center)?;
    resize_slot_width(slot_pane_ids, 3, right)
}

fn startup_three_pane_target_widths(window_width: u16) -> (u16, u16, u16) {
    if window_width < 3 {
        return (1, 1, 1);
    }

    let width = u32::from(window_width);
    let left = (width * u32::from(STARTUP_THREE_PANE_SIDE_TARGET_PCT)) / 100;
    let center = (width * u32::from(STARTUP_THREE_PANE_CENTER_TARGET_PCT)) / 100;
    let right = width - left - center;

    (
        u16::try_from(left).expect("left width must fit u16 after bounded arithmetic"),
        u16::try_from(center).expect("center width must fit u16 after bounded arithmetic"),
        u16::try_from(right).expect("right width must fit u16 after bounded arithmetic"),
    )
}

fn apply_four_pane(
    slot_pane_ids: &[String; 5],
    window_width: u16,
    window_height: u16,
) -> Result<(), SessionError> {
    kill_slots(slot_pane_ids, &[1])?;
    let (left, right, top, _bottom) =
        startup_four_pane_target_dimensions(window_width, window_height);
    resize_slot_width(slot_pane_ids, 2, left)?;
    resize_slot_width(slot_pane_ids, 3, right)?;
    resize_slot_height(slot_pane_ids, 2, top)?;
    resize_slot_height(slot_pane_ids, 3, top)
}

fn startup_four_pane_target_dimensions(
    window_width: u16,
    window_height: u16,
) -> (u16, u16, u16, u16) {
    let (left, right) = biased_split(window_width, STARTUP_FOUR_PANE_MAIN_WIDTH_TARGET_PCT);
    let (bottom, top) = biased_split(window_height, STARTUP_FOUR_PANE_MAIN_HEIGHT_TARGET_PCT);
    (left, right, top, bottom)
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

fn resize_slot_height(
    slot_pane_ids: &[String; 5],
    slot_id: u8,
    height: u16,
) -> Result<(), SessionError> {
    tmux_run(&[
        "resize-pane",
        "-t",
        pane_for_slot(slot_pane_ids, slot_id),
        "-y",
        &height.to_string(),
    ])
}

fn biased_split(total: u16, primary_pct: u16) -> (u16, u16) {
    if total <= 1 {
        return (0, total);
    }

    let total = u32::from(total);
    let mut primary = (total * u32::from(primary_pct)) / 100;
    primary = primary.clamp(1, total.saturating_sub(1));
    let secondary = total - primary;

    (
        u16::try_from(secondary).expect("secondary split must fit u16 after bounded arithmetic"),
        u16::try_from(primary).expect("primary split must fit u16 after bounded arithmetic"),
    )
}

fn pane_for_slot(slot_pane_ids: &[String; 5], slot_id: u8) -> &str {
    slot_pane_ids[usize::from(slot_id - 1)].as_str()
}

#[cfg(test)]
mod tests;
