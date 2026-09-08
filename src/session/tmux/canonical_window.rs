use std::collections::{BTreeMap, BTreeSet};

use super::SessionError;
use super::canonical_window_selection::{
    WindowPaneRow, WorkspaceShape, first_window_identity, parse_window_pane_rows,
    parse_window_pane_rows_with_window, select_canonical_window_for_workspace,
    window_represents_workspace,
};
use super::command::{tmux_output, tmux_output_value};
use super::options::{
    required_session_option, set_pane_option, set_session_option, show_session_option,
};

pub(super) const CANONICAL_WINDOW_ID_KEY: &str = "@ezm_canonical_window_id";

/// Resolves the window owned by ez-mux's managed workspace.
///
/// The active window is deliberately not part of this decision: users may
/// select an auxiliary or ordinary extra window while an internal operation
/// is running. Existing sessions are migrated by recovering a window that
/// contains a canonical pane and persisting its stable tmux window id.
pub(super) fn canonical_window_target(session_name: &str) -> Result<String, SessionError> {
    if let Some(window_id) = show_session_option(session_name, CANONICAL_WINDOW_ID_KEY)?
        .filter(|value| !value.trim().is_empty())
    {
        if window_is_managed(session_name, &window_id)? {
            return Ok(window_id);
        }
    }

    let recovered = recover_canonical_window(session_name)?;
    set_session_option(session_name, CANONICAL_WINDOW_ID_KEY, &recovered)?;
    Ok(recovered)
}

pub(super) fn remember_canonical_window(
    session_name: &str,
    window_id: &str,
) -> Result<(), SessionError> {
    set_session_option(session_name, CANONICAL_WINDOW_ID_KEY, window_id)
}

fn window_is_managed(session_name: &str, window_id: &str) -> Result<bool, SessionError> {
    let output = tmux_output(&[
        "display-message",
        "-p",
        "-t",
        window_id,
        "#{session_name}|#{window_id}",
    ])?;
    if !output.status.success() {
        return Ok(false);
    }

    let Some((actual_session, actual_window)) = first_window_identity(&output.stdout) else {
        return Ok(false);
    };
    if actual_session != session_name || actual_window != window_id.trim() {
        return Ok(false);
    }

    let live_panes = list_window_panes(window_id)?;
    let managed_panes = session_managed_pane_ids(session_name)?;
    let workspace = declared_workspace_shape(session_name)?;
    Ok(window_represents_workspace(
        &live_panes,
        &managed_panes,
        &workspace,
    ))
}

fn recover_canonical_window(session_name: &str) -> Result<String, SessionError> {
    let managed_panes = session_managed_pane_ids(session_name)?;
    let workspace = declared_workspace_shape(session_name)?;
    let output = tmux_output_value(&[
        "list-panes",
        "-a",
        "-t",
        session_name,
        "-F",
        "#{window_id}|#{pane_id}|#{@ezm_slot_id}",
    ])?;
    let candidates = parse_window_pane_rows(&output);

    select_canonical_window_for_workspace(&candidates, &managed_panes, &workspace).ok_or_else(
        || SessionError::TmuxCommandFailed {
            command: format!("resolve-canonical-window -t {session_name}"),
            stderr: String::from(
                "no managed canonical pane was found; canonical window metadata is stale",
            ),
        },
    )
}

fn declared_workspace_shape(session_name: &str) -> Result<WorkspaceShape, SessionError> {
    let layout_mode = show_session_option(session_name, "@ezm_layout_mode")?
        .unwrap_or_else(|| String::from("five-pane"));
    let (active_slots, allowed_suspended_slots) = match layout_mode.as_str() {
        "one-pane" => (BTreeSet::from([1]), BTreeSet::from([2, 3, 4, 5])),
        "two-pane" => (BTreeSet::from([1, 2]), BTreeSet::from([3, 4, 5])),
        "three-pane" => (BTreeSet::from([1, 2, 3]), BTreeSet::from([4, 5])),
        "four-pane" => (BTreeSet::from([1, 2, 3, 4]), BTreeSet::from([1, 5])),
        "five-pane" => ((1_u8..=5).collect(), BTreeSet::new()),
        _ => return Ok(WorkspaceShape::five_pane()),
    };

    let mut suspended_slots = BTreeSet::new();
    for slot_id in 1_u8..=5 {
        let key = format!("@ezm_slot_{slot_id}_suspended");
        if show_session_option(session_name, &key)?.is_some_and(|value| value.trim() == "1") {
            if !allowed_suspended_slots.contains(&slot_id) {
                return Err(SessionError::TmuxCommandFailed {
                    command: format!("validate-session-suspension-metadata -t {session_name}"),
                    stderr: format!(
                        "slot {slot_id} cannot be suspended in layout mode {layout_mode}"
                    ),
                });
            }
            suspended_slots.insert(slot_id);
        }
    }

    Ok(WorkspaceShape::new(active_slots, suspended_slots))
}

/// Resolves a canonical workspace for repair.  A one-pane workspace can be
/// destroyed when its only active pane is killed, so there may be no surviving
/// pane from which to recover the window.  In that narrow case create a new
/// detached window and persist its stable tmux id before listing its panes.
pub(super) fn canonical_window_target_for_repair(
    session_name: &str,
) -> Result<String, SessionError> {
    match canonical_window_target(session_name) {
        Ok(target) => Ok(target),
        Err(error) if canonical_window_missing(&error) => {
            let cwd = show_session_option(session_name, "@ezm_slot_1_cwd")?
                .filter(|value| !value.trim().is_empty());
            let mut args = vec!["new-window", "-d", "-t", session_name];
            if let Some(cwd) = cwd.as_deref() {
                args.extend(["-c", cwd]);
            }
            args.extend(["-P", "-F", "#{window_id}"]);
            let target = tmux_output_value(&args)?.trim().to_owned();
            if target.is_empty() {
                return Err(SessionError::TmuxCommandFailed {
                    command: format!("new-window -d -t {session_name} -P -F #{{window_id}}"),
                    stderr: String::from("tmux returned no canonical repair window id"),
                });
            }
            mark_repair_window(session_name, &target)?;
            remember_canonical_window(session_name, &target)?;
            Ok(target)
        }
        Err(error) => Err(error),
    }
}

fn mark_repair_window(session_name: &str, window_id: &str) -> Result<(), SessionError> {
    let pane_id = tmux_output_value(&["list-panes", "-t", window_id, "-F", "#{pane_id}"])?
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| SessionError::TmuxCommandFailed {
            command: format!("list-panes -t {window_id} -F #{{pane_id}}"),
            stderr: String::from("new canonical repair window has no pane anchor"),
        })?;

    let worktree = required_session_option(session_name, "@ezm_slot_1_worktree")?;
    let cwd = required_session_option(session_name, "@ezm_slot_1_cwd")?;
    let mode = required_session_option(session_name, "@ezm_slot_1_mode")?;

    set_session_option(session_name, "@ezm_slot_1_pane", &pane_id)?;
    set_pane_option(&pane_id, "@ezm_slot_id", "1")?;
    set_pane_option(&pane_id, "@ezm_slot_worktree", &worktree)?;
    set_pane_option(&pane_id, "@ezm_slot_cwd", &cwd)?;
    set_pane_option(&pane_id, "@ezm_slot_mode", &mode)
}

pub(super) fn canonical_window_anchor_pane(session_name: &str) -> Result<String, SessionError> {
    let target = canonical_window_target_for_repair(session_name)?;
    let output = tmux_output_value(&["list-panes", "-t", &target, "-F", "#{pane_id}"])?;
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| SessionError::TmuxCommandFailed {
            command: format!("list-panes -t {target} -F #{{pane_id}}"),
            stderr: String::from("canonical repair window has no pane anchor"),
        })
}

fn canonical_window_missing(error: &SessionError) -> bool {
    matches!(
        error,
        SessionError::TmuxCommandFailed { stderr, .. }
            if stderr.contains("no managed canonical pane was found")
    )
}

fn session_managed_pane_ids(session_name: &str) -> Result<BTreeMap<u8, String>, SessionError> {
    let mut pane_ids = BTreeMap::new();
    for slot_id in 1_u8..=5 {
        let key = format!("@ezm_slot_{slot_id}_pane");
        if let Some(pane_id) = show_session_option(session_name, &key)? {
            let pane_id = pane_id.trim();
            if !pane_id.is_empty() {
                pane_ids.insert(slot_id, pane_id.to_owned());
            }
        }
    }
    Ok(pane_ids)
}

fn list_window_panes(window_id: &str) -> Result<Vec<WindowPaneRow>, SessionError> {
    let output = tmux_output_value(&[
        "list-panes",
        "-t",
        window_id,
        "-F",
        "#{pane_id}|#{@ezm_slot_id}",
    ])?;
    Ok(parse_window_pane_rows_with_window(window_id, &output))
}
