use std::collections::{BTreeSet, HashMap};

use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::command::tmux_output_value;
use super::options::{required_session_option, set_pane_option, set_session_option};
use super::slot_swap::validate_canonical_slot_registry;
use crate::session::SessionDamageAnalysis;
use crate::session::SessionRepairOutcome;

#[derive(Debug, Clone)]
struct SlotMetadata {
    pane_id: String,
    worktree: String,
    cwd: String,
    mode: String,
}

pub(super) fn analyze_session_damage(
    session_name: &str,
) -> Result<SessionDamageAnalysis, SessionError> {
    let slot_metadata = load_slot_metadata(session_name)?;
    let live_panes = list_live_window_panes(session_name)?;
    let slot_to_pane = slot_metadata
        .iter()
        .map(|(&slot_id, metadata)| (slot_id, metadata.pane_id.clone()))
        .collect::<HashMap<_, _>>();

    super::super::repair::analyze_slot_damage(&slot_to_pane, &live_panes)
}

pub(super) fn reconcile_session_damage(
    session_name: &str,
) -> Result<SessionRepairOutcome, SessionError> {
    let slot_metadata = load_slot_metadata(session_name)?;
    let live_panes = list_live_window_panes(session_name)?;

    reconcile_loaded_session_damage(
        session_name,
        slot_metadata,
        &live_panes,
        recreate_missing_slot,
        persist_slot_metadata,
        validate_canonical_slot_registry,
    )
}

fn reconcile_loaded_session_damage(
    session_name: &str,
    mut slot_metadata: HashMap<u8, SlotMetadata>,
    live_panes: &BTreeSet<String>,
    mut recreate_slot: impl FnMut(&str, u8, &HashMap<u8, SlotMetadata>) -> Result<String, SessionError>,
    mut persist_slot: impl FnMut(&str, u8, &SlotMetadata) -> Result<(), SessionError>,
    mut validate_slots: impl FnMut(&str) -> Result<(), SessionError>,
) -> Result<SessionRepairOutcome, SessionError> {
    let slot_to_pane = slot_metadata
        .iter()
        .map(|(&slot_id, metadata)| (slot_id, metadata.pane_id.clone()))
        .collect::<HashMap<_, _>>();

    let analysis = super::super::repair::analyze_slot_damage(&slot_to_pane, live_panes)?;
    if !analysis.has_damage() {
        return Ok(SessionRepairOutcome {
            session_name: session_name.to_owned(),
            healthy_slots: analysis.healthy_slots,
            recreated_slots: Vec::new(),
        });
    }

    for slot_id in &analysis.recreate_order {
        let new_pane_id = recreate_slot(session_name, *slot_id, &slot_metadata)?;
        let metadata =
            slot_metadata
                .get_mut(slot_id)
                .ok_or_else(|| SessionError::TmuxCommandFailed {
                    command: format!("reconcile-session-damage -t {session_name}"),
                    stderr: format!("slot metadata missing while reconciling slot {slot_id}"),
                })?;
        metadata.pane_id = new_pane_id;
        persist_slot(session_name, *slot_id, metadata)?;
    }

    validate_slots(session_name)?;

    Ok(SessionRepairOutcome {
        session_name: session_name.to_owned(),
        healthy_slots: analysis.healthy_slots,
        recreated_slots: analysis.recreate_order,
    })
}

fn load_slot_metadata(session_name: &str) -> Result<HashMap<u8, SlotMetadata>, SessionError> {
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

fn list_live_window_panes(session_name: &str) -> Result<BTreeSet<String>, SessionError> {
    let target = format!("{session_name}:0");
    let output = tmux_output_value(&["list-panes", "-t", &target, "-F", "#{pane_id}"])?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn recreate_missing_slot(
    session_name: &str,
    slot_id: u8,
    slot_metadata: &HashMap<u8, SlotMetadata>,
) -> Result<String, SessionError> {
    let target_slot = match slot_id {
        2..=4 => 1,
        5 => 3,
        _ => {
            return Err(SessionError::TmuxCommandFailed {
                command: format!("reconcile-session-damage -t {session_name}"),
                stderr: format!("slot {slot_id} is not eligible for selective reconcile"),
            });
        }
    };
    let target_pane_id = slot_metadata
        .get(&target_slot)
        .map(|metadata| metadata.pane_id.as_str())
        .ok_or_else(|| SessionError::TmuxCommandFailed {
            command: format!("reconcile-session-damage -t {session_name}"),
            stderr: format!("missing backing pane metadata for slot {target_slot}"),
        })?;
    let split_direction = if slot_id == 4 || slot_id == 5 {
        "-v"
    } else {
        "-h"
    };
    let pane_id = tmux_output_value(&[
        "split-window",
        split_direction,
        "-t",
        target_pane_id,
        "-P",
        "-F",
        "#{pane_id}",
    ])?;

    Ok(pane_id.trim().to_owned())
}

fn persist_slot_metadata(
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

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeSet;

    use super::{SlotMetadata, reconcile_loaded_session_damage};

    fn canonical_slot_metadata() -> std::collections::HashMap<u8, SlotMetadata> {
        std::collections::HashMap::from([
            (
                1_u8,
                SlotMetadata {
                    pane_id: String::from("%1"),
                    worktree: String::from("wt-1"),
                    cwd: String::from("/repo/slot-1"),
                    mode: String::from("agent"),
                },
            ),
            (
                2_u8,
                SlotMetadata {
                    pane_id: String::from("%2"),
                    worktree: String::from("wt-2"),
                    cwd: String::from("/repo/slot-2"),
                    mode: String::from("shell"),
                },
            ),
            (
                3_u8,
                SlotMetadata {
                    pane_id: String::from("%3"),
                    worktree: String::from("wt-3"),
                    cwd: String::from("/repo/slot-3"),
                    mode: String::from("neovim"),
                },
            ),
            (
                4_u8,
                SlotMetadata {
                    pane_id: String::from("%4"),
                    worktree: String::from("wt-4"),
                    cwd: String::from("/repo/slot-4"),
                    mode: String::from("lazygit"),
                },
            ),
            (
                5_u8,
                SlotMetadata {
                    pane_id: String::from("%5"),
                    worktree: String::from("wt-5"),
                    cwd: String::from("/repo/slot-5"),
                    mode: String::from("shell"),
                },
            ),
        ])
    }

    #[test]
    fn selective_reconcile_persists_context_only_for_recreated_slots() {
        let slot_metadata = canonical_slot_metadata();
        let live_panes = BTreeSet::from([
            String::from("%1"),
            String::from("%2"),
            String::from("%3"),
            String::from("%5"),
        ]);
        let persisted = RefCell::new(Vec::<(u8, String, String, String)>::new());
        let validated = RefCell::new(0_u8);

        let outcome = reconcile_loaded_session_damage(
            "ezm-session-ctx",
            slot_metadata,
            &live_panes,
            |_session_name, slot_id, _slot_metadata| {
                assert_eq!(slot_id, 4);
                Ok(String::from("%44"))
            },
            |_session_name, slot_id, metadata| {
                persisted.borrow_mut().push((
                    slot_id,
                    metadata.worktree.clone(),
                    metadata.cwd.clone(),
                    metadata.mode.clone(),
                ));
                Ok(())
            },
            |_session_name| {
                *validated.borrow_mut() += 1;
                Ok(())
            },
        )
        .expect("selective reconcile should succeed");

        assert_eq!(outcome.healthy_slots, vec![1, 2, 3, 5]);
        assert_eq!(outcome.recreated_slots, vec![4]);
        assert_eq!(
            persisted.into_inner(),
            vec![(
                4,
                String::from("wt-4"),
                String::from("/repo/slot-4"),
                String::from("lazygit"),
            )]
        );
        assert_eq!(validated.into_inner(), 1);
    }

    #[test]
    fn selective_reconcile_keeps_dependent_healthy_slot_context_untouched() {
        let slot_metadata = canonical_slot_metadata();
        let live_panes = BTreeSet::from([
            String::from("%1"),
            String::from("%2"),
            String::from("%4"),
            String::from("%5"),
        ]);
        let persisted_slot_ids = RefCell::new(Vec::<u8>::new());

        let outcome = reconcile_loaded_session_damage(
            "ezm-session-ctx",
            slot_metadata,
            &live_panes,
            |_session_name, slot_id, _slot_metadata| {
                assert_eq!(slot_id, 3);
                Ok(String::from("%33"))
            },
            |_session_name, slot_id, _metadata| {
                persisted_slot_ids.borrow_mut().push(slot_id);
                Ok(())
            },
            |_session_name| Ok(()),
        )
        .expect("selective reconcile should succeed");

        assert_eq!(outcome.healthy_slots, vec![1, 2, 4, 5]);
        assert_eq!(outcome.recreated_slots, vec![3]);
        assert_eq!(persisted_slot_ids.into_inner(), vec![3]);
    }
}
