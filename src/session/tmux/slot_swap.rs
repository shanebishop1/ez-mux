use std::sync::OnceLock;

use super::CANONICAL_SLOT_IDS;
use super::PaneWidthSample;
use super::SessionError;
use super::ZoomFlagSupport;
use super::command::{tmux_output_value, tmux_run};
use super::layout::{
    LAYOUT_MODE_FIVE_PANE, LAYOUT_MODE_KEY, LAYOUT_MODE_THREE_PANE, SLOT_SUSPENDED_KEY_PREFIX,
};
use super::options::{
    canonical_slot_mismatch_error, required_pane_option, required_session_option,
    show_session_option,
};
use super::pick_center_pane;
use super::tmux_diagnostics_exit_status;
use super::zoom_flag_support_for_command;

#[derive(Debug, Clone, Copy)]
struct ZoomFlagCapabilities {
    swap_pane: ZoomFlagSupport,
    select_pane: ZoomFlagSupport,
}

#[derive(Debug, Clone, Copy)]
struct SlotContinuitySnapshot<'a> {
    pane_id: &'a str,
    worktree: &'a str,
    cwd: &'a str,
    mode: &'a str,
}

impl Default for ZoomFlagCapabilities {
    fn default() -> Self {
        Self {
            swap_pane: ZoomFlagSupport::Unknown,
            select_pane: ZoomFlagSupport::Unknown,
        }
    }
}

static ZOOM_FLAG_CAPABILITIES: OnceLock<ZoomFlagCapabilities> = OnceLock::new();

pub(super) fn swap_slot_with_center(session_name: &str, slot_id: u8) -> Result<(), SessionError> {
    if !CANONICAL_SLOT_IDS.contains(&slot_id) {
        return Err(SessionError::SlotRegistry(
            super::super::SlotRegistryError::InvalidSlotId { slot_id },
        ));
    }

    validate_canonical_slot_registry(session_name)?;

    let slot_pane_key = format!("@ezm_slot_{slot_id}_pane");
    let slot_pane_id = required_session_option(session_name, &slot_pane_key)?;
    let center_pane_id = resolve_center_slot_pane(session_name)?;

    if slot_pane_id != center_pane_id {
        swap_panes_preserve_zoom(&slot_pane_id, &center_pane_id)?;
    }

    select_pane_preserve_zoom(&slot_pane_id)?;
    validate_canonical_slot_registry(session_name)?;

    Ok(())
}

pub(super) fn validate_canonical_slot_registry(session_name: &str) -> Result<(), SessionError> {
    let mut seen_panes = std::collections::HashSet::new();
    let layout_mode = show_session_option(session_name, LAYOUT_MODE_KEY)?
        .unwrap_or_else(|| LAYOUT_MODE_FIVE_PANE.to_owned());

    for slot_id in 1_u8..=5 {
        let pane_key = format!("@ezm_slot_{slot_id}_pane");
        let worktree_key = format!("@ezm_slot_{slot_id}_worktree");
        let cwd_key = format!("@ezm_slot_{slot_id}_cwd");
        let mode_key = format!("@ezm_slot_{slot_id}_mode");
        let pane_id = required_session_option(session_name, &pane_key)?;
        let worktree = required_session_option(session_name, &worktree_key)?;
        let cwd = required_session_option(session_name, &cwd_key)?;
        let mode = required_session_option(session_name, &mode_key)?;
        let suspended = show_session_option(
            session_name,
            &format!("{SLOT_SUSPENDED_KEY_PREFIX}{slot_id}_suspended"),
        )?
        .is_some_and(|value| value == "1");
        validate_slot_suspension(layout_mode.as_str(), slot_id, suspended)
            .map_err(|reason| canonical_slot_mismatch_error(session_name, reason.as_str()))?;

        if !seen_panes.insert(pane_id.clone()) {
            return Err(canonical_slot_mismatch_error(
                session_name,
                &format!("slot {slot_id} duplicates pane identity {pane_id}"),
            ));
        }

        if suspended {
            let restore_pane =
                required_session_option(session_name, &slot_restore_pane_key(slot_id))?;
            let restore_worktree =
                required_session_option(session_name, &slot_restore_worktree_key(slot_id))?;
            let restore_cwd =
                required_session_option(session_name, &slot_restore_cwd_key(slot_id))?;
            let restore_mode =
                required_session_option(session_name, &slot_restore_mode_key(slot_id))?;
            let current = SlotContinuitySnapshot {
                pane_id: &pane_id,
                worktree: &worktree,
                cwd: &cwd,
                mode: &mode,
            };
            let restore = SlotContinuitySnapshot {
                pane_id: &restore_pane,
                worktree: &restore_worktree,
                cwd: &restore_cwd,
                mode: &restore_mode,
            };
            validate_suspended_slot_restore_metadata(slot_id, current, restore)
                .map_err(|reason| canonical_slot_mismatch_error(session_name, reason.as_str()))?;
            continue;
        }

        let pane_slot_id = required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_id")?;
        if pane_slot_id != slot_id.to_string() {
            return Err(canonical_slot_mismatch_error(
                session_name,
                &format!("slot {slot_id} pane {pane_id} reports @ezm_slot_id={pane_slot_id}"),
            ));
        }

        let pane_worktree =
            required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_worktree")?;
        if pane_worktree != worktree {
            return Err(canonical_slot_mismatch_error(
                session_name,
                &format!(
                    "slot {slot_id} pane {pane_id} worktree mismatch session={worktree} pane={pane_worktree}"
                ),
            ));
        }
    }

    Ok(())
}

fn resolve_center_slot_pane(session_name: &str) -> Result<String, SessionError> {
    let mut samples = Vec::with_capacity(CANONICAL_SLOT_IDS.len());

    for slot_id in CANONICAL_SLOT_IDS {
        let pane_key = format!("@ezm_slot_{slot_id}_pane");
        let pane_id = required_session_option(session_name, &pane_key)?;

        if pane_is_dead(&pane_id)? {
            continue;
        }

        let width = pane_width(&pane_id)?;
        samples.push(PaneWidthSample {
            slot_id,
            pane_id,
            width,
        });
    }

    match pick_center_pane(&samples) {
        Some(pane_id) => Ok(pane_id.to_owned()),
        None => Err(SessionError::TmuxCommandFailed {
            command: format!("resolve-center-slot-pane -t {session_name}"),
            stderr: String::from("no live canonical panes were available to resolve center pane"),
        }),
    }
}

fn pane_is_dead(pane_id: &str) -> Result<bool, SessionError> {
    let value = tmux_output_value(&["display-message", "-p", "-t", pane_id, "#{pane_dead}"])?;
    Ok(value.trim() == "1")
}

fn pane_width(pane_id: &str) -> Result<u16, SessionError> {
    let value = tmux_output_value(&["display-message", "-p", "-t", pane_id, "#{pane_width}"])?;
    value
        .trim()
        .parse::<u16>()
        .map_err(|error| SessionError::TmuxCommandFailed {
            command: format!("display-message -p -t {pane_id} #{{pane_width}}"),
            stderr: format!("failed parsing pane width: {error}"),
        })
}

fn swap_panes_preserve_zoom(
    source_pane_id: &str,
    target_pane_id: &str,
) -> Result<(), SessionError> {
    let capabilities = zoom_flag_capabilities();
    let with_zoom_args = [
        "swap-pane",
        "-Z",
        "-s",
        source_pane_id,
        "-t",
        target_pane_id,
    ];
    let without_zoom_args = ["swap-pane", "-s", source_pane_id, "-t", target_pane_id];

    run_with_zoom_fallback(
        "swap-pane",
        capabilities.swap_pane,
        &with_zoom_args,
        &without_zoom_args,
    )
}

fn zoom_flag_capabilities() -> ZoomFlagCapabilities {
    *ZOOM_FLAG_CAPABILITIES.get_or_init(|| match tmux_output_value(&["list-commands"]) {
        Ok(command_listing) => ZoomFlagCapabilities {
            swap_pane: zoom_flag_support_for_command(&command_listing, "swap-pane"),
            select_pane: zoom_flag_support_for_command(&command_listing, "select-pane"),
        },
        Err(_) => ZoomFlagCapabilities::default(),
    })
}

fn run_with_zoom_fallback(
    command_name: &str,
    zoom_support: ZoomFlagSupport,
    with_zoom_args: &[&str],
    without_zoom_args: &[&str],
) -> Result<(), SessionError> {
    if zoom_support == ZoomFlagSupport::Unsupported {
        return tmux_run(without_zoom_args);
    }

    match tmux_run(with_zoom_args) {
        Ok(()) => Ok(()),
        Err(SessionError::TmuxCommandFailed { command, stderr })
            if should_retry_without_zoom(command_name, &command, &stderr) =>
        {
            tmux_run(without_zoom_args)
        }
        Err(error) => Err(error),
    }
}

fn select_pane_preserve_zoom(pane_id: &str) -> Result<(), SessionError> {
    let capabilities = zoom_flag_capabilities();
    let with_zoom_args = ["select-pane", "-Z", "-t", pane_id];
    let without_zoom_args = ["select-pane", "-t", pane_id];

    run_with_zoom_fallback(
        "select-pane",
        capabilities.select_pane,
        &with_zoom_args,
        &without_zoom_args,
    )
}

fn should_retry_without_zoom(command_name: &str, command: &str, stderr: &str) -> bool {
    command_starts_with_zoom_flag(command_name, command)
        && tmux_diagnostics_exit_status(stderr) == Some(1)
}

fn command_starts_with_zoom_flag(command_name: &str, command: &str) -> bool {
    let mut parts = command.split_ascii_whitespace();
    matches!(parts.next(), Some(name) if name == command_name)
        && matches!(parts.next(), Some(flag) if flag == "-Z")
}

fn validate_slot_suspension(layout_mode: &str, slot_id: u8, suspended: bool) -> Result<(), String> {
    if !suspended {
        return Ok(());
    }

    if layout_mode != LAYOUT_MODE_THREE_PANE {
        return Err(format!(
            "slot {slot_id} marked suspended while layout mode is {layout_mode}"
        ));
    }

    if !matches!(slot_id, 4 | 5) {
        return Err(format!(
            "slot {slot_id} cannot be suspended in canonical model"
        ));
    }

    Ok(())
}

fn validate_suspended_slot_restore_metadata(
    slot_id: u8,
    current: SlotContinuitySnapshot<'_>,
    restore: SlotContinuitySnapshot<'_>,
) -> Result<(), String> {
    if restore.pane_id != current.pane_id {
        return Err(format!(
            "slot {slot_id} suspended pane mismatch session={} restore={}",
            current.pane_id, restore.pane_id
        ));
    }

    if restore.worktree != current.worktree {
        return Err(format!(
            "slot {slot_id} suspended worktree mismatch session={} restore={}",
            current.worktree, restore.worktree
        ));
    }

    if restore.cwd != current.cwd {
        return Err(format!(
            "slot {slot_id} suspended cwd mismatch session={} restore={}",
            current.cwd, restore.cwd
        ));
    }

    if restore.mode != current.mode {
        return Err(format!(
            "slot {slot_id} suspended mode mismatch session={} restore={}",
            current.mode, restore.mode
        ));
    }

    Ok(())
}

fn slot_restore_pane_key(slot_id: u8) -> String {
    format!("@ezm_slot_{slot_id}_restore_pane")
}

fn slot_restore_worktree_key(slot_id: u8) -> String {
    format!("@ezm_slot_{slot_id}_restore_worktree")
}

fn slot_restore_cwd_key(slot_id: u8) -> String {
    format!("@ezm_slot_{slot_id}_restore_cwd")
}

fn slot_restore_mode_key(slot_id: u8) -> String {
    format!("@ezm_slot_{slot_id}_restore_mode")
}

#[cfg(test)]
mod tests {
    use super::SlotContinuitySnapshot;
    use super::should_retry_without_zoom;
    use super::validate_slot_suspension;
    use super::validate_suspended_slot_restore_metadata;
    use crate::session::tmux::layout::{LAYOUT_MODE_FIVE_PANE, LAYOUT_MODE_THREE_PANE};

    #[test]
    fn retries_only_for_zoom_attempts_with_status_one() {
        assert!(should_retry_without_zoom(
            "swap-pane",
            "swap-pane -Z -s %1 -t %2",
            "status=1; stdout=\"\"; stderr=\"unknown option -- Z\""
        ));
        assert!(!should_retry_without_zoom(
            "swap-pane",
            "swap-pane -s %1 -t %2",
            "status=1; stdout=\"\"; stderr=\"pane not found\""
        ));
        assert!(!should_retry_without_zoom(
            "swap-pane",
            "swap-pane -Z -s %1 -t %2",
            "status=127; stdout=\"\"; stderr=\"pane not found\""
        ));
    }

    #[test]
    fn suspension_only_allowed_for_slots_four_and_five_in_three_pane_mode() {
        assert!(validate_slot_suspension(LAYOUT_MODE_THREE_PANE, 4, true).is_ok());
        assert!(validate_slot_suspension(LAYOUT_MODE_THREE_PANE, 5, true).is_ok());
        assert!(
            validate_slot_suspension(LAYOUT_MODE_THREE_PANE, 3, true)
                .expect_err("slot 3 must reject suspension")
                .contains("cannot be suspended")
        );
        assert!(
            validate_slot_suspension(LAYOUT_MODE_FIVE_PANE, 4, true)
                .expect_err("five-pane mode must reject suspension")
                .contains("marked suspended")
        );
        assert!(validate_slot_suspension(LAYOUT_MODE_FIVE_PANE, 4, false).is_ok());
    }

    #[test]
    fn suspended_slots_require_restore_metadata_to_match_slot_identity() {
        assert!(
            validate_suspended_slot_restore_metadata(
                4,
                SlotContinuitySnapshot {
                    pane_id: "%4",
                    worktree: "wt-4",
                    cwd: "/repo/slot-4",
                    mode: "lazygit",
                },
                SlotContinuitySnapshot {
                    pane_id: "%4",
                    worktree: "wt-4",
                    cwd: "/repo/slot-4",
                    mode: "lazygit",
                }
            )
            .is_ok()
        );

        assert!(
            validate_suspended_slot_restore_metadata(
                4,
                SlotContinuitySnapshot {
                    pane_id: "%4",
                    worktree: "wt-4",
                    cwd: "/repo/slot-4",
                    mode: "lazygit",
                },
                SlotContinuitySnapshot {
                    pane_id: "%9",
                    worktree: "wt-4",
                    cwd: "/repo/slot-4",
                    mode: "lazygit",
                }
            )
            .expect_err("suspended metadata must keep slot pane identity")
            .contains("pane mismatch")
        );

        assert!(
            validate_suspended_slot_restore_metadata(
                5,
                SlotContinuitySnapshot {
                    pane_id: "%5",
                    worktree: "wt-5",
                    cwd: "/repo/slot-5",
                    mode: "shell",
                },
                SlotContinuitySnapshot {
                    pane_id: "%5",
                    worktree: "wt-override",
                    cwd: "/repo/slot-5",
                    mode: "shell",
                }
            )
            .expect_err("suspended metadata must keep worktree")
            .contains("worktree mismatch")
        );

        assert!(
            validate_suspended_slot_restore_metadata(
                5,
                SlotContinuitySnapshot {
                    pane_id: "%5",
                    worktree: "wt-5",
                    cwd: "/repo/slot-5",
                    mode: "shell",
                },
                SlotContinuitySnapshot {
                    pane_id: "%5",
                    worktree: "wt-5",
                    cwd: "/repo/other",
                    mode: "shell",
                }
            )
            .expect_err("suspended metadata must keep cwd")
            .contains("cwd mismatch")
        );

        assert!(
            validate_suspended_slot_restore_metadata(
                5,
                SlotContinuitySnapshot {
                    pane_id: "%5",
                    worktree: "wt-5",
                    cwd: "/repo/slot-5",
                    mode: "shell",
                },
                SlotContinuitySnapshot {
                    pane_id: "%5",
                    worktree: "wt-5",
                    cwd: "/repo/slot-5",
                    mode: "agent",
                }
            )
            .expect_err("suspended metadata must keep mode")
            .contains("mode mismatch")
        );
    }
}
