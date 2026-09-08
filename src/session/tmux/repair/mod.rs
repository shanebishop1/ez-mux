use std::collections::{BTreeSet, HashMap};

use super::SessionError;
use super::layout::allowed_suspended_slots_for_layout_mode;
use super::options::set_session_option;
use super::slot_swap::validate_canonical_slot_registry;
use super::style::apply_runtime_style_defaults;
use crate::session::{SessionDamageAnalysis, SessionRepairOutcome};

mod geometry;
mod launch_context;
mod metadata;
mod reconcile;

#[cfg(test)]
mod tests;

pub(super) fn analyze_session_damage(
    session_name: &str,
) -> Result<SessionDamageAnalysis, SessionError> {
    let mut slot_metadata = metadata::load_slot_metadata(session_name)?;
    let live_panes = metadata::list_live_window_panes(session_name)?;
    recover_stale_slot_pane_bindings(session_name, &mut slot_metadata, &live_panes)?;
    let layout_mode = metadata::load_layout_mode(session_name)?;
    let suspended_slots = validate_suspension_metadata(&layout_mode, &slot_metadata)?;
    let slot_to_pane = slot_metadata
        .iter()
        .map(|(&slot_id, metadata)| (slot_id, metadata.pane_id.clone()))
        .collect::<HashMap<_, _>>();

    if suspended_slots.is_empty() {
        super::super::repair::analyze_slot_damage(&slot_to_pane, &live_panes)
    } else {
        super::super::repair::analyze_slot_damage_with_suspension(
            &slot_to_pane,
            &live_panes,
            &suspended_slots,
            &BTreeSet::new(),
        )
    }
}

pub(super) fn reconcile_session_damage(
    session_name: &str,
) -> Result<SessionRepairOutcome, SessionError> {
    reconcile_session_damage_for_slots(session_name, &BTreeSet::new(), &BTreeSet::new())
}

pub(super) fn reconcile_session_damage_for_slots(
    session_name: &str,
    required_slots: &BTreeSet<u8>,
    restore_suspended_slots: &BTreeSet<u8>,
) -> Result<SessionRepairOutcome, SessionError> {
    let launch_context = launch_context::resolve_repair_launch_context(session_name)?;
    let mut slot_metadata = metadata::load_slot_metadata(session_name)?;
    let live_panes = metadata::list_live_window_panes(session_name)?;
    recover_stale_slot_pane_bindings(session_name, &mut slot_metadata, &live_panes)?;
    let layout_mode = metadata::load_layout_mode(session_name)?;
    let suspended_slots = validate_suspension_metadata(&layout_mode, &slot_metadata)?;
    let required_slots_policy = (!required_slots.is_empty()).then_some(required_slots);

    let outcome = if suspended_slots.is_empty() && restore_suspended_slots.is_empty() {
        reconcile::reconcile_loaded_session_damage(
            session_name,
            slot_metadata,
            &live_panes,
            reconcile::ReconcileOperations {
                recreate_slot: Box::new(reconcile::recreate_missing_slot),
                persist_slot: Box::new(metadata::persist_slot_metadata),
                validate_slots: Box::new(validate_canonical_slot_registry),
            },
        )?
    } else {
        reconcile::reconcile_loaded_session_damage_with_policy(
            session_name,
            slot_metadata,
            &live_panes,
            &suspended_slots,
            restore_suspended_slots,
            required_slots_policy,
            reconcile::ReconcileOperations {
                recreate_slot: Box::new(reconcile::recreate_missing_slot),
                persist_slot: Box::new(metadata::persist_slot_metadata),
                validate_slots: Box::new(validate_canonical_slot_registry),
            },
        )?
    };

    if outcome.recreated_slots.is_empty() {
        return Ok(outcome);
    }

    geometry::restore_repaired_layout_geometry(session_name, &layout_mode)?;
    apply_runtime_style_defaults(session_name)?;
    launch_context::restore_recreated_slot_modes(
        session_name,
        &outcome.recreated_slots,
        &launch_context,
    )?;

    Ok(outcome)
}

fn validate_suspension_metadata(
    layout_mode: &str,
    slot_metadata: &HashMap<u8, metadata::SlotMetadata>,
) -> Result<BTreeSet<u8>, SessionError> {
    let allowed = allowed_suspended_slots_for_layout_mode(layout_mode).ok_or_else(|| {
        SessionError::TmuxCommandFailed {
            command: String::from("validate-session-suspension-metadata"),
            stderr: format!("unknown canonical layout mode {layout_mode}"),
        }
    })?;
    let mut suspended = BTreeSet::new();
    for (&slot_id, metadata) in slot_metadata {
        if !metadata.suspended {
            continue;
        }
        if !allowed.contains(&slot_id) {
            return Err(SessionError::TmuxCommandFailed {
                command: String::from("validate-session-suspension-metadata"),
                stderr: format!("slot {slot_id} cannot be suspended in layout mode {layout_mode}"),
            });
        }
        suspended.insert(slot_id);
    }
    Ok(suspended)
}

fn recover_stale_slot_pane_bindings(
    session_name: &str,
    slot_metadata: &mut HashMap<u8, metadata::SlotMetadata>,
    live_panes: &BTreeSet<String>,
) -> Result<(), SessionError> {
    let live_bindings = metadata::discover_live_slot_bindings(live_panes)?;
    let recovered_slots =
        metadata::apply_recovered_slot_pane_bindings(slot_metadata, live_panes, &live_bindings);

    for slot_id in recovered_slots {
        let key = format!("@ezm_slot_{slot_id}_pane");
        let Some(metadata) = slot_metadata.get(&slot_id) else {
            continue;
        };
        set_session_option(session_name, &key, &metadata.pane_id)?;
    }

    Ok(())
}
