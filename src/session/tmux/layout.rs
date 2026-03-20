use std::path::Path;

use super::DEFAULT_CENTER_WIDTH_PCT;
use super::SessionError;
use super::SlotRegistry;
use super::build_registry_for_canonical_panes;
use super::canonical_five_pane_column_widths;
use super::command::{tmux_output_value, tmux_run};
use super::options::{set_or_verify_pane_option, set_or_verify_session_option};
use super::slot_swap::validate_canonical_slot_registry;
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

    let mut created_panes = Vec::with_capacity(4);
    let result = (|| {
        let right_top = split_pane_horizontal(&initial_pane, right_width)?;
        created_panes.push(right_top.clone());
        let center = split_pane_horizontal(&initial_pane, center_width)?;
        created_panes.push(center.clone());
        let left_bottom = split_pane_vertical(&initial_pane)?;
        created_panes.push(left_bottom.clone());
        let right_bottom = split_pane_vertical(&right_top)?;
        created_panes.push(right_bottom.clone());

        let canonical_pane_ids = [
            initial_pane.clone(),
            center,
            right_top,
            left_bottom,
            right_bottom,
        ];
        let discovery = discover_worktrees_for_slots(project_dir);
        if let Some(warning) = &discovery.warning {
            eprintln!("warning: {warning}");
        }
        let registry =
            build_registry_for_canonical_panes(&canonical_pane_ids, &discovery.worktrees)?;
        persist_registry(session_name, &registry)?;
        validate_canonical_slot_registry(session_name)?;
        tmux_run(&["select-pane", "-t", &canonical_pane_ids[1]])
    })();

    if let Err(error) = result {
        if let Err(compensation_error) = kill_created_panes(&created_panes) {
            return Err(SessionError::TmuxCommandFailed {
                command: format!("bootstrap-default-layout -t {session_name}"),
                stderr: format!(
                    "layout bootstrap failed: {error}; compensation failed while cleaning panes: {compensation_error}"
                ),
            });
        }

        return Err(error);
    }

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
        let slot_cwd_key = format!("@ezm_slot_{}_cwd", binding.slot_id);
        let slot_mode_key = format!("@ezm_slot_{}_mode", binding.slot_id);
        let worktree_value = binding.worktree_path.display().to_string();
        set_or_verify_session_option(session_name, &slot_pane_key, &binding.pane_id)?;
        set_or_verify_session_option(session_name, &slot_worktree_key, &worktree_value)?;
        set_or_verify_session_option(session_name, &slot_cwd_key, &worktree_value)?;
        set_or_verify_session_option(session_name, &slot_mode_key, "shell")?;

        let pane_worktree_key = "@ezm_slot_worktree";
        let pane_slot_key = "@ezm_slot_id";
        let pane_cwd_key = "@ezm_slot_cwd";
        let pane_mode_key = "@ezm_slot_mode";
        set_or_verify_pane_option(
            &binding.pane_id,
            pane_slot_key,
            &binding.slot_id.to_string(),
        )?;
        set_or_verify_pane_option(&binding.pane_id, pane_worktree_key, &worktree_value)?;
        set_or_verify_pane_option(&binding.pane_id, pane_cwd_key, &worktree_value)?;
        set_or_verify_pane_option(&binding.pane_id, pane_mode_key, "shell")?;
    }
    Ok(())
}

fn kill_created_panes(created_panes: &[String]) -> Result<(), SessionError> {
    for pane_id in created_panes.iter().rev() {
        tmux_run(&["kill-pane", "-t", pane_id])?;
    }

    Ok(())
}
