use super::SessionError;
use super::command::{tmux_output, tmux_run};
use crate::config::EZM_BIN_ENV;

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

pub(super) fn install_runtime_keybinds() -> Result<(), SessionError> {
    let ezm_bin = resolved_ezm_bin_shell_token();

    for (table, key) in clear_specs() {
        unbind_key_if_present(table, &key)?;
    }

    let three_pane_preset_command = preset_command(&ezm_bin);

    tmux_run(&[
        "bind-key",
        "-T",
        "prefix",
        THREE_PANE_PRESET_KEY,
        "run-shell",
        &three_pane_preset_command,
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
        let swap_command = swap_command(&ezm_bin, slot);
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

        let focus_command = focus_command(&ezm_bin, slot);
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
        (TOGGLE_MODE_KEY, toggle_mode_command(&ezm_bin)),
        (AGENT_MODE_KEY, mode_command(&ezm_bin, "agent")),
        (SHELL_MODE_KEY, mode_command(&ezm_bin, "shell")),
        (NEOVIM_MODE_KEY, mode_command(&ezm_bin, "neovim")),
        (LAZYGIT_MODE_KEY, mode_command(&ezm_bin, "lazygit")),
        (POPUP_KEY, popup_command(&ezm_bin)),
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

fn preset_command(ezm_bin: &str) -> String {
    format!("{ezm_bin} __internal preset --session \"#{{session_name}}\" --preset three-pane")
}

fn swap_command(ezm_bin: &str, slot_id: u8) -> String {
    format!("{ezm_bin} __internal swap --session \"#{{session_name}}\" --slot {slot_id}")
}

fn focus_command(ezm_bin: &str, slot_id: u8) -> String {
    format!("{ezm_bin} __internal focus --session \"#{{session_name}}\" --slot {slot_id}")
}

fn mode_command(ezm_bin: &str, mode: &str) -> String {
    format!(
        "{ezm_bin} __internal mode --session \"#{{session_name}}\" --slot \"#{{@ezm_slot_id}}\" --mode {mode}"
    )
}

fn toggle_mode_command(ezm_bin: &str) -> String {
    format!(
        "{ezm_bin} __internal mode --session \"#{{session_name}}\" --slot \"#{{@ezm_slot_id}}\" --mode \"#{{?#{{==:#{{@ezm_slot_mode}},agent}},shell,agent}}\""
    )
}

fn popup_command(ezm_bin: &str) -> String {
    format!(
        "{ezm_bin} __internal popup --session \"#{{session_name}}\" --slot \"#{{@ezm_slot_id}}\""
    )
}

fn resolved_ezm_bin_shell_token() -> String {
    let env_ezm_bin = std::env::var(EZM_BIN_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let current_exe = std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string());
    let ezm_bin = resolve_ezm_bin(env_ezm_bin, current_exe);

    shell_single_quote(&ezm_bin)
}

fn resolve_ezm_bin(env_ezm_bin: Option<String>, current_exe: Option<String>) -> String {
    env_ezm_bin
        .or(current_exe)
        .unwrap_or_else(|| String::from("ezm"))
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use std::process::{ExitStatus, Output};

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    use super::{
        focus_command, mode_command, popup_command, resolve_ezm_bin, shell_single_quote,
        swap_command, toggle_mode_command,
    };

    #[test]
    fn swap_command_targets_internal_runtime_entrypoint() {
        let rendered = swap_command("'ezm'", 4);
        assert!(rendered.contains("__internal swap"));
        assert!(rendered.contains("--slot 4"));
        assert!(rendered.contains("#{session_name}"));
        assert!(!rendered.contains("${EZM_BIN:-ezm}"));
    }

    #[test]
    fn focus_command_targets_internal_runtime_entrypoint() {
        let rendered = focus_command("'ezm'", 2);
        assert!(rendered.contains("__internal focus"));
        assert!(rendered.contains("--slot 2"));
        assert!(rendered.contains("#{session_name}"));
        assert!(rendered.starts_with("'ezm' __internal focus"));
        assert!(!rendered.contains("'#{session_name}'"));
        assert!(!rendered.contains("${EZM_BIN:-ezm}"));
    }

    #[test]
    fn mode_commands_target_focused_slot_metadata() {
        let rendered = mode_command("'ezm'", "neovim");
        assert!(rendered.contains("__internal mode"));
        assert!(rendered.contains("--mode neovim"));
        assert!(rendered.contains("#{@ezm_slot_id}"));
        assert!(rendered.starts_with("'ezm' __internal mode"));
        assert!(!rendered.contains("'#{session_name}'"));
        assert!(!rendered.contains("'#{@ezm_slot_id}'"));
        assert!(!rendered.contains("${EZM_BIN:-ezm}"));
    }

    #[test]
    fn toggle_mode_command_switches_between_shell_and_agent() {
        let rendered = toggle_mode_command("'ezm'");
        assert!(rendered.contains("__internal mode"));
        assert!(rendered.contains("#{?#{==:#{@ezm_slot_mode},agent},shell,agent}"));
        assert!(rendered.starts_with("'ezm' __internal mode"));
        assert!(!rendered.contains("'#{session_name}'"));
        assert!(!rendered.contains("'#{@ezm_slot_id}'"));
        assert!(!rendered.contains("${EZM_BIN:-ezm}"));
        assert!(!rendered.contains("if ["));
    }

    #[test]
    fn popup_command_targets_focused_slot_metadata() {
        let rendered = popup_command("'ezm'");
        assert!(rendered.contains("__internal popup"));
        assert!(rendered.contains("#{@ezm_slot_id}"));
        assert!(rendered.starts_with("'ezm' __internal popup"));
        assert!(!rendered.contains("'#{session_name}'"));
        assert!(!rendered.contains("'#{@ezm_slot_id}'"));
        assert!(!rendered.contains("${EZM_BIN:-ezm}"));
    }

    #[test]
    fn resolve_ezm_bin_prefers_env_then_current_exe_then_literal_ezm() {
        assert_eq!(
            resolve_ezm_bin(
                Some(String::from("/tmp/ezm")),
                Some(String::from("/bin/ezm"))
            ),
            String::from("/tmp/ezm")
        );
        assert_eq!(
            resolve_ezm_bin(None, Some(String::from("/bin/ezm"))),
            String::from("/bin/ezm")
        );
        assert_eq!(resolve_ezm_bin(None, None), String::from("ezm"));
    }

    #[test]
    fn shell_single_quote_escapes_apostrophes_for_run_shell() {
        let rendered = shell_single_quote("/tmp/it's ezm");
        assert_eq!(rendered, String::from("'/tmp/it'\"'\"'s ezm'"));
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
