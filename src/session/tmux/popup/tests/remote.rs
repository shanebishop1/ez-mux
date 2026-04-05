use super::super::context::{PopupRemoteContext, resolve_popup_remote_context};
use super::super::remote_ssh::popup_remote_launch_command;
use super::super::session::popup_new_session_args;

#[test]
fn popup_new_session_uses_default_shell_without_lc_wrapper() {
    let args = popup_new_session_args("ezm-s100__popup_slot_4", "/tmp/popup-cwd", None)
        .expect("args should resolve");
    let rendered = args.join(" ");

    assert!(rendered.starts_with("new-session -d -s ezm-s100__popup_slot_4 -c /tmp/popup-cwd"));
    assert!(!rendered.contains("sh -lc"));
}

#[test]
fn popup_new_session_uses_remote_ssh_shell_when_remote_routing_is_active() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let nested = repo_root.join("feature");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&nested).expect("create nested");

    let remote_context = resolve_popup_remote_context(
        &nested.display().to_string(),
        Some("/srv/remotes"),
        Some("https://shell.remote.example:7443"),
        false,
    )
    .expect("context should resolve");

    let args = popup_new_session_args(
        "ezm-s100__popup_slot_4",
        "/tmp/popup-cwd",
        remote_context.as_ref(),
    )
    .expect("args should resolve");
    let rendered = args.join(" ");

    assert!(rendered.contains("new-session -d -s ezm-s100__popup_slot_4 -c /tmp/popup-cwd"));
    assert!(rendered.contains("sh -lc"));
    assert!(rendered.contains("ssh -tt -p 7443"));
    assert!(rendered.contains("shell.remote.example"));
    assert!(rendered.contains("cd '"));
}

#[test]
fn popup_remote_launch_command_returns_none_without_server_url() {
    let context = PopupRemoteContext {
        remote_dir: String::from("/srv/remotes/alpha"),
        remote_server_url: None,
        use_mosh: false,
    };

    let command = popup_remote_launch_command(Some(&context)).expect("command should resolve");
    assert!(command.is_none());
}

#[test]
fn popup_new_session_args_fail_fast_for_invalid_remote_authority() {
    let context = PopupRemoteContext {
        remote_dir: String::from("/srv/remotes/alpha"),
        remote_server_url: Some(String::from("https://shell.remote.example:")),
        use_mosh: false,
    };

    let error = popup_new_session_args("ezm-s100__popup_slot_4", "/tmp/popup-cwd", Some(&context))
        .expect_err("invalid authority should fail");
    let rendered = error.to_string();
    assert!(rendered.contains("invalid remote ssh authority"));
    assert!(rendered.contains("EZM_REMOTE_SERVER_URL"));
}

#[test]
fn popup_remote_launch_command_uses_mosh_when_enabled() {
    let context = PopupRemoteContext {
        remote_dir: String::from("/srv/remotes/alpha"),
        remote_server_url: Some(String::from("https://shell.remote.example:7443")),
        use_mosh: true,
    };

    let command = popup_remote_launch_command(Some(&context))
        .expect("command should resolve")
        .expect("remote command should exist");

    assert!(command.contains("mosh --no-init"));
    assert!(command.contains("--ssh='"));
    assert!(command.contains("ssh -p 7443"));
    assert!(command.contains("shell.remote.example"));
    assert!(!command.contains("ssh -tt -p 7443"));
}
