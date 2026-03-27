use super::super::SessionError;
use super::super::command::{tmux_output_value, tmux_primary_window_target, tmux_run};
use super::super::options::required_session_option;
use super::super::{DEFAULT_CENTER_WIDTH_PCT, canonical_five_pane_column_widths};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PaneLeftMetric {
    pub(super) pane_id: String,
    pub(super) left: u16,
}

pub(super) fn discover_right_column_anchor_pane(
    session_name: &str,
    center_pane_id: &str,
) -> Result<Option<String>, SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    let output =
        tmux_output_value(&["list-panes", "-t", &target, "-F", "#{pane_id}|#{pane_left}"])?;
    let metrics = parse_pane_left_metrics(&output);
    Ok(select_right_column_anchor(center_pane_id, &metrics))
}

pub(super) fn parse_pane_left_metrics(output: &str) -> Vec<PaneLeftMetric> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let (pane_id, left) = line.split_once('|')?;
            let pane_id = pane_id.trim();
            let left = left.trim().parse::<u16>().ok()?;
            if pane_id.is_empty() {
                return None;
            }
            Some(PaneLeftMetric {
                pane_id: pane_id.to_owned(),
                left,
            })
        })
        .collect()
}

pub(super) fn select_right_column_anchor(
    center_pane_id: &str,
    metrics: &[PaneLeftMetric],
) -> Option<String> {
    let center_left = metrics
        .iter()
        .find(|metric| metric.pane_id == center_pane_id)
        .map(|metric| metric.left)?;

    metrics
        .iter()
        .filter(|metric| metric.left > center_left)
        .max_by_key(|metric| metric.left)
        .map(|metric| metric.pane_id.clone())
}

pub(super) fn restore_canonical_column_widths(session_name: &str) -> Result<(), SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    let window_width_raw =
        tmux_output_value(&["display-message", "-p", "-t", &target, "#{window_width}"])?;
    let window_width = window_width_raw.trim().parse::<u16>().map_err(|error| {
        SessionError::TmuxCommandFailed {
            command: format!("display-message -p -t {target} #{{window_width}}"),
            stderr: format!("failed parsing window width: {error}"),
        }
    })?;

    let (left_target, center_target, right_target) =
        canonical_five_pane_column_widths(window_width, DEFAULT_CENTER_WIDTH_PCT);
    let left_pane = required_session_option(session_name, "@ezm_slot_2_pane")?;
    let center_pane = required_session_option(session_name, "@ezm_slot_1_pane")?;
    let right_pane = required_session_option(session_name, "@ezm_slot_3_pane")?;

    tmux_run(&[
        "resize-pane",
        "-t",
        &left_pane,
        "-x",
        &left_target.to_string(),
    ])?;
    tmux_run(&[
        "resize-pane",
        "-t",
        &center_pane,
        "-x",
        &center_target.to_string(),
    ])?;
    tmux_run(&[
        "resize-pane",
        "-t",
        &right_pane,
        "-x",
        &right_target.to_string(),
    ])?;

    Ok(())
}
