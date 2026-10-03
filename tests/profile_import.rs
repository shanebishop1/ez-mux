use std::{fs, process::Command};
#[test]
fn import_is_non_destructive_and_does_not_copy_credentials() {
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join(".config/remote-agents/demo");
    fs::create_dir_all(&profile).unwrap();
    fs::write(profile.join("client.sh"),"REMOTE_HOST=devbox\nREMOTE_SLOT_DIRS=(/srv/demo-1 /srv/demo-2)\nOPENCODE_SERVER_PASSWORD=super-secret\n").unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_ezm"))
            .env("HOME", root.path())
            .env("XDG_CONFIG_HOME", root.path().join(".config"))
            .args(["import-remote-agents"])
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let p = root.path().join(".config/ez-mux/projects.toml");
    let before = fs::read_to_string(&p).unwrap();
    assert!(before.contains("/srv/demo-2"));
    assert!(!before.contains("super-secret"));
    let second = run();
    assert!(second.status.success());
    assert_eq!(fs::read_to_string(p).unwrap(), before);
}

#[test]
fn project_credentials_do_not_inherit_other_project_authentication() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let creds = root.path().join("credentials.env");
    fs::write(&creds, "# Intentionally no OpenCode credentials\n").unwrap();
    let binary = root.path().join("opencode");
    fs::write(
        &binary,
        r"#!/usr/bin/env python3
import os,json
assert os.environ['OPENCODE_SERVER_PASSWORD']==''
assert os.environ['OPENCODE_SERVER_USERNAME']=='opencode'
print(json.dumps({'data':{'version':'2.0.22'}}))
",
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let config = root.path().join("projects.toml");
    fs::write(
        &config,
        format!(
            r#"
[projects.demo]
root = {root:?}
server = "http://localhost:4096"
credentials = {creds:?}
opencode_binary = {binary:?}
[projects.demo.groups.main]
[[projects.demo.groups.main.slots]]
directory = {root:?}
"#,
            root = root.path().to_str().unwrap(),
            creds = creds.to_str().unwrap(),
            binary = binary.to_str().unwrap()
        ),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env("EZM_PROJECTS_CONFIG", config)
        .env("OPENCODE_SERVER_PASSWORD", "wrong-project")
        .env("OPENCODE_SERVER_USERNAME", "wrong-user")
        .args(["doctor", "demo"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn imported_auxiliary_loads_its_project_environment() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let profile = root.path().join(".config/remote-agents/demo");
    fs::create_dir_all(&profile).unwrap();
    let binary = root.path().join("perles");
    fs::write(&binary, "#!/bin/sh\nprintf '%s:%s:%s' \"$BEADS_DOLT_SERVER_HOST\" \"$BEADS_DOLT_SERVER_PORT\" \"$DOTFILES_REMOTE_HINT\"\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let credentials = profile.join("credentials.env");
    fs::write(&credentials, "# private configuration\n").unwrap();
    fs::write(profile.join("server.sh"), format!("source '{}'\nREMOTE_ROOT='{}'\nREMOTE_PERLES_BIN='{}'\nREMOTE_BD_BIN=/usr/local/bin/bd\nBEADS_DOLT_SERVER_HOST=project-db\nBEADS_DOLT_SERVER_PORT=3311\nDOTFILES_REMOTE_HINT=devbox\n", credentials.display(),root.path().display(),binary.display())).unwrap();
    let config = root.path().join("projects.toml");
    let result = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env("EZM_PROJECTS_CONFIG", &config)
        .args(["import-remote-agents", "--server"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let registry: toml::Value = toml::from_str(&fs::read_to_string(config).unwrap()).unwrap();
    let command = registry["projects"]["demo"]["perles"]["command"]
        .as_str()
        .unwrap();
    let output = Command::new("sh")
        .args(["-c", command])
        .env("BEADS_DOLT_SERVER_HOST", "wrong-project")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "project-db:3311:devbox"
    );
    assert_eq!(
        registry["projects"]["demo"]["groups"]["main"]["slots"][0]["require_existing"].as_bool(),
        Some(true)
    );
}

#[test]
fn stable_slot_identity_ignores_callers_working_directory() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("one");
    let second = root.path().join("two");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let config = root.path().join("projects.toml");
    fs::write(
        &config,
        format!(
            r"
[projects.demo]
root = {root:?}
[[projects.demo.groups.main.slots]]
directory = {first:?}
command = 'pwd'
[[projects.demo.groups.main.slots]]
directory = {second:?}
command = 'exit 42'
",
            root = root.path().to_str().unwrap(),
            first = first.to_str().unwrap(),
            second = second.to_str().unwrap()
        ),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .current_dir(second)
        .env("HOME", root.path())
        .env("EZM_PROJECTS_CONFIG", config)
        .args([
            "__internal",
            "workspace-agent",
            "--project",
            "demo",
            "--group",
            "main",
            "--slot",
            "1",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        first.canonicalize().unwrap().display().to_string()
    );
}
