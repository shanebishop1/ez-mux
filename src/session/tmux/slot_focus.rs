use std::cmp::Reverse;

use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::command::tmux_output_value;
use super::layout::{
    LAYOUT_MODE_FIVE_PANE, LAYOUT_MODE_FOUR_PANE, LAYOUT_MODE_KEY, LAYOUT_MODE_ONE_PANE,
    LAYOUT_MODE_TWO_PANE,
};
use super::options::show_session_option;
use super::slot_swap::swap_slot_with_target_pane;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PanePosition {
    pane_id: String,
    left: u16,
    top: u16,
    width: u16,
}

pub(super) fn focus_slot(session_name: &str, slot_id: u8) -> Result<(), SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    let focus_anchor_pane_id = resolve_focus_anchor_pane(session_name)?;
    swap_slot_with_target_pane(session_name, slot_id, &focus_anchor_pane_id)
}

fn resolve_focus_anchor_pane(session_name: &str) -> Result<String, SessionError> {
    let layout_mode = show_session_option(session_name, LAYOUT_MODE_KEY)?
        .unwrap_or_else(|| String::from(LAYOUT_MODE_FIVE_PANE));
    let pane_positions = load_pane_positions(session_name)?;

    match pick_focus_anchor_pane(layout_mode.as_str(), &pane_positions) {
        Some(pane_id) => Ok(pane_id.to_owned()),
        None => Err(SessionError::TmuxCommandFailed {
            command: format!("resolve-focus-anchor-pane -t {session_name}"),
            stderr: String::from("no live panes were available to resolve focus anchor"),
        }),
    }
}

fn load_pane_positions(session_name: &str) -> Result<Vec<PanePosition>, SessionError> {
    let target = format!("{session_name}:0");
    let output = tmux_output_value(&[
        "list-panes",
        "-t",
        &target,
        "-F",
        "#{pane_id}|#{pane_left}|#{pane_top}|#{pane_width}",
    ])?;

    parse_pane_positions(&output).map_err(|reason| SessionError::TmuxCommandFailed {
        command: format!(
            "list-panes -t {target} -F #{{pane_id}}|#{{pane_left}}|#{{pane_top}}|#{{pane_width}}"
        ),
        stderr: reason,
    })
}

fn parse_pane_positions(output: &str) -> Result<Vec<PanePosition>, String> {
    let mut pane_positions = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split('|');
        let pane_id = parts.next().unwrap_or_default().to_owned();
        let left = parts
            .next()
            .ok_or_else(|| format!("missing pane_left in `{line}`"))?
            .parse::<u16>()
            .map_err(|error| format!("invalid pane_left in `{line}`: {error}"))?;
        let top = parts
            .next()
            .ok_or_else(|| format!("missing pane_top in `{line}`"))?
            .parse::<u16>()
            .map_err(|error| format!("invalid pane_top in `{line}`: {error}"))?;
        let width = parts
            .next()
            .ok_or_else(|| format!("missing pane_width in `{line}`"))?
            .parse::<u16>()
            .map_err(|error| format!("invalid pane_width in `{line}`: {error}"))?;

        pane_positions.push(PanePosition {
            pane_id,
            left,
            top,
            width,
        });
    }

    Ok(pane_positions)
}

fn pick_focus_anchor_pane<'a>(
    layout_mode: &str,
    pane_positions: &'a [PanePosition],
) -> Option<&'a str> {
    match layout_mode {
        LAYOUT_MODE_ONE_PANE => pane_positions.first(),
        LAYOUT_MODE_TWO_PANE => pane_positions.iter().max_by_key(|pane| pane.left),
        LAYOUT_MODE_FOUR_PANE => pane_positions
            .iter()
            .min_by_key(|pane| (pane.top, Reverse(pane.left))),
        _ => {
            let midpoint_pane = pane_containing_window_midpoint(pane_positions);
            midpoint_pane.or_else(|| pane_closest_to_window_midpoint(pane_positions))
        }
    }
    .map(|pane| pane.pane_id.as_str())
}

fn pane_containing_window_midpoint(pane_positions: &[PanePosition]) -> Option<&PanePosition> {
    let midpoint = window_midpoint(pane_positions)?;

    pane_positions
        .iter()
        .filter(|pane| {
            let right = pane.left.saturating_add(pane.width);
            pane.left <= midpoint && midpoint < right
        })
        .max_by_key(|pane| pane.width)
}

fn pane_closest_to_window_midpoint(pane_positions: &[PanePosition]) -> Option<&PanePosition> {
    let midpoint = i32::from(window_midpoint(pane_positions)?);

    pane_positions.iter().min_by_key(|pane| {
        let pane_midpoint = i32::from(pane.left) + (i32::from(pane.width) / 2);
        (pane_midpoint - midpoint).abs()
    })
}

fn window_midpoint(pane_positions: &[PanePosition]) -> Option<u16> {
    pane_positions
        .iter()
        .map(|pane| pane.left.saturating_add(pane.width))
        .max()
        .map(|window_right| window_right / 2)
}

#[cfg(test)]
mod tests;
