use super::{
    launch_agent_attach_command, launch_command_for_mode,
    launch_command_with_remote_dir_from_mapping, resolve_mode_switch_cwd, use_startup_fast_path,
};
use crate::session::{SharedServerAttachConfig, SlotMode};

#[test]
fn remote_prefix_injects_ezm_remote_dir_export() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let command = launch_command_with_remote_dir_from_mapping(
        "exec \"${SHELL:-/bin/sh}\" -l",
        &nested.display().to_string(),
        Some("/srv/remotes"),
        Some("alice"),
    )
    .expect("command should resolve");

    assert!(command.contains("EZM_REMOTE_DIR='/srv/remotes/alpha/worktrees/feature-x'"));
    assert!(command.contains("OPERATOR='alice'"));
}

#[test]
fn missing_remote_mapping_keeps_original_launch_command() {
    let command = launch_command_with_remote_dir_from_mapping(
        "exec \"${SHELL:-/bin/sh}\" -l",
        "/tmp/local-only",
        None,
        None,
    )
    .expect("command should resolve");

    assert_eq!(command, "exec \"${SHELL:-/bin/sh}\" -l");
}

#[test]
fn remote_prefix_without_operator_fails_fast() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");

    let error = launch_command_with_remote_dir_from_mapping(
        "exec \"${SHELL:-/bin/sh}\" -l",
        &repo_root.display().to_string(),
        Some("/srv/remotes"),
        None,
    )
    .expect_err("missing operator should fail");

    assert!(
        error
            .to_string()
            .contains("remote-prefix routing requires OPERATOR")
    );
}

#[test]
fn agent_mode_uses_shared_server_attach_url_and_mapped_dir() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let command = launch_agent_attach_command(
        &nested.display().to_string(),
        Some("/srv/remotes"),
        &SharedServerAttachConfig {
            url: String::from("http://127.0.0.1:4096"),
            password: None,
        },
    )
    .expect("agent command should resolve");

    assert!(command.contains("opencode attach 'http://127.0.0.1:4096'"));
    assert!(command.contains("--dir '/srv/remotes/alpha/worktrees/feature-x'"));
}

#[test]
fn agent_mode_password_is_included_when_configured() {
    let command = launch_agent_attach_command(
        "/tmp/local-only",
        None,
        &SharedServerAttachConfig {
            url: String::from("http://127.0.0.1:4096"),
            password: Some(String::from("secret-token")),
        },
    )
    .expect("agent command should resolve");

    assert!(command.contains("--password 'secret-token'"));
}

#[test]
fn agent_mode_requires_non_empty_attach_url_when_shared_server_config_is_used() {
    let error = launch_agent_attach_command(
        "/tmp/local-only",
        None,
        &SharedServerAttachConfig {
            url: String::new(),
            password: None,
        },
    )
    .expect_err("empty shared server config should fail");

    assert!(
        error
            .to_string()
            .contains("agent mode requires shared-server attach configuration")
    );
}

#[test]
fn agent_mode_without_shared_server_uses_local_launch_contract() {
    let command = launch_command_for_mode(
        SlotMode::Agent,
        "exec opencode || exec \"${SHELL:-/bin/sh}\" -l",
        "/tmp/local-only",
        None,
        None,
        None,
    )
    .expect("agent local launch should resolve");

    assert_eq!(command, "exec opencode || exec \"${SHELL:-/bin/sh}\" -l");
}

#[test]
fn agent_mode_does_not_require_operator_for_remote_prefix_mapping() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");

    let command = launch_command_for_mode(
        SlotMode::Agent,
        "placeholder",
        &repo_root.display().to_string(),
        Some("/srv/remotes"),
        None,
        Some(&SharedServerAttachConfig {
            url: String::from("http://127.0.0.1:4096"),
            password: None,
        }),
    )
    .expect("agent mode should not require operator");

    assert!(command.contains("opencode attach 'http://127.0.0.1:4096'"));
}

#[test]
fn startup_prefers_assigned_worktree_over_inherited_project_cwd() {
    let slot_worktrees = [
        "/Users/dev/projects/ez-mux/ez-mux-1",
        "/Users/dev/projects/ez-mux/ez-mux-2",
        "/Users/dev/projects/ez-mux/ez-mux-3",
    ];

    let resolved = slot_worktrees
        .iter()
        .map(|worktree| {
            resolve_mode_switch_cwd(true, worktree, || {
                Ok(String::from(
                    "/Users/dev/projects/ez-mux/ez-mux-1",
                ))
            })
            .expect("startup cwd should resolve")
        })
        .collect::<Vec<_>>();

    assert_eq!(resolved[0], slot_worktrees[0]);
    assert_eq!(resolved[1], slot_worktrees[1]);
    assert_eq!(resolved[2], slot_worktrees[2]);
}

#[test]
fn non_startup_mode_switch_uses_captured_pane_cwd() {
    let captured = resolve_mode_switch_cwd(false, "/repo-2", || Ok(String::from("/repo-2/src")))
        .expect("captured cwd should resolve");

    assert_eq!(captured, "/repo-2/src");
}

#[test]
fn startup_mode_switch_enables_fast_path() {
    assert!(use_startup_fast_path(true));
    assert!(!use_startup_fast_path(false));
}
