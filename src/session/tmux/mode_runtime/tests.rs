use super::{
    launch_agent_attach_command, launch_command_for_mode,
    launch_command_with_remote_dir_from_mapping, resolve_mode_switch_cwd,
    resolve_persistent_transition_cwd, startup_mode_signal_enabled, use_startup_fast_path,
};
use crate::session::{
    RemoteModeContext, SharedServerAttachConfig, SlotMode, SlotModeLaunchContext,
};

fn remote_context<'a>(
    remote_path: Option<&'a str>,
    remote_server_url: Option<&'a str>,
) -> RemoteModeContext<'a> {
    RemoteModeContext {
        remote_path,
        remote_server_url,
        use_mosh: false,
    }
}

fn remote_context_with_transport<'a>(
    remote_path: Option<&'a str>,
    remote_server_url: Option<&'a str>,
    use_mosh: bool,
) -> RemoteModeContext<'a> {
    RemoteModeContext {
        remote_path,
        remote_server_url,
        use_mosh,
    }
}

fn mode_launch_context<'a>(
    remote_context: RemoteModeContext<'a>,
    shared_server: Option<&'a SharedServerAttachConfig>,
    agent_command: Option<&'a str>,
    opencode_theme: Option<&'a str>,
) -> SlotModeLaunchContext<'a> {
    SlotModeLaunchContext {
        remote_context,
        shared_server,
        agent_command,
        opencode_theme,
    }
}

#[test]
fn shell_mode_remote_prefix_launches_over_ssh_with_remote_dir() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let command = launch_command_with_remote_dir_from_mapping(
        SlotMode::Shell,
        "exec \"${SHELL:-/bin/sh}\" -l",
        &nested.display().to_string(),
        remote_context(Some("/srv/remotes"), Some("devbox-ez-1")),
    )
    .expect("command should resolve");

    assert!(command.contains("if ssh -tt 'devbox-ez-1'"));
    assert!(command.contains("cd '\"'\"'/srv/remotes/alpha/worktrees/feature-x'\"'\"'"));
}

#[test]
fn shell_mode_remote_prefix_uses_ssh_port_for_absolute_url_authority() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let command = launch_command_with_remote_dir_from_mapping(
        SlotMode::Shell,
        "exec \"${SHELL:-/bin/sh}\" -l",
        &nested.display().to_string(),
        remote_context(
            Some("/srv/remotes"),
            Some("https://shell.remote.example:7443"),
        ),
    )
    .expect("command should resolve");

    assert!(command.contains("if ssh -tt -p 7443 'shell.remote.example'"));
}

#[test]
fn shell_mode_remote_prefix_uses_mosh_when_enabled() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let command = launch_command_with_remote_dir_from_mapping(
        SlotMode::Shell,
        "exec \"${SHELL:-/bin/sh}\" -l",
        &nested.display().to_string(),
        remote_context_with_transport(
            Some("/srv/remotes"),
            Some("https://shell.remote.example:7443"),
            true,
        ),
    )
    .expect("command should resolve");

    assert!(command.contains("if mosh --ssh='ssh -p 7443' 'shell.remote.example'"));
    assert!(!command.contains("if ssh -tt -p 7443 'shell.remote.example'"));
}

#[test]
fn shell_mode_remote_prefix_fails_fast_for_invalid_remote_authority() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let error = launch_command_with_remote_dir_from_mapping(
        SlotMode::Shell,
        "exec \"${SHELL:-/bin/sh}\" -l",
        &nested.display().to_string(),
        remote_context(Some("/srv/remotes"), Some("https://shell.remote.example:")),
    )
    .expect_err("invalid authority should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("invalid remote ssh authority"));
    assert!(rendered.contains("EZM_REMOTE_SERVER_URL"));
}

#[test]
fn shell_mode_remote_prefix_failure_redacts_authority_password_in_diagnostics() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let error = launch_command_with_remote_dir_from_mapping(
        SlotMode::Shell,
        "exec \"${SHELL:-/bin/sh}\" -l",
        &nested.display().to_string(),
        remote_context(
            Some("/srv/remotes"),
            Some("https://operator:super-secret@shell.remote.example:"),
        ),
    )
    .expect_err("invalid authority should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("operator:<redacted>@shell.remote.example:"));
    assert!(!rendered.contains("super-secret"));
    assert!(rendered.contains("invalid remote ssh authority"));
}

#[test]
fn lazygit_mode_remote_prefix_launches_over_ssh() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let command = launch_command_with_remote_dir_from_mapping(
        SlotMode::Lazygit,
        "if command -v lazygit >/dev/null 2>&1; then lazygit; exit_code=$?; if [ \"$exit_code\" -ne 0 ]; then printf '%s\\n' \"ez-mux mode tool lazygit exited with status $exit_code\" >&2; :; fi; fi; exec \"${SHELL:-/bin/sh}\" -l",
        &nested.display().to_string(),
        remote_context(Some("/srv/remotes"), Some("devbox-ez-1")),
    )
    .expect("command should resolve");

    assert!(command.contains("if ssh -tt 'devbox-ez-1'"));
    assert!(command.contains("lazygit"));
    assert!(command.contains("; :; fi; fi; exec \"${SHELL:-/bin/sh}\" -l"));
    assert!(!command.contains("exit \"$exit_code\""));
    assert!(!command.contains("status=$?"));
    assert!(command.contains("${SHELL:-/bin/sh}"));
}

#[test]
fn missing_remote_mapping_keeps_original_launch_command() {
    let command = launch_command_with_remote_dir_from_mapping(
        SlotMode::Shell,
        "exec \"${SHELL:-/bin/sh}\" -l",
        "/tmp/local-only",
        RemoteModeContext::default(),
    )
    .expect("command should resolve");

    assert_eq!(command, "exec \"${SHELL:-/bin/sh}\" -l");
}

#[test]
fn neovim_mode_remote_prefix_launches_over_ssh() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let command = launch_command_with_remote_dir_from_mapping(
        SlotMode::Neovim,
        "if command -v nvim >/dev/null 2>&1; then nvim; fi; exec \"${SHELL:-/bin/sh}\" -l",
        &nested.display().to_string(),
        remote_context(
            Some("/srv/remotes"),
            Some("https://shell.remote.example:7443"),
        ),
    )
    .expect("command should resolve");

    assert!(command.contains("if ssh -tt -p 7443 'shell.remote.example'"));
    assert!(command.contains("nvim"));
    assert!(command.contains("cd '\"'\"'/srv/remotes/alpha/worktrees/feature-x'\"'\"'"));
}

#[test]
fn agent_mode_uses_shared_server_attach_url_and_mapped_dir() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let command = launch_agent_attach_command(
        1,
        &nested.display().to_string(),
        Some("/srv/remotes"),
        &SharedServerAttachConfig {
            url: String::from("http://127.0.0.1:4096"),
            password: None,
        },
        None,
    )
    .expect("agent command should resolve");

    assert!(command.contains("opencode attach 'http://127.0.0.1:4096'"));
    assert!(command.contains("--dir '/srv/remotes/alpha/worktrees/feature-x'"));
}

#[test]
fn agent_mode_password_is_included_when_configured() {
    let command = launch_agent_attach_command(
        1,
        "/tmp/local-only",
        None,
        &SharedServerAttachConfig {
            url: String::from("http://127.0.0.1:4096"),
            password: Some(String::from("secret-token")),
        },
        None,
    )
    .expect("agent command should resolve");

    assert!(command.contains("--password 'secret-token'"));
}

#[test]
fn agent_mode_requires_non_empty_attach_url_when_shared_server_config_is_used() {
    let error = launch_agent_attach_command(
        1,
        "/tmp/local-only",
        None,
        &SharedServerAttachConfig {
            url: String::new(),
            password: None,
        },
        None,
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
        2,
        SlotMode::Agent,
        "exec opencode || exec \"${SHELL:-/bin/sh}\" -l",
        "/tmp/local-only",
        mode_launch_context(RemoteModeContext::default(), None, None, None),
    )
    .expect("agent local launch should resolve");

    assert_eq!(command, "exec opencode || exec \"${SHELL:-/bin/sh}\" -l");
}

#[test]
fn agent_mode_uses_remote_path_mapping_without_operator() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");

    let command = launch_command_for_mode(
        3,
        SlotMode::Agent,
        "placeholder",
        &repo_root.display().to_string(),
        mode_launch_context(
            remote_context(Some("/srv/remotes"), None),
            Some(&SharedServerAttachConfig {
                url: String::from("http://127.0.0.1:4096"),
                password: None,
            }),
            None,
            None,
        ),
    )
    .expect("agent mode should resolve");

    assert!(command.contains("opencode attach 'http://127.0.0.1:4096'"));
}

#[test]
fn agent_mode_theme_sets_custom_tui_config_for_attach_launches() {
    let command = launch_agent_attach_command(
        2,
        "/tmp/local-only",
        None,
        &SharedServerAttachConfig {
            url: String::from("http://127.0.0.1:4096"),
            password: None,
        },
        Some("orng"),
    )
    .expect("agent command should resolve");

    assert!(command.contains("OPENCODE_CONFIG_DIR"));
    assert!(command.contains("OPENCODE_TUI_CONFIG"));
    assert!(command.contains("OPENCODE_TEST_MANAGED_CONFIG_DIR"));
    assert!(command.contains("slot-2"));

    let path = command
        .split_once("OPENCODE_TUI_CONFIG='")
        .and_then(|(_, rest)| rest.split_once('\''))
        .map(|(path, _)| path)
        .expect("theme command should include exported tui config path");
    let rendered = std::fs::read_to_string(path).expect("theme config should be written");
    assert!(rendered.contains("\"theme\": \"orng\""));
}

#[test]
fn agent_mode_theme_sets_custom_tui_config_for_local_launches() {
    let command = launch_command_for_mode(
        4,
        SlotMode::Agent,
        "exec opencode || exec \"${SHELL:-/bin/sh}\" -l",
        "/tmp/local-only",
        mode_launch_context(RemoteModeContext::default(), None, None, Some("catppuccin")),
    )
    .expect("agent local launch should resolve");

    assert!(command.contains("OPENCODE_CONFIG_DIR"));
    assert!(command.contains("OPENCODE_TUI_CONFIG"));
    assert!(command.contains("OPENCODE_TEST_MANAGED_CONFIG_DIR"));
    assert!(command.contains("slot-4"));
}

#[test]
fn agent_mode_uses_configured_override_command_when_present() {
    let command = launch_command_for_mode(
        1,
        SlotMode::Agent,
        "exec opencode || exec \"${SHELL:-/bin/sh}\" -l",
        "/tmp/local-only",
        mode_launch_context(
            RemoteModeContext::default(),
            Some(&SharedServerAttachConfig {
                url: String::from("http://127.0.0.1:4096"),
                password: Some(String::from("secret")),
            }),
            Some("exec claude || exec \"${SHELL:-/bin/sh}\" -l"),
            Some("nightowl"),
        ),
    )
    .expect("agent override launch should resolve");

    assert_eq!(command, "exec claude || exec \"${SHELL:-/bin/sh}\" -l");
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
                Ok(String::from("/Users/dev/projects/ez-mux/ez-mux-1"))
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
fn persistent_switch_uses_assigned_worktree_when_switching_away_from_agent() {
    let resolved =
        resolve_persistent_transition_cwd("agent", "/repo-2", || Ok(String::from("/home/shane")))
            .expect("persistent cwd should resolve");

    assert_eq!(resolved, "/repo-2");
}

#[test]
fn persistent_switch_uses_captured_cwd_for_non_agent_modes() {
    let resolved =
        resolve_persistent_transition_cwd("shell", "/repo-2", || Ok(String::from("/repo-2/src")))
            .expect("persistent cwd should resolve");

    assert_eq!(resolved, "/repo-2/src");
}

#[test]
fn startup_mode_switch_enables_fast_path() {
    assert!(use_startup_fast_path(true));
    assert!(!use_startup_fast_path(false));
}

#[test]
fn startup_mode_signal_reads_truthy_environment_values() {
    assert!(startup_mode_signal_enabled(Some("1")));
    assert!(startup_mode_signal_enabled(Some("true")));
    assert!(startup_mode_signal_enabled(Some(" yes ")));
    assert!(startup_mode_signal_enabled(Some("on")));
}

#[test]
fn startup_mode_signal_defaults_to_non_startup_when_unset() {
    assert!(!startup_mode_signal_enabled(None));
    assert!(!startup_mode_signal_enabled(Some("0")));
    assert!(!startup_mode_signal_enabled(Some("false")));
}
