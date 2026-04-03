use std::process::{ExitStatus, Output};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use super::{
    ACTIVE_SLOT_BORDER_STYLE_FORMAT, focus_command, mode_command, pane_nav_bindings, popup_command,
    popup_hard_close_action, popup_toggle_open_action, preset_command, resolve_ezm_bin,
    shell_command_token, should_clear_existing_keybinds_before_install, swap_command,
    toggle_mode_command,
};

#[test]
fn swap_command_targets_internal_runtime_entrypoint() {
    let rendered = swap_command("'ezm'", 4);
    assert!(rendered.contains("__internal swap"));
    assert!(rendered.contains("--slot 4"));
    assert!(rendered.contains("#{session_name}"));
    assert!(rendered.contains(">/dev/null 2>&1"));
    assert!(!rendered.contains("${EZM_BIN:-ezm}"));
}

#[test]
fn focus_command_targets_internal_runtime_entrypoint() {
    let rendered = focus_command("'ezm'", 2);
    assert!(rendered.contains("__internal focus"));
    assert!(rendered.contains("--slot 2"));
    assert!(rendered.contains("#{session_name}"));
    assert!(rendered.contains(">/dev/null 2>&1"));
    assert!(rendered.starts_with("'ezm' __internal focus"));
    assert!(!rendered.contains("'#{session_name}'"));
    assert!(!rendered.contains("${EZM_BIN:-ezm}"));
}

#[test]
fn focus_and_swap_commands_close_stdin_and_suppress_output() {
    let focus_rendered = focus_command("'ezm'", 1);
    let swap_rendered = swap_command("'ezm'", 1);

    assert!(focus_rendered.contains("</dev/null >/dev/null 2>&1"));
    assert!(swap_rendered.contains("</dev/null >/dev/null 2>&1"));
}

#[test]
fn mode_commands_target_focused_slot_metadata() {
    let rendered = mode_command("'ezm'", "neovim");
    assert!(rendered.contains("__internal mode"));
    assert!(rendered.contains("--mode neovim"));
    assert!(rendered.contains("#{@ezm_slot_id}"));
    assert!(rendered.contains("</dev/null >/dev/null 2>&1"));
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
    assert!(rendered.contains("</dev/null >/dev/null 2>&1"));
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
    assert!(
        rendered
            .contains("#{?#{@ezm_popup_origin_slot},#{@ezm_popup_origin_slot},#{@ezm_slot_id}}")
    );
    assert!(rendered.contains("</dev/null >/dev/null 2>&1"));
    assert!(rendered.starts_with("'ezm' __internal popup"));
    assert!(
        rendered.contains(
            "#{?#{@ezm_popup_origin_session},#{@ezm_popup_origin_session},#{session_name}}"
        )
    );
    assert!(!rendered.contains("${EZM_BIN:-ezm}"));
}

#[test]
fn popup_command_targets_client_tty_for_keybind_context() {
    let rendered = popup_command("'ezm'");
    assert!(rendered.contains("--client \"#{client_tty}\""));
}

#[test]
fn popup_command_avoids_client_interpolation_and_closes_stdio() {
    let rendered = popup_command("'ezm'");
    assert!(rendered.contains("--client \"#{client_tty}\""));
    assert!(rendered.contains("</dev/null >/dev/null 2>&1"));
}

#[test]
fn popup_toggle_open_action_quotes_internal_popup_command_as_single_argument() {
    let rendered = popup_toggle_open_action("'ezm'");
    assert!(rendered.starts_with("run-shell -b \""));
    assert!(rendered.contains("__internal popup"));
    assert!(rendered.contains("--session \\\"#{?#{@ezm_popup_origin_session}"));
    assert!(rendered.ends_with("2>&1\""));
    assert!(!rendered.contains("'\"'\"'"));
}

#[test]
fn popup_hard_close_action_targets_current_popup_session() {
    assert_eq!(popup_hard_close_action(), "kill-session");
}

#[test]
fn preset_command_runs_quietly_in_background() {
    let rendered = preset_command("'ezm'");
    assert!(rendered.contains("__internal preset"));
    assert!(rendered.contains("--preset three-pane"));
    assert!(rendered.contains("</dev/null >/dev/null 2>&1"));
}

#[test]
fn startup_keybind_install_skips_unbind_clear_phase() {
    assert!(!should_clear_existing_keybinds_before_install());
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
fn resolve_ezm_bin_strips_wrapping_quotes_from_env_hint() {
    assert_eq!(
        resolve_ezm_bin(Some(String::from("'/tmp/ezm'")), None),
        String::from("/tmp/ezm")
    );
    assert_eq!(
        resolve_ezm_bin(Some(String::from("\"/tmp/ezm\"")), None),
        String::from("/tmp/ezm")
    );
    assert_eq!(
        resolve_ezm_bin(Some(String::from("'\"/tmp/ezm\"'")), None),
        String::from("/tmp/ezm")
    );
}

#[test]
fn resolve_ezm_bin_strips_unbalanced_boundary_quotes_from_env_hint() {
    assert_eq!(
        resolve_ezm_bin(Some(String::from("'/tmp/ezm")), None),
        String::from("/tmp/ezm")
    );
    assert_eq!(
        resolve_ezm_bin(Some(String::from("/tmp/ezm'")), None),
        String::from("/tmp/ezm")
    );
}

#[test]
fn resolve_ezm_bin_strips_backslash_escaped_boundary_quotes_from_env_hint() {
    assert_eq!(
        resolve_ezm_bin(Some(String::from("\\\"/tmp/ezm\\\"")), None),
        String::from("/tmp/ezm")
    );
    assert_eq!(
        resolve_ezm_bin(Some(String::from("\\'/tmp/ezm\\'")), None),
        String::from("/tmp/ezm")
    );
}

#[test]
fn resolve_ezm_bin_ignores_multi_token_env_hint_and_falls_back() {
    assert_eq!(
        resolve_ezm_bin(
            Some(String::from("/tmp/ezm __internal focus")),
            Some(String::from("/bin/ezm"))
        ),
        String::from("/bin/ezm")
    );
}

#[test]
fn shell_command_token_leaves_shell_safe_paths_unquoted() {
    let rendered = shell_command_token("/tmp/ezm-bin");
    assert_eq!(rendered, String::from("/tmp/ezm-bin"));
}

#[test]
fn shell_command_token_double_quotes_paths_with_spaces() {
    let rendered = shell_command_token("/tmp/ezm bin");
    assert_eq!(rendered, String::from("\"/tmp/ezm bin\""));
}

#[test]
fn pane_nav_bindings_cover_hjkl_directions() {
    assert_eq!(
        pane_nav_bindings(),
        [("h", "-L"), ("j", "-D"), ("k", "-U"), ("l", "-R")]
    );
}

#[test]
fn active_slot_border_style_format_maps_all_five_slot_colors() {
    assert!(ACTIVE_SLOT_BORDER_STYLE_FORMAT.contains("#{@ezm_slot_id}"));
    assert!(ACTIVE_SLOT_BORDER_STYLE_FORMAT.contains("#5ac8e0"));
    assert!(ACTIVE_SLOT_BORDER_STYLE_FORMAT.contains("#eb6f92"));
    assert!(ACTIVE_SLOT_BORDER_STYLE_FORMAT.contains("#7fd77a"));
    assert!(ACTIVE_SLOT_BORDER_STYLE_FORMAT.contains("#b58df2"));
    assert!(ACTIVE_SLOT_BORDER_STYLE_FORMAT.contains("#f2cd72"));
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
