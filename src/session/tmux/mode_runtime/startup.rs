use super::super::SessionError;

pub(super) fn use_startup_fast_path(prefer_assigned_worktree_cwd: bool) -> bool {
    prefer_assigned_worktree_cwd
}

pub(super) fn startup_mode_signal_present() -> bool {
    startup_mode_signal_enabled(std::env::var("EZM_STARTUP_SLOT_MODE").ok().as_deref())
}

pub(super) fn startup_mode_signal_enabled(value: Option<&str>) -> bool {
    value
        .map(str::trim)
        .is_some_and(|value| matches!(value, "1" | "true" | "yes" | "on"))
}

pub(super) fn resolve_mode_switch_cwd<F>(
    prefer_assigned_worktree_cwd: bool,
    assigned_worktree: &str,
    captured_cwd: F,
) -> Result<String, SessionError>
where
    F: FnOnce() -> Result<String, SessionError>,
{
    if prefer_assigned_worktree_cwd {
        return Ok(assigned_worktree.to_owned());
    }

    captured_cwd()
}
