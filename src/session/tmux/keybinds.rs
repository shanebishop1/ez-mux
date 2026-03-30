use super::SessionError;
use super::command::{tmux_output, tmux_run_batch};
use super::popup::popup_parent_cleanup_hook_install_command;
use super::remote_env::sync_runtime_env_into_tmux_server;
use crate::config::EZM_BIN_ENV;

const SWAP_TABLE: &str = "ezm-swap";
const FOCUS_TABLE: &str = "ezm-focus";
const LEGACY_SWAP_PREFIX_KEY: &str = "g";
const FOCUS_PREFIX_KEY: &str = "f";
const TOGGLE_MODE_KEY: &str = "u";
const AGENT_MODE_KEY: &str = "a";
const SHELL_MODE_KEY: &str = "S";
const NEOVIM_MODE_KEY: &str = "N";
const LAZYGIT_MODE_KEY: &str = "G";
const POPUP_KEY: &str = "P";
const DETACH_KEY: &str = "d";
const THREE_PANE_PRESET_KEY: &str = "M-3";
const PANE_NAV_LEFT_KEY: &str = "h";
const PANE_NAV_DOWN_KEY: &str = "j";
const PANE_NAV_UP_KEY: &str = "k";
const PANE_NAV_RIGHT_KEY: &str = "l";
const ACTIVE_SLOT_BORDER_STYLE_FORMAT: &str = "fg=#{?#{==:#{@ezm_slot_id},1},#5ac8e0,#{?#{==:#{@ezm_slot_id},2},#eb6f92,#{?#{==:#{@ezm_slot_id},3},#7fd77a,#{?#{==:#{@ezm_slot_id},4},#b58df2,#f2cd72}}}}";

pub(super) fn install_runtime_keybinds() -> Result<(), SessionError> {
    let ezm_bin = resolved_ezm_bin_shell_token();
    sync_runtime_env_into_tmux_server()?;
    clear_legacy_bindings()?;

    if should_clear_existing_keybinds_before_install() {
        for (table, key) in clear_specs() {
            unbind_key_if_present(table, &key)?;
        }
    }

    let mut commands = Vec::new();
    commands.extend(install_prefix_routing_bindings(&ezm_bin));
    commands.extend(install_pane_navigation_bindings());
    commands.extend(install_slot_table_bindings(&ezm_bin));
    commands.extend(install_table_exit_bindings());
    commands.extend(install_mode_bindings(&ezm_bin));
    commands.push(popup_parent_cleanup_hook_install_command());

    tmux_run_batch(&commands)
}

fn clear_legacy_bindings() -> Result<(), SessionError> {
    unbind_key_if_present("prefix", LEGACY_SWAP_PREFIX_KEY)
}

fn should_clear_existing_keybinds_before_install() -> bool {
    false
}

fn install_prefix_routing_bindings(ezm_bin: &str) -> Vec<Vec<String>> {
    let three_pane_preset_command = preset_command(ezm_bin);

    vec![
        run_shell_binding_command("prefix", THREE_PANE_PRESET_KEY, &three_pane_preset_command),
        command(&[
            "bind-key",
            "-T",
            "prefix",
            FOCUS_PREFIX_KEY,
            "switch-client",
            "-T",
            FOCUS_TABLE,
        ]),
    ]
}

fn install_pane_navigation_bindings() -> Vec<Vec<String>> {
    let mut commands = Vec::with_capacity(4);
    for (key, direction) in pane_nav_bindings() {
        commands.push(command(&[
            "bind-key",
            "-T",
            "prefix",
            key,
            "select-pane",
            direction,
            "\\;",
            "set-window-option",
            "pane-active-border-style",
            ACTIVE_SLOT_BORDER_STYLE_FORMAT,
        ]));
    }

    commands
}

fn install_slot_table_bindings(ezm_bin: &str) -> Vec<Vec<String>> {
    let mut commands = Vec::with_capacity(10);
    for slot in 1_u8..=5 {
        let key = slot.to_string();
        let swap_command = swap_command(ezm_bin, slot);
        commands.push(command(&[
            "bind-key",
            "-T",
            SWAP_TABLE,
            &key,
            "run-shell",
            "-b",
            &swap_command,
            "\\;",
            "switch-client",
            "-T",
            "root",
        ]));

        let focus_command = focus_command(ezm_bin, slot);
        commands.push(command(&[
            "bind-key",
            "-T",
            FOCUS_TABLE,
            &key,
            "run-shell",
            "-b",
            &focus_command,
            "\\;",
            "switch-client",
            "-T",
            "root",
        ]));
    }

    commands
}

fn install_table_exit_bindings() -> Vec<Vec<String>> {
    let mut commands = Vec::with_capacity(6);
    for key in ["Escape", "q", "Any"] {
        commands.push(command(&[
            "bind-key",
            "-T",
            SWAP_TABLE,
            key,
            "switch-client",
            "-T",
            "root",
        ]));
        commands.push(command(&[
            "bind-key",
            "-T",
            FOCUS_TABLE,
            key,
            "switch-client",
            "-T",
            "root",
        ]));
    }

    commands
}

fn install_mode_bindings(ezm_bin: &str) -> Vec<Vec<String>> {
    let mode_bindings = [
        (TOGGLE_MODE_KEY, toggle_mode_command(ezm_bin)),
        (AGENT_MODE_KEY, mode_command(ezm_bin, "agent")),
        (SHELL_MODE_KEY, mode_command(ezm_bin, "shell")),
        (NEOVIM_MODE_KEY, mode_command(ezm_bin, "neovim")),
        (LAZYGIT_MODE_KEY, mode_command(ezm_bin, "lazygit")),
    ];

    let mut commands = Vec::with_capacity(mode_bindings.len() + 2);
    for (key, command) in mode_bindings {
        commands.push(run_shell_binding_command("prefix", key, &command));
    }
    commands.push(popup_toggle_binding_command(ezm_bin));
    commands.push(popup_context_detach_binding_command());

    commands
}

fn popup_toggle_binding_command(ezm_bin: &str) -> Vec<String> {
    let popup_open_action = popup_toggle_open_action(ezm_bin);
    command(&[
        "bind-key",
        "-T",
        "prefix",
        POPUP_KEY,
        "if-shell",
        "-F",
        "#{@ezm_popup_origin_session}",
        "detach-client",
        &popup_open_action,
    ])
}

fn popup_context_detach_binding_command() -> Vec<String> {
    command(&[
        "bind-key",
        "-T",
        "prefix",
        DETACH_KEY,
        "if-shell",
        "-F",
        "#{@ezm_popup_origin_session}",
        popup_hard_close_action(),
        "detach-client",
    ])
}

fn popup_toggle_open_action(ezm_bin: &str) -> String {
    let popup_open_command = popup_command(ezm_bin);
    format!(
        "run-shell -b \"{}\"",
        shell_escape_double_quoted(&popup_open_command)
    )
}

fn popup_hard_close_action() -> &'static str {
    "kill-session"
}

fn run_shell_binding_command(table: &str, key: &str, shell_command: &str) -> Vec<String> {
    command(&[
        "bind-key",
        "-T",
        table,
        key,
        "run-shell",
        "-b",
        shell_command,
    ])
}

fn command(args: &[&str]) -> Vec<String> {
    args.iter().map(|value| (*value).to_owned()).collect()
}

fn clear_specs() -> Vec<(&'static str, String)> {
    let mut specs = vec![
        ("prefix", THREE_PANE_PRESET_KEY.to_owned()),
        ("prefix", FOCUS_PREFIX_KEY.to_owned()),
        ("prefix", TOGGLE_MODE_KEY.to_owned()),
        ("prefix", AGENT_MODE_KEY.to_owned()),
        ("prefix", SHELL_MODE_KEY.to_owned()),
        ("prefix", NEOVIM_MODE_KEY.to_owned()),
        ("prefix", LAZYGIT_MODE_KEY.to_owned()),
        ("prefix", POPUP_KEY.to_owned()),
        ("prefix", DETACH_KEY.to_owned()),
        ("prefix", PANE_NAV_LEFT_KEY.to_owned()),
        ("prefix", PANE_NAV_DOWN_KEY.to_owned()),
        ("prefix", PANE_NAV_UP_KEY.to_owned()),
        ("prefix", PANE_NAV_RIGHT_KEY.to_owned()),
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

fn pane_nav_bindings() -> [(&'static str, &'static str); 4] {
    [
        (PANE_NAV_LEFT_KEY, "-L"),
        (PANE_NAV_DOWN_KEY, "-D"),
        (PANE_NAV_UP_KEY, "-U"),
        (PANE_NAV_RIGHT_KEY, "-R"),
    ]
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
    format!(
        "{ezm_bin} __internal swap --session \"#{{session_name}}\" --slot {slot_id} </dev/null >/dev/null 2>&1"
    )
}

fn focus_command(ezm_bin: &str, slot_id: u8) -> String {
    format!(
        "{ezm_bin} __internal focus --session \"#{{session_name}}\" --slot {slot_id} </dev/null >/dev/null 2>&1"
    )
}

fn mode_command(ezm_bin: &str, mode: &str) -> String {
    format!(
        "{ezm_bin} __internal mode --session \"#{{session_name}}\" --slot \"#{{@ezm_slot_id}}\" --mode {mode} >/dev/null"
    )
}

fn toggle_mode_command(ezm_bin: &str) -> String {
    format!(
        "{ezm_bin} __internal mode --session \"#{{session_name}}\" --slot \"#{{@ezm_slot_id}}\" --mode \"#{{?#{{==:#{{@ezm_slot_mode}},agent}},shell,agent}}\" >/dev/null"
    )
}

fn popup_command(ezm_bin: &str) -> String {
    format!(
        "{ezm_bin} __internal popup --session \"#{{?#{{@ezm_popup_origin_session}},#{{@ezm_popup_origin_session}},#{{session_name}}}}\" --slot \"#{{?#{{@ezm_popup_origin_slot}},#{{@ezm_popup_origin_slot}},#{{@ezm_slot_id}}}}\" --client \"#{{client_tty}}\" </dev/null >/dev/null 2>&1"
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

    shell_command_token(&ezm_bin)
}

fn resolve_ezm_bin(env_ezm_bin: Option<String>, current_exe: Option<String>) -> String {
    env_ezm_bin
        .or(current_exe)
        .unwrap_or_else(|| String::from("ezm"))
}

fn shell_command_token(value: &str) -> String {
    if value.as_bytes().iter().all(|byte| {
        matches!(
            byte,
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'/' | b'.' | b'_' | b'-'
        )
    }) {
        return value.to_owned();
    }

    format!("\"{}\"", shell_escape_double_quoted(value))
}

fn shell_escape_double_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}

#[cfg(test)]
mod tests;
