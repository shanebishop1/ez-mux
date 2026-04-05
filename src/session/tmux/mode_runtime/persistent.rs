use std::sync::OnceLock;

use super::super::SessionError;
use super::super::command::{format_output_diagnostics, tmux_output, tmux_output_value, tmux_run};
use super::super::options::{
    set_pane_option, set_session_option, show_pane_option, show_session_option,
    unset_session_option,
};
use super::super::{ZoomFlagSupport, tmux_diagnostics_exit_status, zoom_flag_support_for_command};
use super::pane_runtime::respawn_slot_mode;

const MODE_CACHE_SESSION_SUFFIX: &str = "__mode_cache";
const LEGACY_MODE_CACHE_WINDOW_NAME: &str = "__ezm_mode_cache";

#[derive(Debug, Clone, Copy)]
struct ZoomFlagCapabilities {
    swap_pane: ZoomFlagSupport,
}

impl Default for ZoomFlagCapabilities {
    fn default() -> Self {
        Self {
            swap_pane: ZoomFlagSupport::Unknown,
        }
    }
}

static ZOOM_FLAG_CAPABILITIES: OnceLock<ZoomFlagCapabilities> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ActivatedModePane {
    pub(super) pane_id: String,
    pub(super) pane_cwd: String,
}

pub(super) struct ModeActivationSpec<'a> {
    pub(super) current_mode: &'a str,
    pub(super) target_mode: &'a str,
    pub(super) launch_cwd: &'a str,
    pub(super) worktree: &'a str,
    pub(super) launch_command: &'a str,
}

pub(super) fn cleanup_legacy_mode_cache_sessions(session_name: &str) -> Result<(), SessionError> {
    for slot_id in 1_u8..=5 {
        kill_legacy_mode_cache_session(session_name, slot_id)?;
    }

    kill_legacy_mode_cache_window(session_name)
}

pub(super) fn activate_mode_pane(
    session_name: &str,
    slot_id: u8,
    current_pane_id: &str,
    spec: &ModeActivationSpec<'_>,
) -> Result<ActivatedModePane, SessionError> {
    let target_backing_key = backing_pane_key(slot_id, spec.target_mode);
    let mut target_pane_id = resolve_cached_backing_pane(session_name, &target_backing_key)?;

    let pane_cwd = if let Some(pane_id) = target_pane_id.as_ref() {
        pane_runtime_cwd(pane_id)?.unwrap_or_else(|| spec.launch_cwd.to_owned())
    } else {
        let pane_id = create_mode_backing_pane(session_name, spec.launch_cwd)?;
        initialize_new_mode_pane(
            &pane_id,
            slot_id,
            spec.target_mode,
            spec.launch_cwd,
            spec.worktree,
            spec.launch_command,
        )?;
        target_pane_id = Some(pane_id);
        spec.launch_cwd.to_owned()
    };

    let target_pane_id = target_pane_id.ok_or_else(|| SessionError::TmuxCommandFailed {
        command: format!("activate-mode-pane -t {session_name} --slot {slot_id}"),
        stderr: String::from("failed resolving target backing pane"),
    })?;

    set_pane_option(current_pane_id, "@ezm_slot_cwd", spec.launch_cwd)?;
    set_pane_option(current_pane_id, "@ezm_slot_mode", spec.current_mode)?;
    set_pane_option(current_pane_id, "@ezm_slot_worktree", spec.worktree)?;

    if target_pane_id != current_pane_id {
        swap_visible_with_backing(current_pane_id, &target_pane_id)?;
    }

    set_session_option(session_name, &slot_pane_key(slot_id), &target_pane_id)?;
    set_session_option(
        session_name,
        &backing_pane_key(slot_id, spec.current_mode),
        current_pane_id,
    )?;
    unset_session_option(session_name, &target_backing_key)?;
    set_pane_option(&target_pane_id, "@ezm_slot_id", &slot_id.to_string())?;
    set_pane_option(&target_pane_id, "@ezm_slot_worktree", spec.worktree)?;

    Ok(ActivatedModePane {
        pane_id: target_pane_id,
        pane_cwd,
    })
}

fn resolve_cached_backing_pane(
    session_name: &str,
    target_backing_key: &str,
) -> Result<Option<String>, SessionError> {
    let Some(pane_id) = show_session_option(session_name, target_backing_key)? else {
        return Ok(None);
    };

    if pane_exists(&pane_id)? {
        return Ok(Some(pane_id));
    }

    unset_session_option(session_name, target_backing_key)?;
    Ok(None)
}

fn pane_exists(pane_id: &str) -> Result<bool, SessionError> {
    let output = tmux_output(&["display-message", "-p", "-t", pane_id, "#{pane_id}"])?;
    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    if output.status.code() == Some(1) && stderr.contains("can't find pane") {
        return Ok(false);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("display-message -p -t {pane_id} #{{pane_id}}"),
        stderr: format_output_diagnostics(&output),
    })
}

fn pane_runtime_cwd(pane_id: &str) -> Result<Option<String>, SessionError> {
    if let Some(cwd) = show_pane_option(pane_id, "@ezm_slot_cwd")? {
        let normalized = cwd.trim();
        if !normalized.is_empty() {
            return Ok(Some(normalized.to_owned()));
        }
    }

    let output = tmux_output_value(&[
        "display-message",
        "-p",
        "-t",
        pane_id,
        "#{pane_current_path}",
    ])?;
    let normalized = output.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    Ok(Some(normalized.to_owned()))
}

fn initialize_new_mode_pane(
    pane_id: &str,
    slot_id: u8,
    mode: &str,
    cwd: &str,
    worktree: &str,
    launch_command: &str,
) -> Result<(), SessionError> {
    set_pane_option(pane_id, "@ezm_slot_id", &slot_id.to_string())?;
    set_pane_option(pane_id, "@ezm_slot_mode", mode)?;
    set_pane_option(pane_id, "@ezm_slot_cwd", cwd)?;
    set_pane_option(pane_id, "@ezm_slot_worktree", worktree)?;
    respawn_slot_mode(pane_id, cwd, launch_command)
}

fn create_mode_backing_pane(session_name: &str, cwd: &str) -> Result<String, SessionError> {
    let mode_cache_session = mode_cache_session_name(session_name);
    if !mode_cache_session_exists(&mode_cache_session)? {
        return tmux_output_value(&[
            "new-session",
            "-d",
            "-s",
            &mode_cache_session,
            "-c",
            cwd,
            "-P",
            "-F",
            "#{pane_id}",
        ])
        .map(|value| value.trim().to_owned());
    }

    tmux_output_value(&[
        "split-window",
        "-d",
        "-t",
        &mode_cache_session,
        "-c",
        cwd,
        "-P",
        "-F",
        "#{pane_id}",
    ])
    .map(|value| value.trim().to_owned())
}

fn mode_cache_session_exists(session_name: &str) -> Result<bool, SessionError> {
    let output = tmux_output(&["-q", "has-session", "-t", session_name])?;
    if output.status.success() {
        return Ok(true);
    }

    if output.status.code() == Some(1) {
        return Ok(false);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("has-session -t {session_name}"),
        stderr: format_output_diagnostics(&output),
    })
}

fn kill_legacy_mode_cache_session(session_name: &str, slot_id: u8) -> Result<(), SessionError> {
    let legacy_session = legacy_mode_cache_session_name(session_name, slot_id);
    let output = tmux_output(&["kill-session", "-t", &legacy_session])?;
    if output.status.success() {
        return Ok(());
    }

    if output.status.code() == Some(1)
        && String::from_utf8_lossy(&output.stderr)
            .to_ascii_lowercase()
            .contains("can't find session")
    {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("kill-session -t {legacy_session}"),
        stderr: format_output_diagnostics(&output),
    })
}

fn kill_legacy_mode_cache_window(session_name: &str) -> Result<(), SessionError> {
    let output = tmux_output_value(&[
        "list-windows",
        "-t",
        session_name,
        "-F",
        "#{window_id}|#{window_name}",
    ])?;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((window_id, window_name)) = line.split_once('|') else {
            continue;
        };
        if window_name.trim() != LEGACY_MODE_CACHE_WINDOW_NAME {
            continue;
        }
        tmux_run(&["kill-window", "-t", window_id.trim()])?;
    }

    Ok(())
}

fn swap_visible_with_backing(
    current_pane_id: &str,
    target_pane_id: &str,
) -> Result<(), SessionError> {
    let capabilities = zoom_flag_capabilities();
    let with_zoom_args = swap_visible_with_backing_args(current_pane_id, target_pane_id, true);
    let without_zoom_args = swap_visible_with_backing_args(current_pane_id, target_pane_id, false);

    run_with_zoom_fallback(
        "swap-pane",
        capabilities.swap_pane,
        &with_zoom_args,
        &without_zoom_args,
    )
}

fn swap_visible_with_backing_args<'a>(
    current_pane_id: &'a str,
    target_pane_id: &'a str,
    preserve_zoom: bool,
) -> Vec<&'a str> {
    let mut args = vec!["swap-pane", "-d"];
    if preserve_zoom {
        args.push("-Z");
    }
    args.extend(["-s", target_pane_id, "-t", current_pane_id]);
    args
}

fn zoom_flag_capabilities() -> ZoomFlagCapabilities {
    *ZOOM_FLAG_CAPABILITIES.get_or_init(|| match tmux_output_value(&["list-commands"]) {
        Ok(command_listing) => ZoomFlagCapabilities {
            swap_pane: zoom_flag_support_for_command(&command_listing, "swap-pane"),
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

fn should_retry_without_zoom(command_name: &str, command: &str, stderr: &str) -> bool {
    command_starts_with_zoom_flag(command_name, command)
        && tmux_diagnostics_exit_status(stderr) == Some(1)
}

fn command_starts_with_zoom_flag(command_name: &str, command: &str) -> bool {
    let mut parts = command.split_ascii_whitespace();
    matches!(parts.next(), Some(name) if name == command_name)
        && matches!(parts.next(), Some(flag) if flag == "-d")
        && matches!(parts.next(), Some(flag) if flag == "-Z")
}

fn slot_pane_key(slot_id: u8) -> String {
    format!("@ezm_slot_{slot_id}_pane")
}

fn backing_pane_key(slot_id: u8, mode: &str) -> String {
    format!("@ezm_slot_{slot_id}_backing_{mode}_pane")
}

fn mode_cache_session_name(session_name: &str) -> String {
    format!("{session_name}{MODE_CACHE_SESSION_SUFFIX}")
}

fn legacy_mode_cache_session_name(session_name: &str, slot_id: u8) -> String {
    format!("{session_name}__mode_slot_{slot_id}")
}

#[cfg(test)]
mod tests {
    use super::{
        LEGACY_MODE_CACHE_WINDOW_NAME, MODE_CACHE_SESSION_SUFFIX, backing_pane_key,
        legacy_mode_cache_session_name, mode_cache_session_name, should_retry_without_zoom,
        swap_visible_with_backing_args,
    };

    #[test]
    fn mode_cache_key_names_are_stable() {
        assert_eq!(
            backing_pane_key(3, "agent"),
            "@ezm_slot_3_backing_agent_pane"
        );
        assert_eq!(
            mode_cache_session_name("ezm-session-abc"),
            "ezm-session-abc__mode_cache"
        );
        assert_eq!(
            legacy_mode_cache_session_name("ezm-session-abc", 4),
            "ezm-session-abc__mode_slot_4"
        );
        assert_eq!(MODE_CACHE_SESSION_SUFFIX, "__mode_cache");
        assert_eq!(LEGACY_MODE_CACHE_WINDOW_NAME, "__ezm_mode_cache");
    }

    #[test]
    fn visible_backing_swap_preserves_zoom_without_changing_active_target() {
        assert_eq!(
            swap_visible_with_backing_args("%1", "%9", true),
            vec!["swap-pane", "-d", "-Z", "-s", "%9", "-t", "%1"]
        );
        assert_eq!(
            swap_visible_with_backing_args("%1", "%9", false),
            vec!["swap-pane", "-d", "-s", "%9", "-t", "%1"]
        );
    }

    #[test]
    fn mode_backing_swap_retries_only_for_zoom_attempts_with_status_one() {
        assert!(should_retry_without_zoom(
            "swap-pane",
            "swap-pane -d -Z -s %9 -t %1",
            "status=1; stdout=\"\"; stderr=\"unknown option -- Z\""
        ));
        assert!(!should_retry_without_zoom(
            "swap-pane",
            "swap-pane -d -s %9 -t %1",
            "status=1; stdout=\"\"; stderr=\"pane not found\""
        ));
        assert!(!should_retry_without_zoom(
            "swap-pane",
            "swap-pane -d -Z -s %9 -t %1",
            "status=127; stdout=\"\"; stderr=\"pane not found\""
        ));
    }
}
