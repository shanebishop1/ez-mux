use super::super::SessionError;
use super::super::command::{format_output_diagnostics, tmux_output, tmux_run};
use super::remote_launch::escape_single_quotes;
use crate::session::TeardownHook;

pub(super) fn run_teardown_hooks(
    pane_id: &str,
    hooks: &[TeardownHook],
) -> Result<(), SessionError> {
    for hook in hooks {
        match hook {
            TeardownHook::SendCtrlC => {
                tmux_run(&["send-keys", "-t", pane_id, "C-c"])?;
            }
        }
    }

    Ok(())
}

pub(super) fn respawn_slot_mode(
    pane_id: &str,
    cwd: &str,
    launch_command: &str,
) -> Result<(), SessionError> {
    let shell_command = format!("sh -lc '{}'", escape_single_quotes(launch_command));
    let args = [
        "respawn-pane",
        "-k",
        "-t",
        pane_id,
        "-c",
        cwd,
        &shell_command,
    ];
    let output = tmux_output(&args)?;
    if output.status.success() {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("respawn-pane -k -t {pane_id} -c {cwd} <mode-launch-command>"),
        stderr: format_output_diagnostics(&output),
    })
}
