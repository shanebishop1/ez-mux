use std::sync::OnceLock;

use super::command::{tmux_output_value, tmux_run};
use super::tmux_diagnostics_exit_status;
use super::{SessionError, ZoomFlagSupport};
use crate::session::zoom_flag_support_for_command;

#[derive(Debug, Clone, Copy)]
struct ZoomFlagCapabilities {
    swap_pane: ZoomFlagSupport,
    select_pane: ZoomFlagSupport,
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

pub(super) fn zoom_flag_support(command_name: &str) -> ZoomFlagSupport {
    let capabilities =
        *ZOOM_FLAG_CAPABILITIES.get_or_init(|| match tmux_output_value(&["list-commands"]) {
            Ok(command_listing) => ZoomFlagCapabilities {
                swap_pane: zoom_flag_support_for_command(&command_listing, "swap-pane"),
                select_pane: zoom_flag_support_for_command(&command_listing, "select-pane"),
            },
            Err(_) => ZoomFlagCapabilities::default(),
        });

    match command_name {
        "swap-pane" => capabilities.swap_pane,
        "select-pane" => capabilities.select_pane,
        _ => ZoomFlagSupport::Unknown,
    }
}

pub(super) fn run_with_zoom_fallback(
    command_name: &str,
    zoom_support: ZoomFlagSupport,
    with_zoom_args: &[&str],
    without_zoom_args: &[&str],
    zoom_attempt_prefix: &[&str],
) -> Result<(), SessionError> {
    if zoom_support == ZoomFlagSupport::Unsupported {
        return tmux_run(without_zoom_args);
    }

    match tmux_run(with_zoom_args) {
        Ok(()) => Ok(()),
        Err(SessionError::TmuxCommandFailed { command, stderr })
            if should_retry_without_zoom(command_name, &command, zoom_attempt_prefix, &stderr) =>
        {
            tmux_run(without_zoom_args)
        }
        Err(error) => Err(error),
    }
}

pub(super) fn should_retry_without_zoom(
    command_name: &str,
    command: &str,
    zoom_attempt_prefix: &[&str],
    stderr: &str,
) -> bool {
    command_starts_with_zoom_prefix(command_name, command, zoom_attempt_prefix)
        && tmux_diagnostics_exit_status(stderr) == Some(1)
}

fn command_starts_with_zoom_prefix(
    command_name: &str,
    command: &str,
    zoom_attempt_prefix: &[&str],
) -> bool {
    let mut parts = command.split_ascii_whitespace();
    if parts.next() != Some(command_name) {
        return false;
    }

    zoom_attempt_prefix
        .iter()
        .all(|expected| parts.next() == Some(*expected))
}

#[cfg(test)]
mod tests {
    use super::should_retry_without_zoom;

    #[test]
    fn retries_only_for_the_expected_zoom_attempt_and_status_one() {
        assert!(should_retry_without_zoom(
            "swap-pane",
            "swap-pane -Z -s %1 -t %2",
            &["-Z"],
            "status=1; stdout=\"\"; stderr=\"unknown option -- Z\""
        ));
        assert!(should_retry_without_zoom(
            "swap-pane",
            "swap-pane -d -Z -s %1 -t %2",
            &["-d", "-Z"],
            "status=1; stdout=\"\"; stderr=\"unknown option -- Z\""
        ));
        assert!(!should_retry_without_zoom(
            "swap-pane",
            "swap-pane -s %1 -t %2",
            &["-Z"],
            "status=1; stdout=\"\"; stderr=\"pane not found\""
        ));
        assert!(!should_retry_without_zoom(
            "swap-pane",
            "swap-pane -Z -s %1 -t %2",
            &["-Z"],
            "status=127; stdout=\"\"; stderr=\"pane not found\""
        ));
    }
}
