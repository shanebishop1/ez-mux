use std::collections::BTreeMap;

use ez_mux::session::THREE_PANE_CENTER_TARGET_PCT;
use ez_mux::session::THREE_PANE_SIDE_TARGET_PCT;
use ez_mux::session::THREE_PANE_TARGET_TOLERANCE_PCT;
use serde::Serialize;

use crate::support::foundation_harness::FoundationHarness;

const CENTER_WIDTH_TARGET_PCT: i32 = 40;
const CENTER_WIDTH_TOLERANCE_PCT: i32 = 3;

#[derive(Serialize)]
pub(crate) struct SessionSnapshot {
    pub(crate) name: String,
    pub(crate) exists: bool,
    pub(crate) count: usize,
}

#[derive(Serialize)]
pub(crate) struct LayoutSnapshot {
    pub(crate) pane_count: usize,
    pub(crate) window_width: i32,
    pub(crate) left_width: i32,
    pub(crate) center_width: i32,
    pub(crate) right_width: i32,
    pub(crate) left_width_pct: i32,
    pub(crate) center_width_pct: i32,
    pub(crate) right_width_pct: i32,
    pub(crate) left_width_target_pct: i32,
    pub(crate) center_width_target_pct: i32,
    pub(crate) right_width_target_pct: i32,
    pub(crate) center_width_tolerance_pct: i32,
    pub(crate) three_pane_within_tolerance: bool,
    pub(crate) center_within_tolerance: bool,
    pub(crate) left_column_panes: usize,
    pub(crate) center_column_panes: usize,
    pub(crate) right_column_panes: usize,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn inspect_layout(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<(LayoutSnapshot, Vec<String>), String> {
    let window_width_raw = harness.tmux_capture(&[
        "display-message",
        "-p",
        "-t",
        &format!("{session_name}:0"),
        "#{window_width}",
    ])?;
    let window_width = window_width_raw
        .trim()
        .parse::<i32>()
        .map_err(|error| format!("invalid window width `{window_width_raw}`: {error}"))?;

    let pane_dump = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session_name}:0"),
        "-F",
        "#{pane_id}|#{pane_width}|#{pane_height}|#{pane_left}",
    ])?;

    let mut panes = Vec::new();
    for line in pane_dump
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split('|');
        let pane_id = parts.next().unwrap_or_default().to_owned();
        let pane_width = parts
            .next()
            .ok_or_else(|| format!("missing pane width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane width in `{line}`: {error}"))?;
        let pane_height = parts
            .next()
            .ok_or_else(|| format!("missing pane height in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane height in `{line}`: {error}"))?;
        let pane_left = parts
            .next()
            .ok_or_else(|| format!("missing pane left in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane left in `{line}`: {error}"))?;
        panes.push((pane_id, pane_width, pane_height, pane_left));
    }

    let max_height = panes
        .iter()
        .map(|(_, _, height, _)| *height)
        .max()
        .unwrap_or(0);

    let mut columns = BTreeMap::<i32, Vec<(String, i32, i32)>>::new();
    for (pane_id, pane_width, pane_height, pane_left) in &panes {
        columns
            .entry(*pane_left)
            .or_default()
            .push((pane_id.clone(), *pane_width, *pane_height));
    }

    let ordered_columns = columns.values().collect::<Vec<_>>();
    let left_column_panes = ordered_columns.first().map_or(0, |column| column.len());
    let right_column_panes = ordered_columns.last().map_or(0, |column| column.len());
    let left_width = ordered_columns
        .first()
        .and_then(|column| column.first())
        .map_or(0, |pane| pane.1);
    let right_width = ordered_columns
        .last()
        .and_then(|column| column.first())
        .map_or(0, |pane| pane.1);

    let mut center_width = 0;
    let mut center_column_panes = 0;
    if ordered_columns.len() >= 3 {
        let center_column = ordered_columns[ordered_columns.len() / 2];
        center_column_panes = center_column.len();
        center_width = center_column.first().map_or(0, |pane| pane.1);
        if center_column.len() != 1 || center_column[0].2 < max_height {
            center_column_panes = 0;
        }
    } else {
        for panes_in_column in columns.values() {
            if panes_in_column.len() == 1 {
                center_column_panes = 1;
                center_width = panes_in_column[0].1;
                if panes_in_column[0].2 < max_height {
                    center_column_panes = 0;
                }
                break;
            }
        }
    }

    let left_width_pct = if window_width > 0 {
        (left_width * 100) / window_width
    } else {
        0
    };
    let center_width_pct = if window_width > 0 {
        (center_width * 100) / window_width
    } else {
        0
    };
    let right_width_pct = if window_width > 0 {
        (right_width * 100) / window_width
    } else {
        0
    };
    let three_pane_target_tolerance_pct = i32::from(THREE_PANE_TARGET_TOLERANCE_PCT);
    let left_three_pane_delta = (left_width_pct - i32::from(THREE_PANE_SIDE_TARGET_PCT)).abs();
    let center_three_pane_delta =
        (center_width_pct - i32::from(THREE_PANE_CENTER_TARGET_PCT)).abs();
    let right_three_pane_delta = (right_width_pct - i32::from(THREE_PANE_SIDE_TARGET_PCT)).abs();
    let three_pane_within_tolerance = left_three_pane_delta <= three_pane_target_tolerance_pct
        && center_three_pane_delta <= three_pane_target_tolerance_pct
        && right_three_pane_delta <= three_pane_target_tolerance_pct;
    let delta = (center_width_pct - CENTER_WIDTH_TARGET_PCT).abs();
    let center_within_tolerance = delta <= CENTER_WIDTH_TOLERANCE_PCT;

    let assertions = vec![
        format!("pane count = {}", panes.len()),
        format!("window width = {window_width}"),
        format!("left width = {left_width}"),
        format!("center width = {center_width}"),
        format!("right width = {right_width}"),
        format!(
            "left width pct = {left_width_pct} (target={} +/- {})",
            THREE_PANE_SIDE_TARGET_PCT, THREE_PANE_TARGET_TOLERANCE_PCT
        ),
        format!(
            "center width pct = {center_width_pct} (target={} +/- {})",
            CENTER_WIDTH_TARGET_PCT, CENTER_WIDTH_TOLERANCE_PCT
        ),
        format!(
            "right width pct = {right_width_pct} (target={} +/- {})",
            THREE_PANE_SIDE_TARGET_PCT, THREE_PANE_TARGET_TOLERANCE_PCT
        ),
        format!(
            "left/center/right panes = {left_column_panes}/{center_column_panes}/{right_column_panes}"
        ),
        format!("three-pane width tolerance satisfied = {three_pane_within_tolerance}"),
        format!("center width within tolerance = {center_within_tolerance}"),
    ];

    Ok((
        LayoutSnapshot {
            pane_count: panes.len(),
            window_width,
            left_width,
            center_width,
            right_width,
            left_width_pct,
            center_width_pct,
            right_width_pct,
            left_width_target_pct: i32::from(THREE_PANE_SIDE_TARGET_PCT),
            center_width_target_pct: CENTER_WIDTH_TARGET_PCT,
            right_width_target_pct: i32::from(THREE_PANE_SIDE_TARGET_PCT),
            center_width_tolerance_pct: CENTER_WIDTH_TOLERANCE_PCT,
            three_pane_within_tolerance,
            center_within_tolerance,
            left_column_panes,
            center_column_panes,
            right_column_panes,
        },
        assertions,
    ))
}
