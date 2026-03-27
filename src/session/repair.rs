use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::TmuxClient;
use super::resolve_session_identity;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDamageAnalysis {
    pub healthy_slots: Vec<u8>,
    pub missing_visible_slots: Vec<u8>,
    pub missing_backing_slots: Vec<u8>,
    pub recreate_order: Vec<u8>,
}

impl SessionDamageAnalysis {
    #[must_use]
    pub fn has_damage(&self) -> bool {
        !self.missing_visible_slots.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRepairOutcome {
    pub session_name: String,
    pub healthy_slots: Vec<u8>,
    pub recreated_slots: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRepairExecution {
    pub session_name: String,
    pub healthy_slots: Vec<u8>,
    pub missing_visible_slots: Vec<u8>,
    pub missing_backing_slots: Vec<u8>,
    pub recreate_order: Vec<u8>,
    pub recreated_slots: Vec<u8>,
}

impl SessionRepairExecution {
    #[must_use]
    pub fn action_label(&self) -> &'static str {
        if self.recreated_slots.is_empty() {
            "noop"
        } else {
            "reconcile"
        }
    }
}

/// Analyzes canonical slot metadata against live panes in tmux.
///
/// # Errors
/// Returns an error when tmux metadata cannot be read.
pub fn analyze_session_damage(
    session_name: &str,
    tmux: &impl TmuxClient,
) -> Result<SessionDamageAnalysis, SessionError> {
    tmux.analyze_session_damage(session_name)
}

/// Recreates only missing canonical panes for one session.
///
/// # Errors
/// Returns an error when selective reconcile cannot safely proceed.
pub fn reconcile_session_damage(
    session_name: &str,
    tmux: &impl TmuxClient,
) -> Result<SessionRepairOutcome, SessionError> {
    tmux.reconcile_session_damage(session_name)
}

/// Repairs the current project's tmux session when damage is detected.
///
/// # Errors
/// Returns an error when project/session resolution fails or tmux reconcile
/// cannot safely complete.
pub fn repair_current_project_session(
    tmux: &impl TmuxClient,
) -> Result<SessionRepairExecution, SessionError> {
    let project_dir = std::env::current_dir().map_err(SessionError::CurrentDir)?;
    repair_project_session(&project_dir, tmux)
}

/// Repairs the current project's tmux session and re-attaches when interactive.
///
/// # Errors
/// Returns an error when project/session resolution fails, tmux reconcile cannot
/// safely complete, or interactive attach fails.
pub fn repair_current_project_session_and_attach(
    tmux: &impl TmuxClient,
) -> Result<SessionRepairExecution, SessionError> {
    let project_dir = std::env::current_dir().map_err(SessionError::CurrentDir)?;
    repair_project_session_and_attach(&project_dir, tmux)
}

/// Repairs one resolved project session when damage is detected.
///
/// # Errors
/// Returns an error when session resolution fails or tmux reconcile cannot
/// safely complete.
pub fn repair_project_session(
    project_dir: &Path,
    tmux: &impl TmuxClient,
) -> Result<SessionRepairExecution, SessionError> {
    let session_name = resolve_session_identity(project_dir)?.session_name;
    let analysis = analyze_session_damage(&session_name, tmux)?;

    let recreated_slots = if analysis.has_damage() {
        reconcile_session_damage(&session_name, tmux)?.recreated_slots
    } else {
        Vec::new()
    };

    Ok(SessionRepairExecution {
        session_name,
        healthy_slots: analysis.healthy_slots,
        missing_visible_slots: analysis.missing_visible_slots,
        missing_backing_slots: analysis.missing_backing_slots,
        recreate_order: analysis.recreate_order,
        recreated_slots,
    })
}

/// Repairs one resolved project session and re-attaches when interactive.
///
/// # Errors
/// Returns an error when session resolution fails, reconcile cannot safely
/// complete, or interactive attach fails.
pub fn repair_project_session_and_attach(
    project_dir: &Path,
    tmux: &impl TmuxClient,
) -> Result<SessionRepairExecution, SessionError> {
    let execution = repair_project_session(project_dir, tmux)?;
    tmux.attach_session(&execution.session_name)?;
    Ok(execution)
}

pub(crate) fn analyze_slot_damage(
    slot_to_pane: &HashMap<u8, String>,
    live_panes: &BTreeSet<String>,
) -> Result<SessionDamageAnalysis, SessionError> {
    let mut healthy_slots = Vec::new();
    let mut missing_visible_slots = Vec::new();

    for slot_id in CANONICAL_SLOT_IDS {
        let pane_id =
            slot_to_pane
                .get(&slot_id)
                .ok_or_else(|| SessionError::TmuxCommandFailed {
                    command: String::from("analyze-session-damage"),
                    stderr: format!("missing required session slot pane option for slot {slot_id}"),
                })?;
        if live_panes.contains(pane_id) {
            healthy_slots.push(slot_id);
        } else {
            missing_visible_slots.push(slot_id);
        }
    }

    if missing_visible_slots.is_empty() {
        return Ok(SessionDamageAnalysis {
            healthy_slots,
            missing_visible_slots,
            missing_backing_slots: Vec::new(),
            recreate_order: Vec::new(),
        });
    }

    let mut missing_slots_set = BTreeSet::new();
    for slot_id in &missing_visible_slots {
        let _ = missing_slots_set.insert(*slot_id);
    }

    let mut changed = true;
    while changed {
        changed = false;
        let current = missing_slots_set.iter().copied().collect::<Vec<_>>();
        for slot_id in current {
            if let Some(backing_slot) = required_backing_slot(slot_id) {
                let backing_pane = slot_to_pane.get(&backing_slot).ok_or_else(|| {
                    SessionError::TmuxCommandFailed {
                        command: String::from("analyze-session-damage"),
                        stderr: format!(
                            "missing required backing session slot pane option for slot {backing_slot}"
                        ),
                    }
                })?;
                if !live_panes.contains(backing_pane) && missing_slots_set.insert(backing_slot) {
                    changed = true;
                }
            }
        }
    }

    if missing_slots_set.contains(&1) {
        return Err(SessionError::TmuxCommandFailed {
            command: String::from("analyze-session-damage"),
            stderr: String::from(
                "slot 1 pane is missing; selective reconcile is unsafe and requires full reset",
            ),
        });
    }

    let mut missing_backing_slots = missing_slots_set
        .iter()
        .copied()
        .filter(|slot_id| !missing_visible_slots.contains(slot_id))
        .collect::<Vec<_>>();
    missing_backing_slots.sort_unstable();

    let mut recreate_order = Vec::new();
    for slot_id in CANONICAL_SLOT_IDS {
        append_slot_with_backing(slot_id, &missing_slots_set, &mut recreate_order);
    }

    Ok(SessionDamageAnalysis {
        healthy_slots,
        missing_visible_slots,
        missing_backing_slots,
        recreate_order,
    })
}

#[must_use]
pub(crate) fn required_backing_slot(slot_id: u8) -> Option<u8> {
    match slot_id {
        2..=4 => Some(1),
        5 => Some(3),
        _ => None,
    }
}

fn append_slot_with_backing(slot_id: u8, missing: &BTreeSet<u8>, ordered: &mut Vec<u8>) {
    if !missing.contains(&slot_id) || ordered.contains(&slot_id) {
        return;
    }
    if let Some(backing_slot) = required_backing_slot(slot_id) {
        append_slot_with_backing(backing_slot, missing, ordered);
    }
    ordered.push(slot_id);
}

#[cfg(test)]
mod tests;
