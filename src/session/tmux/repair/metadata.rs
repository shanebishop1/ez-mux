use std::collections::{BTreeSet, HashMap};

use super::super::CANONICAL_SLOT_IDS;
use super::super::SessionError;
use super::super::command::{tmux_output, tmux_output_value, tmux_primary_window_target};
use super::super::options::{required_session_option, set_pane_option, set_session_option};

#[derive(Debug, Clone)]
pub(super) struct SlotMetadata {
    pub(super) pane_id: String,
    pub(super) worktree: String,
    pub(super) cwd: String,
    pub(super) mode: String,
}

pub(super) fn load_slot_metadata(
    session_name: &str,
) -> Result<HashMap<u8, SlotMetadata>, SessionError> {
    let mut metadata = HashMap::with_capacity(CANONICAL_SLOT_IDS.len());
    for slot_id in CANONICAL_SLOT_IDS {
        let pane_key = format!("@ezm_slot_{slot_id}_pane");
        let worktree_key = format!("@ezm_slot_{slot_id}_worktree");
        let cwd_key = format!("@ezm_slot_{slot_id}_cwd");
        let mode_key = format!("@ezm_slot_{slot_id}_mode");
        let pane_id = required_session_option(session_name, &pane_key)?;
        let worktree = required_session_option(session_name, &worktree_key)?;
        let cwd = required_session_option(session_name, &cwd_key)?;
        let mode = required_session_option(session_name, &mode_key)?;
        let _ = metadata.insert(
            slot_id,
            SlotMetadata {
                pane_id,
                worktree,
                cwd,
                mode,
            },
        );
    }
    Ok(metadata)
}

pub(super) fn list_live_window_panes(session_name: &str) -> Result<BTreeSet<String>, SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    let output = tmux_output_value(&["list-panes", "-t", &target, "-F", "#{pane_id}"])?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

pub(super) fn discover_live_slot_bindings(
    live_panes: &BTreeSet<String>,
) -> Result<HashMap<u8, String>, SessionError> {
    let mut bindings = HashMap::new();
    for pane_id in live_panes {
        let output = tmux_output(&[
            "show-options",
            "-p",
            "-q",
            "-v",
            "-t",
            pane_id,
            "@ezm_slot_id",
        ])?;
        if !output.status.success() {
            continue;
        }

        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let Some(slot_id) = parse_live_slot_binding(&value) else {
            continue;
        };
        bindings.entry(slot_id).or_insert_with(|| pane_id.clone());
    }

    Ok(bindings)
}

pub(super) fn parse_live_slot_binding(value: &str) -> Option<u8> {
    let slot_id = value.trim().parse::<u8>().ok()?;
    if CANONICAL_SLOT_IDS.contains(&slot_id) {
        Some(slot_id)
    } else {
        None
    }
}

pub(super) fn apply_recovered_slot_pane_bindings(
    slot_metadata: &mut HashMap<u8, SlotMetadata>,
    live_panes: &BTreeSet<String>,
    live_bindings: &HashMap<u8, String>,
) -> Vec<u8> {
    let mut recovered_slots = Vec::new();
    for (&slot_id, live_pane_id) in live_bindings {
        let Some(metadata) = slot_metadata.get_mut(&slot_id) else {
            continue;
        };
        if metadata.pane_id == *live_pane_id || live_panes.contains(&metadata.pane_id) {
            continue;
        }
        metadata.pane_id.clone_from(live_pane_id);
        recovered_slots.push(slot_id);
    }
    recovered_slots.sort_unstable();
    recovered_slots
}

pub(super) fn persist_slot_metadata(
    session_name: &str,
    slot_id: u8,
    metadata: &SlotMetadata,
) -> Result<(), SessionError> {
    let slot_pane_key = format!("@ezm_slot_{slot_id}_pane");
    set_session_option(session_name, &slot_pane_key, &metadata.pane_id)?;
    set_pane_option(&metadata.pane_id, "@ezm_slot_id", &slot_id.to_string())?;
    set_pane_option(&metadata.pane_id, "@ezm_slot_worktree", &metadata.worktree)?;
    set_pane_option(&metadata.pane_id, "@ezm_slot_cwd", &metadata.cwd)?;
    set_pane_option(&metadata.pane_id, "@ezm_slot_mode", &metadata.mode)
}
