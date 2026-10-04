use clap::Parser;

#[test]
fn layout_cli_rejects_project_orchestration() {
    for verb in [
        "open", "start", "attach", "new", "adopt", "status", "doctor", "close", "stop",
    ] {
        assert!(
            ez_mux::cli::Cli::try_parse_from(["ezm", verb, "demo"]).is_err(),
            "ezm accepted {verb}"
        );
    }
}

#[test]
fn orchestration_cli_is_distinct_and_requires_its_pinned_layout_companion() {
    use std::{fs, process::Command};
    let binary = env!("CARGO_BIN_EXE_remote-agents");
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("remote-agents"));
    let wrong = Command::new(binary)
        .args(["preset", "--preset", "three"])
        .output()
        .unwrap();
    assert!(!wrong.status.success());
    let home = tempfile::tempdir().unwrap();
    let alone = home.path().join("remote-agents");
    fs::copy(binary, &alone).unwrap();
    let missing = Command::new(alone)
        .env("HOME", home.path())
        .args(["start", "demo"])
        .output()
        .unwrap();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("Missing companion ezm"));
    assert!(!home.path().join(".config").exists());
}
