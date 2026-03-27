use std::collections::{BTreeSet, HashMap};

use super::SessionError;
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
    let slot_metadata = metadata::load_slot_metadata(session_name)?;
    let live_panes = metadata::list_live_window_panes(session_name)?;
    let slot_to_pane = slot_metadata
        .iter()
        .map(|(&slot_id, metadata)| (slot_id, metadata.pane_id.clone()))
        .collect::<HashMap<_, _>>();

    super::super::repair::analyze_slot_damage(&slot_to_pane, &live_panes)
}

pub(super) fn reconcile_session_damage(
    session_name: &str,
) -> Result<SessionRepairOutcome, SessionError> {
    let launch_context = launch_context::resolve_repair_launch_context();
    let mut slot_metadata = metadata::load_slot_metadata(session_name)?;
    let live_panes = metadata::list_live_window_panes(session_name)?;
    recover_stale_slot_pane_bindings(session_name, &mut slot_metadata, &live_panes)?;

    let outcome = reconcile::reconcile_loaded_session_damage(
        session_name,
        slot_metadata,
        &live_panes,
        reconcile::recreate_missing_slot,
        metadata::persist_slot_metadata,
        validate_canonical_slot_registry,
    )?;

    if outcome.recreated_slots.is_empty() {
        return Ok(outcome);
    }

    geometry::restore_canonical_column_widths(session_name)?;
    apply_runtime_style_defaults(session_name)?;
    launch_context::restore_recreated_slot_modes(
        session_name,
        &outcome.recreated_slots,
        &launch_context,
    )?;

    Ok(outcome)
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
