use std::path::Path;

use super::DEFAULT_CENTER_WIDTH_PCT;
use super::SessionError;
use super::SlotRegistry;
use super::build_registry_for_canonical_panes;
use super::canonical_five_pane_column_widths;
use super::command::{tmux_output_value, tmux_run};
use super::options::{set_or_verify_pane_option, set_or_verify_session_option};
use super::worktree::discover_worktrees_for_slots;

pub(super) fn bootstrap_default_layout(
    session_name: &str,
    project_dir: &Path,
) -> Result<(), SessionError> {
    let target = format!("{session_name}:0");
    let initial_pane = tmux_output_value(&["list-panes", "-t", &target, "-F", "#{pane_id}"])?
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .ok_or_else(|| SessionError::TmuxCommandFailed {
            command: format!("list-panes -t {target} -F #{{pane_id}}"),
            stderr: String::from("tmux returned no pane id for initial session window"),
        })?
        .to_owned();

    let window_width =
        tmux_output_value(&["display-message", "-p", "-t", &target, "#{window_width}"])?
            .trim()
            .parse::<u16>()
            .map_err(|error| SessionError::TmuxCommandFailed {
                command: format!("display-message -p -t {target} #{{window_width}}"),
                stderr: format!("failed parsing window width: {error}"),
            })?;
    let (_left_width, center_width, right_width) =
        canonical_five_pane_column_widths(window_width, DEFAULT_CENTER_WIDTH_PCT);

    let right_top = split_pane_horizontal(&initial_pane, right_width)?;
    let center = split_pane_horizontal(&initial_pane, center_width)?;
    let left_bottom = split_pane_vertical(&initial_pane)?;
    let right_bottom = split_pane_vertical(&right_top)?;

    let canonical_pane_ids = [initial_pane, center, right_top, left_bottom, right_bottom];
    let worktrees = discover_worktrees_for_slots(project_dir);
    let registry = build_registry_for_canonical_panes(&canonical_pane_ids, &worktrees)?;
    persist_registry(session_name, &registry)?;

    tmux_run(&["select-pane", "-t", &canonical_pane_ids[1]])?;
    Ok(())
}

fn split_pane_horizontal(target_pane: &str, new_width: u16) -> Result<String, SessionError> {
    tmux_output_value(&[
        "split-window",
        "-h",
        "-t",
        target_pane,
        "-l",
        &new_width.to_string(),
        "-P",
        "-F",
        "#{pane_id}",
    ])
    .map(|value| value.trim().to_owned())
}

fn split_pane_vertical(target_pane: &str) -> Result<String, SessionError> {
    tmux_output_value(&[
        "split-window",
        "-v",
        "-t",
        target_pane,
        "-P",
        "-F",
        "#{pane_id}",
    ])
    .map(|value| value.trim().to_owned())
}

fn persist_registry(session_name: &str, registry: &SlotRegistry) -> Result<(), SessionError> {
    for binding in registry.bindings() {
        let slot_pane_key = format!("@ezm_slot_{}_pane", binding.slot_id);
        let slot_worktree_key = format!("@ezm_slot_{}_worktree", binding.slot_id);
        set_or_verify_session_option(session_name, &slot_pane_key, &binding.pane_id)?;
        set_or_verify_session_option(
            session_name,
            &slot_worktree_key,
            &binding.worktree_path.display().to_string(),
        )?;

        let pane_worktree_key = "@ezm_slot_worktree";
        let pane_slot_key = "@ezm_slot_id";
        set_or_verify_pane_option(
            &binding.pane_id,
            pane_slot_key,
            &binding.slot_id.to_string(),
        )?;
        set_or_verify_pane_option(
            &binding.pane_id,
            pane_worktree_key,
            &binding.worktree_path.display().to_string(),
        )?;
    }
    Ok(())
}
