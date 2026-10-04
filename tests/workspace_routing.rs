use std::{fs, os::unix::fs::PermissionsExt, process::Command};
use tempfile::TempDir;

#[test]
fn destination_only_registry_routes_without_local_worktree_configuration() {
    let root = TempDir::new().unwrap();
    let registry = root.path().join("projects.toml");
    fs::write(&registry, "[projects.demo]\nhost = 'my-devbox'\n").unwrap();
    let ssh = root.path().join("ssh");
    fs::write(
        &ssh,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOME/ssh-args\"\n",
    )
    .unwrap();
    fs::set_permissions(&ssh, fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env("PATH", format!("{}:/usr/bin:/bin", root.path().display()))
        .env("EZM_PROJECTS_CONFIG", &registry)
        .env_remove("EZM_CONFIG")
        .args(["status", "demo"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = fs::read_to_string(root.path().join("ssh-args")).unwrap();
    assert!(args.contains("my-devbox"));
    assert!(args.contains("'status' 'demo' '--local'"));
}

#[test]
fn named_remote_route_honors_existing_tssh_setting() {
    let root = TempDir::new().unwrap();
    let registry = root.path().join("projects.toml");
    fs::write(&registry, "[projects.demo]\nhost = 'my-devbox'\n").unwrap();
    let config = root.path().join("ez-mux.toml");
    fs::write(&config, "ezm_use_tssh = true\n").unwrap();
    let tssh = root.path().join("tssh");
    fs::write(
        &tssh,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOME/transport-args\"\n",
    )
    .unwrap();
    fs::set_permissions(&tssh, fs::Permissions::from_mode(0o700)).unwrap();
    let ssh = root.path().join("ssh");
    fs::write(&ssh, "#!/bin/sh\nexit 73\n").unwrap();
    fs::set_permissions(&ssh, fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env("PATH", format!("{}:/usr/bin:/bin", root.path().display()))
        .env("EZM_PROJECTS_CONFIG", &registry)
        .env("EZM_CONFIG", &config)
        .env_remove("EZM_USE_TSSH")
        .args(["status", "demo"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(root.path().join("transport-args"))
            .unwrap()
            .contains("my-devbox")
    );
}

#[test]
fn direct_attachment_expands_the_inherited_agent_slot() {
    let root = TempDir::new().unwrap();
    let registry = root.path().join("projects.toml");
    fs::write(&registry, format!("[projects.demo]\nroot = {path:?}\n[[projects.demo.groups.main.slots]]\ndirectory = {path:?}\n", path=root.path().to_str().unwrap())).unwrap();
    let config = root.path().join("ez-mux.toml");
    fs::write(&config, "agent_command = \"printf '%s' '{slot}'\"\n").unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env("EZM_PROJECTS_CONFIG", &registry)
        .env("EZM_CONFIG", &config)
        .args(["attach", "demo", "--slot", "1"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&result.stdout).trim(), "1");
}

#[test]
fn explicit_group_rejects_unowned_extra_visible_panes() {
    let root = TempDir::new().unwrap();
    let registry = root.path().join("projects.toml");
    fs::write(&registry, format!("[projects.demo]\nroot = {path:?}\n[projects.demo.groups.main]\npanes = 5\n[[projects.demo.groups.main.slots]]\ndirectory = {path:?}\n", path=root.path().to_str().unwrap())).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env("EZM_PROJECTS_CONFIG", registry)
        .args(["doctor", "demo"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Explicit groups"));
}
