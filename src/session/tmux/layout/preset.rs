use crate::session::three_pane_target_widths;
use crate::session::three_pane_widths_within_tolerance;

use super::super::DEFAULT_CENTER_WIDTH_PCT;
use super::super::LayoutPreset;
use super::super::SessionError;
use super::super::canonical_five_pane_column_widths;
use super::super::command::{
    format_output_diagnostics, tmux_output, tmux_output_value, tmux_primary_window_target, tmux_run,
};
use super::super::options::{
    required_pane_option, required_session_option, set_session_option, show_session_option,
};
use super::super::repair::reconcile_session_damage;
use super::super::slot_swap::validate_canonical_slot_registry;
use super::super::style::apply_runtime_style_defaults;
use super::LAYOUT_MODE_FIVE_PANE;
use super::LAYOUT_MODE_KEY;
use super::LAYOUT_MODE_THREE_PANE;

const SLOT_SUSPENDED_KEY_PREFIX: &str = "@ezm_slot_";
const RESTORE_WIDTH_KEY_PREFIX: &str = "@ezm_restore_width_slot_";

#[derive(Debug, Clone)]
struct SlotRestoreMetadata {
    pane_id: String,
    worktree: String,
    cwd: String,
    mode: String,
}

pub(super) fn apply_layout_preset(
    session_name: &str,
    preset: LayoutPreset,
) -> Result<(), SessionError> {
    match preset {
        LayoutPreset::ThreePane => apply_or_restore_three_pane_preset(session_name),
    }
}

pub(super) fn slot_suspended_key(slot_id: u8) -> String {
    format!("{SLOT_SUSPENDED_KEY_PREFIX}{slot_id}_suspended")
}

fn apply_or_restore_three_pane_preset(session_name: &str) -> Result<(), SessionError> {
    let layout_mode = show_session_option(session_name, LAYOUT_MODE_KEY)?
        .unwrap_or_else(|| LAYOUT_MODE_FIVE_PANE.to_owned());

    if is_three_pane_mode(layout_mode.as_str()) {
        return restore_five_pane_layout(session_name);
    }

    apply_three_pane_preset(session_name)
}

fn apply_three_pane_preset(session_name: &str) -> Result<(), SessionError> {
    let left_pane = required_session_option(session_name, "@ezm_slot_2_pane")?;
    let center_pane = required_session_option(session_name, "@ezm_slot_1_pane")?;
    let right_pane = required_session_option(session_name, "@ezm_slot_3_pane")?;

    persist_restore_width_targets(session_name, &left_pane, &center_pane, &right_pane)?;

    for slot_id in [4_u8, 5] {
        let pane_key = format!("@ezm_slot_{slot_id}_pane");
        let pane_id = required_session_option(session_name, &pane_key)?;
        persist_slot_suspension_metadata(session_name, slot_id, &pane_id)?;
        kill_pane_if_present(&pane_id)?;
    }

    let target = tmux_primary_window_target(session_name)?;

    let window_width =
        tmux_output_value(&["display-message", "-p", "-t", &target, "#{window_width}"])?
            .trim()
            .parse::<u16>()
            .map_err(|error| SessionError::TmuxCommandFailed {
                command: format!("display-message -p -t {target} #{{window_width}}"),
                stderr: format!("failed parsing window width: {error}"),
            })?;
    let (left_target, center_target, _right_target) = three_pane_target_widths(window_width);

    let (_, _, right_target) = three_pane_target_widths(window_width);
    let mut measured = resize_three_pane_columns(
        &left_pane,
        &center_pane,
        &right_pane,
        left_target,
        center_target,
        right_target,
        window_width,
    )?;

    if !three_pane_widths_within_tolerance(measured.0, measured.1, measured.2, window_width) {
        measured = resize_three_pane_columns(
            &left_pane,
            &center_pane,
            &right_pane,
            left_target,
            center_target,
            right_target,
            window_width,
        )?;
    }

    let (left_width, center_width, right_width) = measured;
    if !three_pane_widths_within_tolerance(left_width, center_width, right_width, window_width) {
        return Err(SessionError::TmuxCommandFailed {
            command: format!("apply-three-pane-preset -t {target}"),
            stderr: format!(
                "width tolerance violated: left={left_width}; center={center_width}; right={right_width}; window={window_width}"
            ),
        });
    }

    set_session_option(session_name, LAYOUT_MODE_KEY, LAYOUT_MODE_THREE_PANE)?;
    validate_canonical_slot_registry(session_name)?;
    apply_runtime_style_defaults(session_name)?;

    Ok(())
}

fn resize_three_pane_columns(
    left_pane: &str,
    center_pane: &str,
    right_pane: &str,
    left_target: u16,
    center_target: u16,
    right_target: u16,
    window_width: u16,
) -> Result<(u16, u16, u16), SessionError> {
    let attempts: [[(&str, u16); 3]; 4] = [
        [
            (left_pane, left_target),
            (right_pane, right_target),
            (center_pane, center_target),
        ],
        [
            (right_pane, right_target),
            (left_pane, left_target),
            (center_pane, center_target),
        ],
        [
            (left_pane, left_target),
            (center_pane, center_target),
            (right_pane, right_target),
        ],
        [
            (center_pane, center_target),
            (left_pane, left_target),
            (right_pane, right_target),
        ],
    ];

    for attempt in attempts {
        for (pane, target_width) in attempt {
            tmux_run(&["resize-pane", "-t", pane, "-x", &target_width.to_string()])?;
        }

        let measured = (
            pane_width(left_pane)?,
            pane_width(center_pane)?,
            pane_width(right_pane)?,
        );
        if three_pane_widths_within_tolerance(measured.0, measured.1, measured.2, window_width) {
            return Ok(measured);
        }
    }

    Ok((
        pane_width(left_pane)?,
        pane_width(center_pane)?,
        pane_width(right_pane)?,
    ))
}

fn restore_five_pane_layout(session_name: &str) -> Result<(), SessionError> {
    let slot_four_restore = load_slot_restore_metadata(session_name, 4)?;
    let slot_five_restore = load_slot_restore_metadata(session_name, 5)?;

    let _ = reconcile_session_damage(session_name)?;

    verify_restored_slot_continuity(session_name, 4, &slot_four_restore)?;
    verify_restored_slot_continuity(session_name, 5, &slot_five_restore)?;

    let target = tmux_primary_window_target(session_name)?;
    let left_pane = required_session_option(session_name, "@ezm_slot_2_pane")?;
    let center_pane = required_session_option(session_name, "@ezm_slot_1_pane")?;
    let right_pane = required_session_option(session_name, "@ezm_slot_3_pane")?;

    let window_width =
        tmux_output_value(&["display-message", "-p", "-t", &target, "#{window_width}"])?
            .trim()
            .parse::<u16>()
            .map_err(|error| SessionError::TmuxCommandFailed {
                command: format!("display-message -p -t {target} #{{window_width}}"),
                stderr: format!("failed parsing window width: {error}"),
            })?;
    let (left_target, center_target, right_target) =
        load_restore_width_targets(session_name, window_width)?;

    tmux_run(&[
        "resize-pane",
        "-t",
        &left_pane,
        "-x",
        &left_target.to_string(),
    ])?;
    tmux_run(&[
        "resize-pane",
        "-t",
        &center_pane,
        "-x",
        &center_target.to_string(),
    ])?;
    tmux_run(&[
        "resize-pane",
        "-t",
        &right_pane,
        "-x",
        &right_target.to_string(),
    ])?;

    clear_slot_suspension_metadata(session_name, 4)?;
    clear_slot_suspension_metadata(session_name, 5)?;
    set_session_option(session_name, LAYOUT_MODE_KEY, LAYOUT_MODE_FIVE_PANE)?;
    validate_canonical_slot_registry(session_name)?;
    apply_runtime_style_defaults(session_name)?;

    Ok(())
}

fn persist_slot_suspension_metadata(
    session_name: &str,
    slot_id: u8,
    pane_id: &str,
) -> Result<(), SessionError> {
    let worktree = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_worktree"))?;
    let cwd = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_cwd"))?;
    let mode = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_mode"))?;
    set_session_option(session_name, &slot_suspended_key(slot_id), "1")?;
    set_session_option(session_name, &slot_restore_pane_key(slot_id), pane_id)?;
    set_session_option(session_name, &slot_restore_worktree_key(slot_id), &worktree)?;
    set_session_option(session_name, &slot_restore_cwd_key(slot_id), &cwd)?;
    set_session_option(session_name, &slot_restore_mode_key(slot_id), &mode)
}

fn persist_restore_width_targets(
    session_name: &str,
    left_pane: &str,
    center_pane: &str,
    right_pane: &str,
) -> Result<(), SessionError> {
    let left_width = pane_width(left_pane)?;
    let center_width = pane_width(center_pane)?;
    let right_width = pane_width(right_pane)?;

    set_session_option(session_name, &restore_width_key(1), &left_width.to_string())?;
    set_session_option(
        session_name,
        &restore_width_key(2),
        &center_width.to_string(),
    )?;
    set_session_option(
        session_name,
        &restore_width_key(3),
        &right_width.to_string(),
    )
}

fn load_restore_width_targets(
    session_name: &str,
    window_width: u16,
) -> Result<(u16, u16, u16), SessionError> {
    let fallback = canonical_five_pane_column_widths(window_width, DEFAULT_CENTER_WIDTH_PCT);

    let left = parse_restore_width_option(session_name, 1)?;
    let center = parse_restore_width_option(session_name, 2)?;
    let right = parse_restore_width_option(session_name, 3)?;

    match (left, center, right) {
        (Some(left), Some(center), Some(right)) => Ok((left, center, right)),
        _ => Ok(fallback),
    }
}

fn parse_restore_width_option(
    session_name: &str,
    slot_id: u8,
) -> Result<Option<u16>, SessionError> {
    let key = restore_width_key(slot_id);
    let Some(raw) = show_session_option(session_name, &key)? else {
        return Ok(None);
    };

    raw.trim()
        .parse::<u16>()
        .map(Some)
        .map_err(|error| SessionError::TmuxCommandFailed {
            command: format!("restore-five-pane-layout -t {session_name}"),
            stderr: format!("failed parsing {key} as width: {error}"),
        })
}

fn clear_slot_suspension_metadata(session_name: &str, slot_id: u8) -> Result<(), SessionError> {
    set_session_option(session_name, &slot_suspended_key(slot_id), "0")
}

fn load_slot_restore_metadata(
    session_name: &str,
    slot_id: u8,
) -> Result<SlotRestoreMetadata, SessionError> {
    let suspended = required_session_option(session_name, &slot_suspended_key(slot_id))?;
    if suspended != "1" {
        return Err(SessionError::TmuxCommandFailed {
            command: format!("restore-five-pane-layout -t {session_name}"),
            stderr: format!("slot {slot_id} is not marked suspended"),
        });
    }

    Ok(SlotRestoreMetadata {
        pane_id: required_session_option(session_name, &slot_restore_pane_key(slot_id))?,
        worktree: required_session_option(session_name, &slot_restore_worktree_key(slot_id))?,
        cwd: required_session_option(session_name, &slot_restore_cwd_key(slot_id))?,
        mode: required_session_option(session_name, &slot_restore_mode_key(slot_id))?,
    })
}

fn verify_restored_slot_continuity(
    session_name: &str,
    slot_id: u8,
    metadata: &SlotRestoreMetadata,
) -> Result<(), SessionError> {
    let pane_id = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_pane"))?;
    let pane_slot_id = required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_id")?;
    let pane_worktree =
        required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_worktree")?;
    let pane_cwd = required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_cwd")?;
    let pane_mode = required_pane_option(session_name, slot_id, &pane_id, "@ezm_slot_mode")?;

    validate_restored_slot_continuity(
        slot_id,
        &pane_slot_id,
        &pane_worktree,
        &pane_cwd,
        &pane_mode,
        metadata,
    )
    .map_err(|reason| SessionError::TmuxCommandFailed {
        command: format!("restore-five-pane-layout -t {session_name}"),
        stderr: reason,
    })
}

fn validate_restored_slot_continuity(
    slot_id: u8,
    pane_slot_id: &str,
    pane_worktree: &str,
    pane_cwd: &str,
    pane_mode: &str,
    metadata: &SlotRestoreMetadata,
) -> Result<(), String> {
    if pane_slot_id != slot_id.to_string() {
        return Err(format!(
            "slot {slot_id} restored pane reports @ezm_slot_id={pane_slot_id}"
        ));
    }

    if pane_worktree != metadata.worktree {
        return Err(format!(
            "slot {slot_id} restored pane worktree mismatch suspended_pane={} restore={} pane={pane_worktree}",
            metadata.pane_id, metadata.worktree
        ));
    }

    if pane_cwd != metadata.cwd {
        return Err(format!(
            "slot {slot_id} restored pane cwd mismatch suspended_pane={} restore={} pane={pane_cwd}",
            metadata.pane_id, metadata.cwd
        ));
    }

    if pane_mode != metadata.mode {
        return Err(format!(
            "slot {slot_id} restored pane mode mismatch suspended_pane={} restore={} pane={pane_mode}",
            metadata.pane_id, metadata.mode
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

fn restore_width_key(slot_id: u8) -> String {
    format!("{RESTORE_WIDTH_KEY_PREFIX}{slot_id}")
}

fn is_three_pane_mode(layout_mode: &str) -> bool {
    layout_mode == LAYOUT_MODE_THREE_PANE
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

fn kill_pane_if_present(pane_id: &str) -> Result<(), SessionError> {
    let output = tmux_output(&["kill-pane", "-t", pane_id])?;
    if output.status.success() || missing_pane_diagnostic(&output) {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("kill-pane -t {pane_id}"),
        stderr: format_output_diagnostics(&output),
    })
}

fn missing_pane_diagnostic(output: &std::process::Output) -> bool {
    output.status.code() == Some(1)
        && String::from_utf8_lossy(&output.stderr)
            .to_ascii_lowercase()
            .contains("can't find pane")
}

#[cfg(test)]
mod tests {
    use super::{
        SlotRestoreMetadata, is_three_pane_mode, restore_width_key, slot_restore_cwd_key,
        slot_restore_mode_key, slot_restore_pane_key, slot_restore_worktree_key,
        slot_suspended_key, validate_restored_slot_continuity,
    };

    #[test]
    fn slot_restore_metadata_keys_are_stable() {
        assert_eq!(slot_suspended_key(4), "@ezm_slot_4_suspended");
        assert_eq!(slot_restore_pane_key(4), "@ezm_slot_4_restore_pane");
        assert_eq!(slot_restore_worktree_key(4), "@ezm_slot_4_restore_worktree");
        assert_eq!(slot_restore_cwd_key(4), "@ezm_slot_4_restore_cwd");
        assert_eq!(slot_restore_mode_key(4), "@ezm_slot_4_restore_mode");
        assert_eq!(restore_width_key(1), "@ezm_restore_width_slot_1");
        assert_eq!(restore_width_key(2), "@ezm_restore_width_slot_2");
        assert_eq!(restore_width_key(3), "@ezm_restore_width_slot_3");
    }

    #[test]
    fn three_pane_mode_detection_is_explicit() {
        assert!(is_three_pane_mode("three-pane"));
        assert!(!is_three_pane_mode("five-pane"));
        assert!(!is_three_pane_mode(""));
    }

    #[test]
    fn restored_slot_continuity_requires_original_slot_identity_and_metadata() {
        let metadata = SlotRestoreMetadata {
            pane_id: String::from("%4"),
            worktree: String::from("wt-4"),
            cwd: String::from("/repo/slot-4"),
            mode: String::from("lazygit"),
        };

        assert!(
            validate_restored_slot_continuity(4, "4", "wt-4", "/repo/slot-4", "lazygit", &metadata)
                .is_ok()
        );

        assert!(
            validate_restored_slot_continuity(4, "9", "wt-4", "/repo/slot-4", "lazygit", &metadata)
                .expect_err("restore path must preserve canonical slot id")
                .contains("@ezm_slot_id")
        );

        assert!(
            validate_restored_slot_continuity(
                4,
                "4",
                "wt-remapped",
                "/repo/slot-4",
                "lazygit",
                &metadata
            )
            .expect_err("restore path must reapply captured worktree")
            .contains("worktree mismatch")
        );

        assert!(
            validate_restored_slot_continuity(4, "4", "wt-4", "/repo/other", "lazygit", &metadata)
                .expect_err("restore path must reapply captured cwd")
                .contains("cwd mismatch")
        );

        assert!(
            validate_restored_slot_continuity(4, "4", "wt-4", "/repo/slot-4", "shell", &metadata)
                .expect_err("restore path must reapply captured mode")
                .contains("mode mismatch")
        );
    }
}
