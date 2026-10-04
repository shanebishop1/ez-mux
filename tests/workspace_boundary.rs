use std::{fs, process::Command};
use tempfile::TempDir;

#[test]
fn explicit_workspace_status_needs_no_project_registry_or_agent_backend() {
    let root = TempDir::new().unwrap();
    let manifest = root.path().join("workspace.json");
    fs::write(&manifest, serde_json::json!({"name":"demo", "root":root.path(), "groups":{"main":{"slots":[{"directory":root.path()}]}}}).to_string()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env_remove("EZM_CONFIG")
        .args(["workspace", "--file", manifest.to_str().unwrap(), "status"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("main: closed"));
}

#[test]
fn machine_and_agent_policy_is_rejected_by_workspace_manifest() {
    for (field, value) in [
        ("host", "devbox"),
        ("server", "http://localhost:4096"),
        ("credentials", "/private/auth"),
    ] {
        let root = TempDir::new().unwrap();
        let manifest = root.path().join("workspace.json");
        let mut document = serde_json::json!({"name":"demo", "root":root.path()});
        document[field] = value.into();
        fs::write(&manifest, document.to_string()).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_ezm"))
            .env("HOME", root.path())
            .env_remove("EZM_CONFIG")
            .args(["workspace", "--file", manifest.to_str().unwrap(), "status"])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("workspace manifest"));
    }
}

#[test]
fn unoverridden_slot_uses_normal_agent_fallback() {
    use std::os::unix::fs::PermissionsExt;
    let root = TempDir::new().unwrap();
    let binary = root.path().join("opencode");
    fs::write(&binary, "#!/bin/sh\nprintf 'normal-agent'\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let other = root.path().join("other");
    fs::create_dir(&other).unwrap();
    let manifest = root.path().join("workspace.json");
    fs::write(&manifest, serde_json::json!({"name":"demo", "root":root.path(), "groups":{"main":{"slots":[{"directory":other,"command":"true"},{"directory":root.path()}]}}}).to_string()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env("SHELL", "/usr/bin/true")
        .env("PATH", format!("{}:/usr/bin:/bin", root.path().display()))
        .env_remove("EZM_AGENT_COMMAND")
        .env_remove("EZM_CONFIG")
        .args([
            "__internal",
            "workspace-agent",
            "--file",
            manifest.to_str().unwrap(),
            "--group",
            "main",
            "--slot",
            "2",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stdout).contains("normal-agent"));
}

#[test]
fn custom_agent_command_preserves_literal_text_and_exports_slot() {
    let root = TempDir::new().unwrap();
    let manifest = root.path().join("workspace.json");
    fs::write(
        &manifest,
        serde_json::json!({"name":"demo", "root":root.path(),
        "groups":{"main":{"slots":[{"directory":root.path(),
        "command":"printf '%s|%s' \"$EZM_SLOT\" '{slot}'"}]}}})
        .to_string(),
    )
    .unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env_remove("EZM_CONFIG")
        .args([
            "__internal",
            "workspace-agent",
            "--file",
            manifest.to_str().unwrap(),
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
    assert_eq!(String::from_utf8_lossy(&result.stdout), "1|{slot}");
}

#[test]
fn explicit_layout_allows_shared_directories_and_extra_empty_panes() {
    let root = TempDir::new().unwrap();
    let manifest = root.path().join("workspace.json");
    fs::write(&manifest, serde_json::json!({"name":"demo", "root":root.path(),
        "groups":{"main":{"panes":3,"slots":[{"directory":root.path()},{"directory":root.path()}]}}}).to_string()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env_remove("EZM_CONFIG")
        .args(["workspace", "--file", manifest.to_str().unwrap(), "status"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
