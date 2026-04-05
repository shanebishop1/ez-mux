use super::{
    AUXILIARY_WINDOW_NAME, build_auxiliary_local_launch_command,
    build_auxiliary_remote_launch_command, discover_executable_in_path,
    resolve_auxiliary_remote_launch, should_validate_registry_for_auxiliary,
};

#[test]
fn auxiliary_launch_command_uses_resolved_perles_path() {
    let command =
        build_auxiliary_local_launch_command(std::path::Path::new("/tmp/tools/perles"), None, None);
    assert_eq!(
        command,
        "'/tmp/tools/perles'; exit_code=$?; if [ \"$exit_code\" -ne 0 ]; then printf '%s\\n' \"ez-mux auxiliary viewer perles exited with status $exit_code\" >&2; fi; exec \"${SHELL:-/bin/sh}\" -l"
    );
}

#[test]
fn auxiliary_launch_command_escapes_single_quote_sensitive_characters() {
    let command = build_auxiliary_local_launch_command(
        std::path::Path::new("/tmp/tools/space and 'quote'/$HOME/`cmd`/perles"),
        Some("  /tmp/beads dir/it's $HOME  "),
        Some("/tmp/beads-db/`cmd`-'set'.jsonl"),
    );
    assert_eq!(
        command,
        "export PERLES_DIR='/tmp/beads dir/it'\"'\"'s $HOME'; export PERLES_DB='/tmp/beads-db/`cmd`-'\"'\"'set'\"'\"'.jsonl'; '/tmp/tools/space and '\"'\"'quote'\"'\"'/$HOME/`cmd`/perles'; exit_code=$?; if [ \"$exit_code\" -ne 0 ]; then printf '%s\\n' \"ez-mux auxiliary viewer perles exited with status $exit_code\" >&2; fi; exec \"${SHELL:-/bin/sh}\" -l"
    );
}

#[test]
fn auxiliary_remote_launch_command_routes_over_ssh_with_remote_directory() {
    let command = build_auxiliary_remote_launch_command(
        "/srv/remotes/ez-mux",
        "https://shell.remote.example:7443",
        false,
    )
    .expect("remote command should build");

    assert!(command.contains("ssh -tt -p 7443 'shell.remote.example'"));
    assert!(command.contains("/srv/remotes/ez-mux"));
    assert!(command.contains("\"${SHELL:-/bin/sh}\" -lic '"));
    assert!(command.contains("command -v perles"));
    assert!(command.contains("exec \"${SHELL:-/bin/sh}\" -l"));
}

#[test]
fn auxiliary_remote_launch_command_does_not_export_perles_paths() {
    let command = build_auxiliary_remote_launch_command(
        "/srv/remotes/ez-mux",
        "https://shell.remote.example",
        false,
    )
    .expect("remote command should build");

    assert!(!command.contains("export PERLES_DIR="));
    assert!(!command.contains("export PERLES_DB="));
}

#[test]
fn auxiliary_remote_launch_bootstraps_shell_before_running_perles() {
    let command = build_auxiliary_remote_launch_command(
        "/srv/remotes/ez-mux",
        "https://shell.remote.example",
        false,
    )
    .expect("remote command should build");

    let bootstrap_index = command
        .find("\"${SHELL:-/bin/sh}\" -lic '")
        .expect("remote command should use login+interactive shell bootstrap");
    let perles_index = command
        .find("command -v perles")
        .expect("remote command should invoke perles discovery");
    assert!(bootstrap_index < perles_index);
}

#[test]
fn auxiliary_remote_launch_command_fails_fast_for_invalid_remote_authority() {
    let error = build_auxiliary_remote_launch_command(
        "/srv/remotes/ez-mux",
        "https://shell.remote.example:",
        false,
    )
    .expect_err("invalid authority should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("invalid remote ssh authority"));
    assert!(rendered.contains("EZM_REMOTE_SERVER_URL"));
}

#[test]
fn auxiliary_remote_launch_resolves_when_remote_path_and_server_url_are_present() {
    let resolved = resolve_auxiliary_remote_launch(
        "/tmp/repo/worktree",
        Some("/srv/remotes"),
        Some("https://shell.remote.example:7443"),
        false,
    )
    .expect("remote launch should resolve")
    .expect("remote launch should be active");

    assert_eq!(resolved.remote_dir, "/srv/remotes/worktree");
    assert_eq!(
        resolved.remote_server_url,
        "https://shell.remote.example:7443"
    );
    assert!(!resolved.use_mosh);
}

#[test]
fn auxiliary_remote_launch_is_inactive_without_server_url() {
    let resolved =
        resolve_auxiliary_remote_launch("/tmp/repo/worktree", Some("/srv/remotes"), None, false)
            .expect("missing server url should not fail");
    assert!(resolved.is_none());
}

#[test]
fn auxiliary_remote_launch_command_uses_mosh_when_enabled() {
    let command = build_auxiliary_remote_launch_command(
        "/srv/remotes/ez-mux",
        "https://shell.remote.example:7443",
        true,
    )
    .expect("remote command should build");

    assert!(command.contains("mosh --no-init --ssh='ssh -p 7443' 'shell.remote.example'"));
    assert!(!command.contains("ssh -tt -p 7443 'shell.remote.example'"));
}

#[test]
fn discover_executable_returns_none_for_missing_binary_name() {
    let unlikely = format!("ezm-no-such-tool-{AUXILIARY_WINDOW_NAME}");
    assert!(discover_executable_in_path(&unlikely).is_none());
}

#[test]
fn auxiliary_open_skips_registry_validation_on_create_path() {
    assert!(!should_validate_registry_for_auxiliary(true));
    assert!(should_validate_registry_for_auxiliary(false));
}
