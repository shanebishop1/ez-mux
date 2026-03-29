use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::SlotMode;
use super::options::required_session_option;
use super::slot_swap::validate_canonical_slot_registry;
use super::style::refresh_active_border_for_slot;
use crate::session::{SlotModeLaunchContext, mode_launch_contract};

mod cwd;
mod launch;
mod metadata;
mod opencode_theme;
mod pane_runtime;
mod persistent;
mod remote_launch;
mod startup;

#[cfg(test)]
mod tests;

#[cfg(test)]
use launch::{launch_agent_attach_command, launch_command_for_mode};
use metadata::{
    ModeMetadataState, apply_mode_metadata, compensate_mode_metadata, load_previous_mode_metadata,
    verify_mode_metadata,
};
use persistent::activate_mode_pane;
#[cfg(test)]
use remote_launch::launch_command_with_remote_dir_from_mapping;
#[cfg(test)]
use startup::{resolve_mode_switch_cwd, startup_mode_signal_enabled, use_startup_fast_path};

struct SlotModeKeys {
    pane: String,
    worktree: String,
    cwd: String,
    mode: String,
}

pub(super) fn switch_slot_mode(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    launch_context: SlotModeLaunchContext<'_>,
) -> Result<(), SessionError> {
    let startup = startup::startup_mode_signal_present();
    switch_slot_mode_internal(session_name, slot_id, mode, launch_context, startup)
}

pub(super) fn cleanup_legacy_mode_cache_sessions(session_name: &str) -> Result<(), SessionError> {
    persistent::cleanup_legacy_mode_cache_sessions(session_name)
}

pub(super) fn switch_slot_mode_for_repair(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    launch_context: SlotModeLaunchContext<'_>,
) -> Result<(), SessionError> {
    switch_slot_mode_internal(session_name, slot_id, mode, launch_context, true)
}

fn switch_slot_mode_internal(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    launch_context: SlotModeLaunchContext<'_>,
    prefer_assigned_worktree_cwd: bool,
) -> Result<(), SessionError> {
    let startup_fast_path = startup::use_startup_fast_path(prefer_assigned_worktree_cwd);

    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    let slot_pane_key = format!("@ezm_slot_{slot_id}_pane");
    let slot_worktree_key = format!("@ezm_slot_{slot_id}_worktree");
    let slot_cwd_key = format!("@ezm_slot_{slot_id}_cwd");
    let slot_mode_key = format!("@ezm_slot_{slot_id}_mode");
    let keys = SlotModeKeys {
        pane: slot_pane_key,
        worktree: slot_worktree_key,
        cwd: slot_cwd_key,
        mode: slot_mode_key,
    };

    if startup_fast_path {
        return switch_slot_mode_startup_path(
            session_name,
            slot_id,
            mode,
            launch_context,
            prefer_assigned_worktree_cwd,
            &keys,
        );
    }

    switch_slot_mode_persistent_path(session_name, slot_id, mode, launch_context, &keys)
}

fn switch_slot_mode_startup_path(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    launch_context: SlotModeLaunchContext<'_>,
    prefer_assigned_worktree_cwd: bool,
    keys: &SlotModeKeys,
) -> Result<(), SessionError> {
    let pane_id = required_session_option(session_name, &keys.pane)?;
    let worktree = required_session_option(session_name, &keys.worktree)?;
    let current_cwd =
        startup::resolve_mode_switch_cwd(prefer_assigned_worktree_cwd, &worktree, || {
            cwd::capture_slot_cwd(session_name, slot_id, &pane_id, &keys.cwd, &worktree)
        })?;
    let contract = mode_launch_contract(mode);
    let launch_command = launch::launch_command_for_mode(
        slot_id,
        mode,
        &contract.launch_command,
        &current_cwd,
        launch_context,
    )?;
    pane_runtime::run_teardown_hooks(&pane_id, &contract.teardown_hooks)?;
    pane_runtime::respawn_slot_mode(&pane_id, &current_cwd, &launch_command)?;

    let target = ModeMetadataState {
        session_cwd: current_cwd.clone(),
        session_mode: mode.label().to_owned(),
        pane_cwd: current_cwd,
        pane_mode: mode.label().to_owned(),
        pane_worktree: worktree,
    };

    apply_mode_metadata(session_name, &keys.cwd, &keys.mode, &pane_id, &target)?;
    refresh_active_border_for_slot(session_name, slot_id)?;
    Ok(())
}

fn switch_slot_mode_persistent_path(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
    launch_context: SlotModeLaunchContext<'_>,
    keys: &SlotModeKeys,
) -> Result<(), SessionError> {
    validate_canonical_slot_registry(session_name)?;

    let pane_id = required_session_option(session_name, &keys.pane)?;
    let worktree = required_session_option(session_name, &keys.worktree)?;

    let previous =
        load_previous_mode_metadata(session_name, slot_id, &keys.cwd, &keys.mode, &pane_id)?;

    let current_cwd = resolve_persistent_transition_cwd(&previous.session_mode, &worktree, || {
        cwd::capture_slot_cwd(session_name, slot_id, &pane_id, &keys.cwd, &worktree)
    })?;

    if previous.session_mode == mode.label() {
        refresh_active_border_for_slot(session_name, slot_id)?;
        return Ok(());
    }

    let contract = mode_launch_contract(mode);
    let launch_command = launch::launch_command_for_mode(
        slot_id,
        mode,
        &contract.launch_command,
        &current_cwd,
        launch_context,
    )?;

    let activation_spec = persistent::ModeActivationSpec {
        current_mode: &previous.session_mode,
        target_mode: mode.label(),
        launch_cwd: &current_cwd,
        worktree: &worktree,
        launch_command: &launch_command,
    };
    let activated = activate_mode_pane(session_name, slot_id, &pane_id, &activation_spec)?;

    let target = ModeMetadataState {
        session_cwd: activated.pane_cwd.clone(),
        session_mode: mode.label().to_owned(),
        pane_cwd: activated.pane_cwd,
        pane_mode: mode.label().to_owned(),
        pane_worktree: worktree,
    };

    if let Err(error) = apply_mode_metadata(
        session_name,
        &keys.cwd,
        &keys.mode,
        &activated.pane_id,
        &target,
    ) {
        return compensate_mode_metadata(
            session_name,
            slot_id,
            &keys.cwd,
            &keys.mode,
            &activated.pane_id,
            &previous,
            error,
        );
    }

    verify_mode_metadata(
        session_name,
        slot_id,
        &keys.cwd,
        &keys.mode,
        &activated.pane_id,
        &target,
    )?;

    validate_canonical_slot_registry(session_name)?;
    refresh_active_border_for_slot(session_name, slot_id)?;
    Ok(())
}

fn resolve_persistent_transition_cwd<F>(
    previous_mode: &str,
    assigned_worktree: &str,
    captured_cwd: F,
) -> Result<String, SessionError>
where
    F: FnOnce() -> Result<String, SessionError>,
{
    if previous_mode == SlotMode::Agent.label() {
        return Ok(assigned_worktree.to_owned());
    }

    captured_cwd()
}
