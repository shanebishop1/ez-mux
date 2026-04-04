use super::{
    RegistryWriteStrategy, binary_hint_looks_like_single_executable,
    bootstrap_registry_write_strategy, normalize_shell_binary_hint, parse_bootstrap_anchor,
    should_apply_runtime_styles_during_bootstrap, should_validate_registry_after_bootstrap,
    startup_mode_for_slot, startup_mode_schedule_command,
};

#[test]
fn startup_mode_defaults_visible_slots_to_agent_when_worktree_candidates_are_underfilled() {
    let modes = (1_u8..=5)
        .map(|slot_id| startup_mode_for_slot(slot_id, 1))
        .collect::<Vec<_>>();

    assert_eq!(modes, vec!["agent", "agent", "agent", "agent", "agent"]);
}

#[test]
fn startup_mode_schedule_command_runs_internal_mode_in_background() {
    let rendered = startup_mode_schedule_command("'ezm'", "ezm-demo", 3);
    assert!(rendered.contains("sleep 0.05;"));
    assert!(rendered.contains("__internal mode"));
    assert!(rendered.contains("--session 'ezm-demo'"));
    assert!(rendered.contains("--slot 3"));
    assert!(rendered.contains("--mode agent"));
    assert!(rendered.contains("EZM_STARTUP_SLOT_MODE=1"));
    assert!(rendered.contains("</dev/null >/dev/null 2>&1"));
}

#[test]
fn bootstrap_registry_uses_set_only_writes() {
    assert_eq!(
        bootstrap_registry_write_strategy(),
        RegistryWriteStrategy::SetOnly
    );
}

#[test]
fn bootstrap_skips_full_registry_validation_roundtrip() {
    assert!(!should_validate_registry_after_bootstrap());
}

#[test]
fn bootstrap_applies_runtime_style_on_first_attach() {
    assert!(should_apply_runtime_styles_during_bootstrap());
}

#[test]
fn parse_bootstrap_anchor_reads_window_pane_and_width() {
    let parsed = parse_bootstrap_anchor("@9|%42|192\n").expect("parse bootstrap anchor");
    assert_eq!(parsed.window_target, String::from("@9"));
    assert_eq!(parsed.pane_id, String::from("%42"));
    assert_eq!(parsed.window_width, 192);
}

#[test]
fn normalize_shell_binary_hint_strips_quoted_boundary_variants() {
    assert_eq!(
        normalize_shell_binary_hint("'/tmp/ezm'"),
        Some(String::from("/tmp/ezm"))
    );
    assert_eq!(
        normalize_shell_binary_hint("\"/tmp/ezm\""),
        Some(String::from("/tmp/ezm"))
    );
    assert_eq!(
        normalize_shell_binary_hint("'/tmp/ezm"),
        Some(String::from("/tmp/ezm"))
    );
    assert_eq!(
        normalize_shell_binary_hint("/tmp/ezm'"),
        Some(String::from("/tmp/ezm"))
    );
    assert_eq!(
        normalize_shell_binary_hint("\\\"/tmp/ezm\\\""),
        Some(String::from("/tmp/ezm"))
    );
    assert!(binary_hint_looks_like_single_executable("/tmp/ezm"));
    assert!(!binary_hint_looks_like_single_executable(
        "/tmp/ezm __internal focus"
    ));
}
