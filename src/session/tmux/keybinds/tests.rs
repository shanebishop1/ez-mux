use std::process::Command;
use std::process::{ExitStatus, Output};

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use super::{
    ACTIVE_SLOT_BORDER_STYLE_FORMAT, binding_contains_legacy_internal_slot_command, focus_command,
    guarded_run_shell_binding_command, guarded_table_run_shell_binding_command, mode_command,
    pane_nav_bindings, popup_command, popup_hard_close_action, popup_toggle_binding_command,
    popup_toggle_open_action, preset_command, resolve_ezm_bin, shell_command_token, swap_command,
    tmux_command_argument, toggle_mode_command,
};

#[test]
fn swap_command_targets_internal_runtime_entrypoint() {
    let rendered = swap_command("'ezm'", 4);
    assert!(rendered.contains("__internal swap"));
    assert!(rendered.contains("--slot 4"));
    assert!(rendered.contains("#{q:session_name}"));
    assert!(rendered.contains(">/dev/null 2>&1"));
    assert!(!rendered.contains("${EZM_BIN:-ezm}"));
}

#[test]
fn focus_command_targets_internal_runtime_entrypoint() {
    let rendered = focus_command("'ezm'", 2);
    assert!(rendered.contains("__internal focus"));
    assert!(rendered.contains("--slot 2"));
    assert!(rendered.contains("#{q:session_name}"));
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
    assert!(rendered.contains("#{q:@ezm_slot_id}"));
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
    assert!(rendered.contains("#{q:#{?#{==:#{@ezm_slot_mode},agent},shell,agent}}"));
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
        rendered.contains(
            "#{q:#{?#{@ezm_popup_origin_slot},#{@ezm_popup_origin_slot},#{@ezm_slot_id}}}"
        )
    );
    assert!(rendered.contains("</dev/null >/dev/null 2>&1"));
    assert!(rendered.starts_with("'ezm' __internal popup"));
    assert!(rendered.contains(
        "#{q:#{?#{@ezm_popup_origin_session},#{@ezm_popup_origin_session},#{session_name}}}"
    ));
    assert!(!rendered.contains("${EZM_BIN:-ezm}"));
}

#[test]
fn popup_command_targets_client_tty_for_keybind_context() {
    let rendered = popup_command("'ezm'");
    assert!(rendered.contains("--client #{q:client_tty}"));
}

#[test]
fn popup_command_avoids_client_interpolation_and_closes_stdio() {
    let rendered = popup_command("'ezm'");
    assert!(rendered.contains("--client #{q:client_tty}"));
    assert!(rendered.contains("</dev/null >/dev/null 2>&1"));
}

#[test]
fn popup_toggle_open_action_quotes_internal_popup_command_as_single_argument() {
    let rendered = popup_toggle_open_action("'ezm'");
    assert!(rendered.starts_with("run-shell -b \""));
    assert!(rendered.contains("__internal popup"));
    assert!(rendered.contains("--session #{q:#{?#{@ezm_popup_origin_session}"));
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
fn guarded_prefix_binding_skips_non_ezm_sessions() {
    let binding = guarded_run_shell_binding_command(
        "prefix",
        "a",
        "#{@ezm_slot_id}",
        "/tmp/ezm __internal mode --session #{session_name}",
    );

    assert_eq!(binding[0], "bind-key");
    assert!(binding.iter().any(|part| part == "if-shell"));
    assert!(binding.iter().any(|part| part == "#{@ezm_slot_id}"));
    assert!(binding.iter().any(|part| part.contains("run-shell -b")));
}

#[test]
fn popup_binding_preserves_helper_detach_and_guards_ordinary_open() {
    let binding = popup_toggle_binding_command("ezm");
    assert!(
        binding
            .iter()
            .any(|part| part == "#{@ezm_popup_origin_session}")
    );
    assert!(
        binding
            .iter()
            .any(|part| part.contains("if-shell -F \"#{@ezm_slot_1_pane}\""))
    );
}

#[test]
fn popup_binding_quotes_nested_run_shell_for_tmux_36_parser() {
    let binding = popup_toggle_binding_command("ezm");
    let nested = binding
        .last()
        .expect("popup binding should include ordinary-session action");

    assert!(
        nested.contains(
            "if-shell -F \"#{@ezm_slot_1_pane}\" \"run-shell -b \\\"ezm __internal popup"
        )
    );
    assert!(nested.ends_with("2>&1\\\"\""));
}

#[test]
fn tmux_command_argument_escapes_the_nested_parser_boundary() {
    assert_eq!(
        tmux_command_argument("run-shell -b \"tmux set-option -g @hit yes\""),
        "\"run-shell -b \\\"tmux set-option -g @hit yes\\\"\""
    );
}

#[test]
fn popup_binding_nested_action_parses_on_tmux_36_without_arity_error() {
    let temp_dir = tempfile::tempdir().expect("isolated tmux directory");
    let socket = temp_dir.path().join("s");
    let socket_arg = socket.to_string_lossy().into_owned();

    run_isolated_tmux(&socket_arg, &["new-session", "-d", "-s", "ezm-parser"]);
    run_isolated_tmux(&socket_arg, &["set-option", "-g", "@ezm_slot_1_pane", "%1"]);

    let binding = popup_toggle_binding_command("/bin/true");
    let nested_action = binding.last().expect("popup binding action");
    let output = Command::new("tmux")
        .args([
            "-S",
            &socket_arg,
            "-f",
            "/dev/null",
            "if-shell",
            "-F",
            "1",
            nested_action,
        ])
        .output()
        .expect("tmux parser command");

    assert!(
        output.status.success(),
        "tmux nested popup action failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("too many arguments"));
    run_isolated_tmux(&socket_arg, &["kill-server"]);
}

fn run_isolated_tmux(socket: &str, args: &[&str]) {
    let output = Command::new("tmux")
        .args(["-S", socket, "-f", "/dev/null"])
        .args(args)
        .output()
        .expect("tmux command");
    assert!(
        output.status.success(),
        "tmux {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn shell_command_token_escapes_shell_expansion_boundaries() {
    let rendered = shell_command_token("/tmp/ezm; $(touch pwned) `id` \"quoted\" \\\\ path");
    assert!(rendered.starts_with('"'));
    assert!(rendered.ends_with('"'));
    assert!(rendered.contains("\\$"));
    assert!(rendered.contains("\\`"));
    assert!(rendered.contains("\\\"quoted\\\""));
    assert!(rendered.contains("\\\\ path"));
}

#[test]
fn guarded_table_binding_returns_to_root_after_attempt() {
    let binding = guarded_table_run_shell_binding_command(
        "ezm-focus",
        "1",
        "#{@ezm_slot_1_pane}",
        "/tmp/ezm __internal focus --session #{session_name} --slot 1",
    );

    assert!(binding.iter().any(|part| part == "if-shell"));
    assert!(binding.iter().any(|part| part == "switch-client"));
    assert!(binding.iter().any(|part| part == "root"));
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

#[test]
fn legacy_prefix_slot_binding_detection_matches_old_internal_routes_only() {
    assert!(binding_contains_legacy_internal_slot_command(
        "bind-key -T prefix 1 run-shell -b \"/tmp/ezm __internal focus --session #{session_name} --slot 1\""
    ));
    assert!(binding_contains_legacy_internal_slot_command(
        "bind-key -T prefix 3 run-shell -b \"/tmp/ezm __internal swap --session #{session_name} --slot 3\""
    ));
    assert!(!binding_contains_legacy_internal_slot_command(
        "bind-key -T prefix 1 select-window -t :=1"
    ));
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
