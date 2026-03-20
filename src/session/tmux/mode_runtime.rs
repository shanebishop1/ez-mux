use super::CANONICAL_SLOT_IDS;
use super::SessionError;
use super::SlotMode;
use super::command::{tmux_output_value, tmux_run};
use super::options::{
    required_pane_option, required_session_option, set_pane_option, set_session_option,
    show_session_option,
};
use super::slot_swap::validate_canonical_slot_registry;
use crate::session::{TeardownHook, mode_launch_contract};

pub(super) fn switch_slot_mode(
    session_name: &str,
    slot_id: u8,
    mode: SlotMode,
) -> Result<(), SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    validate_canonical_slot_registry(session_name)?;
    let slot_pane_key = format!("@ezm_slot_{slot_id}_pane");
    let slot_worktree_key = format!("@ezm_slot_{slot_id}_worktree");
    let slot_cwd_key = format!("@ezm_slot_{slot_id}_cwd");
    let slot_mode_key = format!("@ezm_slot_{slot_id}_mode");

    let pane_id = required_session_option(session_name, &slot_pane_key)?;
    let worktree = required_session_option(session_name, &slot_worktree_key)?;
    let current_cwd = capture_slot_cwd(session_name, slot_id, &pane_id, &slot_cwd_key, &worktree)?;
    let pane_slot_id = required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_id")?;
    if pane_slot_id != slot_id.to_string() {
        return Err(SessionError::TmuxCommandFailed {
            command: format!("switch-slot-mode -t {session_name} --slot {slot_id}"),
            stderr: format!(
                "slot metadata mismatch: pane {pane_id} has @ezm_slot_id={pane_slot_id}"
            ),
        });
    }

    let contract = mode_launch_contract(mode);
    run_teardown_hooks(&pane_id, &contract.teardown_hooks)?;
    respawn_slot_mode(&pane_id, &current_cwd, &contract.launch_command)?;

    set_session_option(session_name, &slot_cwd_key, &current_cwd)?;
    set_session_option(session_name, &slot_mode_key, mode.label())?;
    set_pane_option(&pane_id, "@ezm_slot_cwd", &current_cwd)?;
    set_pane_option(&pane_id, "@ezm_slot_mode", mode.label())?;
    set_pane_option(&pane_id, "@ezm_slot_worktree", &worktree)?;

    validate_canonical_slot_registry(session_name)?;
    Ok(())
}

fn capture_slot_cwd(
    session_name: &str,
    slot_id: u8,
    pane_id: &str,
    slot_cwd_key: &str,
    fallback_worktree: &str,
) -> Result<String, SessionError> {
    let pane_path = tmux_output_value(&[
        "display-message",
        "-p",
        "-t",
        pane_id,
        "#{pane_current_path}",
    ])?;
    let pane_path = pane_path.trim();
    if !pane_path.is_empty() {
        return Ok(pane_path.to_owned());
    }

    if let Some(existing) = show_session_option(session_name, slot_cwd_key)? {
        if !existing.trim().is_empty() {
            return Ok(existing.trim().to_owned());
        }
    }

    if !fallback_worktree.trim().is_empty() {
        return Ok(fallback_worktree.to_owned());
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("capture-slot-cwd -t {session_name} --slot {slot_id}"),
        stderr: String::from("slot cwd capture returned empty path"),
    })
}

fn run_teardown_hooks(pane_id: &str, hooks: &[TeardownHook]) -> Result<(), SessionError> {
    for hook in hooks {
        match hook {
            TeardownHook::SendCtrlC => {
                tmux_run(&["send-keys", "-t", pane_id, "C-c"])?;
            }
        }
    }

    Ok(())
}

fn respawn_slot_mode(pane_id: &str, cwd: &str, launch_command: &str) -> Result<(), SessionError> {
    let shell_command = format!("sh -lc '{}'", escape_single_quotes(launch_command));
    tmux_run(&[
        "respawn-pane",
        "-k",
        "-t",
        pane_id,
        "-c",
        cwd,
        &shell_command,
    ])
}

fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}
