use super::super::hooks::{
    hooks_contain_popup_parent_cleanup, popup_cleanup_hook_names, popup_parent_cleanup_hook_command,
};

#[test]
fn popup_cleanup_hook_names_match_popup_cleanup_entries_only() {
    let hooks = concat!(
        "session-closed[0] run-shell -b \"tmux kill-session -t \\\"#{hook_session_name}__popup_slot_1\\\"\"\n",
        "session-closed[1] display-message keep-me\n",
        "session-closed[2] run-shell -b \"tmux kill-session -t \\\"#{hook_session_name}__popup_slot_5\\\"\"\n"
    );

    assert_eq!(
        popup_cleanup_hook_names(hooks),
        vec![
            String::from("session-closed[0]"),
            String::from("session-closed[2]"),
        ]
    );
}

#[test]
fn popup_parent_cleanup_hook_command_invokes_shell_cleanup_route() {
    let rendered = popup_parent_cleanup_hook_command();
    assert!(rendered.starts_with("run-shell -b \""));
    assert!(rendered.contains("tmux has-session -t \\\"#{hook_session_name}__popup_slot_1\\\""));
    assert!(rendered.contains("tmux kill-session -t \\\"#{hook_session_name}__popup_slot_5\\\""));
    assert!(rendered.contains("EZM_POPUP_PARENT_CLEANUP_V2"));
    assert!(rendered.ends_with('"'));
    assert!(!rendered.contains("'\"'\"'"));
}

#[test]
fn popup_parent_cleanup_hook_detection_uses_script_marker() {
    let hooks = "session-closed[0] run-shell -b \"tmux has-session -t \\\"#{hook_session_name}__popup_slot_1\\\"; : # EZM_POPUP_PARENT_CLEANUP_V2\"";
    assert!(hooks_contain_popup_parent_cleanup(hooks));
}

#[test]
fn popup_cleanup_hook_names_ignore_non_popup_cleanup_hooks() {
    let hooks = concat!(
        "session-closed\n",
        "session-closed[0] display-message keep-me\n",
        "pane-died[0] run-shell -b \"echo other\"\n"
    );

    assert!(popup_cleanup_hook_names(hooks).is_empty());
}

#[test]
fn popup_cleanup_hook_names_skip_current_parent_cleanup_hook_entries() {
    let hooks = "session-closed[0] run-shell -b \"tmux has-session -t \\\"#{hook_session_name}__popup_slot_1\\\"; : # EZM_POPUP_PARENT_CLEANUP_V2\"";
    assert!(popup_cleanup_hook_names(hooks).is_empty());
}

#[test]
fn popup_cleanup_hook_names_include_legacy_internal_cleanup_entries() {
    let hooks = "session-closed[2] run-shell -b \"/tmp/ezm __internal popup-parent-closed --session \\\"#{hook_session_name}\\\"\"";
    assert_eq!(
        popup_cleanup_hook_names(hooks),
        vec![String::from("session-closed[2]")]
    );
}
