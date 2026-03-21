use super::SessionError;
use super::command::{tmux_output, tmux_run};

const SWAP_TABLE: &str = "ezm-swap";
const FOCUS_TABLE: &str = "ezm-focus";
const SWAP_PREFIX_KEY: &str = "g";
const FOCUS_PREFIX_KEY: &str = "f";
const TOGGLE_MODE_KEY: &str = "u";
const AGENT_MODE_KEY: &str = "a";
const SHELL_MODE_KEY: &str = "S";
const NEOVIM_MODE_KEY: &str = "N";
const LAZYGIT_MODE_KEY: &str = "G";
const POPUP_KEY: &str = "P";
const THREE_PANE_PRESET_KEY: &str = "M-3";

const THREE_PANE_PRESET_RUN_SHELL: &str =
    "${EZM_BIN:-ezm} __internal preset --session #{session_name} --preset three-pane";

pub(super) fn install_runtime_keybinds() -> Result<(), SessionError> {
    for (table, key) in clear_specs() {
        unbind_key_if_present(table, &key)?;
    }

    tmux_run(&[
        "bind-key",
        "-T",
        "prefix",
        THREE_PANE_PRESET_KEY,
        "run-shell",
        THREE_PANE_PRESET_RUN_SHELL,
    ])?;
    tmux_run(&[
        "bind-key",
        "-T",
        "prefix",
        SWAP_PREFIX_KEY,
        "switch-client",
        "-T",
        SWAP_TABLE,
    ])?;
    tmux_run(&[
        "bind-key",
        "-T",
        "prefix",
        FOCUS_PREFIX_KEY,
        "switch-client",
        "-T",
        FOCUS_TABLE,
    ])?;

    for slot in 1_u8..=5 {
        let key = slot.to_string();
        let swap_command = swap_command(slot);
        tmux_run(&[
            "bind-key",
            "-T",
            SWAP_TABLE,
            &key,
            "run-shell",
            &swap_command,
            "\\;",
            "switch-client",
            "-T",
            "root",
        ])?;

        let focus_command = focus_command(slot);
        tmux_run(&[
            "bind-key",
            "-T",
            FOCUS_TABLE,
            &key,
            "run-shell",
            &focus_command,
            "\\;",
            "switch-client",
            "-T",
            "root",
        ])?;
    }

    for key in ["Escape", "q", "Any"] {
        tmux_run(&[
            "bind-key",
            "-T",
            SWAP_TABLE,
            key,
            "switch-client",
            "-T",
            "root",
        ])?;
        tmux_run(&[
            "bind-key",
            "-T",
            FOCUS_TABLE,
            key,
            "switch-client",
            "-T",
            "root",
        ])?;
    }

    let mode_bindings = [
        (TOGGLE_MODE_KEY, toggle_mode_command()),
        (AGENT_MODE_KEY, mode_command("agent")),
        (SHELL_MODE_KEY, mode_command("shell")),
        (NEOVIM_MODE_KEY, mode_command("neovim")),
        (LAZYGIT_MODE_KEY, mode_command("lazygit")),
        (POPUP_KEY, popup_command()),
    ];

    for (key, command) in mode_bindings {
        tmux_run(&["bind-key", "-T", "prefix", key, "run-shell", &command])?;
    }

    Ok(())
}

fn clear_specs() -> Vec<(&'static str, String)> {
    let mut specs = vec![
        ("prefix", THREE_PANE_PRESET_KEY.to_owned()),
        ("prefix", SWAP_PREFIX_KEY.to_owned()),
        ("prefix", FOCUS_PREFIX_KEY.to_owned()),
        ("prefix", TOGGLE_MODE_KEY.to_owned()),
        ("prefix", AGENT_MODE_KEY.to_owned()),
        ("prefix", SHELL_MODE_KEY.to_owned()),
        ("prefix", NEOVIM_MODE_KEY.to_owned()),
        ("prefix", LAZYGIT_MODE_KEY.to_owned()),
        ("prefix", POPUP_KEY.to_owned()),
        (SWAP_TABLE, String::from("Escape")),
        (SWAP_TABLE, String::from("q")),
        (SWAP_TABLE, String::from("Any")),
        (FOCUS_TABLE, String::from("Escape")),
        (FOCUS_TABLE, String::from("q")),
        (FOCUS_TABLE, String::from("Any")),
    ];

    for slot in 1_u8..=5 {
        specs.push((SWAP_TABLE, slot.to_string()));
        specs.push((FOCUS_TABLE, slot.to_string()));
    }

    specs
}

fn unbind_key_if_present(table: &str, key: &str) -> Result<(), SessionError> {
    let output = tmux_output(&["unbind-key", "-T", table, key])?;
    if output.status.success() || missing_binding_diagnostic(&output) {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("unbind-key -T {table} {key}"),
        stderr: super::command::format_output_diagnostics(&output),
    })
}

fn missing_binding_diagnostic(output: &std::process::Output) -> bool {
    if output.status.code() != Some(1) {
        return false;
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
    stderr.contains("unknown key")
        || stderr.contains("key not found")
        || stderr.contains("not bound")
        || (stderr.contains("table") && stderr.contains("doesn't exist"))
}

fn swap_command(slot_id: u8) -> String {
    format!("${{EZM_BIN:-ezm}} __internal swap --session \"#{{session_name}}\" --slot {slot_id}")
}

fn focus_command(slot_id: u8) -> String {
    format!("${{EZM_BIN:-ezm}} __internal focus --session \"#{{session_name}}\" --slot {slot_id}")
}

fn mode_command(mode: &str) -> String {
    format!(
        "${{EZM_BIN:-ezm}} __internal mode --session \"#{{session_name}}\" --slot \"#{{@ezm_slot_id}}\" --mode {mode}"
    )
}

fn toggle_mode_command() -> String {
    String::from(
        "__ezm_slot=\"#{@ezm_slot_id}\"; __ezm_mode=\"#{@ezm_slot_mode}\"; __ezm_next=\"agent\"; if [ \"${__ezm_mode}\" = \"agent\" ]; then __ezm_next=\"shell\"; fi; ${EZM_BIN:-ezm} __internal mode --session \"#{session_name}\" --slot \"${__ezm_slot}\" --mode \"${__ezm_next}\"",
    )
}

fn popup_command() -> String {
    String::from(
        "${EZM_BIN:-ezm} __internal popup --session \"#{session_name}\" --slot \"#{@ezm_slot_id}\"",
    )
}

#[cfg(test)]
mod tests {
    use std::process::{ExitStatus, Output};

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    use super::{focus_command, mode_command, popup_command, swap_command, toggle_mode_command};

    #[test]
    fn swap_command_targets_internal_runtime_entrypoint() {
        let rendered = swap_command(4);
        assert!(rendered.contains("__internal swap"));
        assert!(rendered.contains("--slot 4"));
        assert!(rendered.contains("#{session_name}"));
    }

    #[test]
    fn focus_command_targets_internal_runtime_entrypoint() {
        let rendered = focus_command(2);
        assert!(rendered.contains("__internal focus"));
        assert!(rendered.contains("--slot 2"));
        assert!(rendered.contains("#{session_name}"));
        assert!(!rendered.contains('\''));
    }

    #[test]
    fn mode_commands_target_focused_slot_metadata() {
        let rendered = mode_command("neovim");
        assert!(rendered.contains("__internal mode"));
        assert!(rendered.contains("--mode neovim"));
        assert!(rendered.contains("#{@ezm_slot_id}"));
        assert!(!rendered.contains('\''));
    }

    #[test]
    fn toggle_mode_command_switches_between_shell_and_agent() {
        let rendered = toggle_mode_command();
        assert!(rendered.contains("__internal mode"));
        assert!(rendered.contains("__ezm_next=\"agent\""));
        assert!(rendered.contains("__ezm_next=\"shell\""));
        assert!(!rendered.contains('\''));
    }

    #[test]
    fn popup_command_targets_focused_slot_metadata() {
        let rendered = popup_command();
        assert!(rendered.contains("__internal popup"));
        assert!(rendered.contains("#{@ezm_slot_id}"));
        assert!(!rendered.contains('\''));
    }

    #[cfg(unix)]
    #[test]
    fn missing_binding_diagnostic_accepts_missing_table_error() {
        let output = Output {
            status: ExitStatus::from_raw(256),
            stdout: Vec::new(),
            stderr: b"table ezm-swap doesn't exist".to_vec(),
        };

        assert!(super::missing_binding_diagnostic(&output));
    }
}
