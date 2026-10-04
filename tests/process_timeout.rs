use std::{
    fs,
    os::unix::fs::PermissionsExt,
    process::Command,
    time::{Duration, Instant},
};
#[test]
fn stalled_api_is_bounded_and_creates_no_mapping() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("opencode");
    fs::write(
        &bin,
        "#!/usr/bin/env python3\nimport time\ntime.sleep(60)\n",
    )
    .unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o700)).unwrap();
    let began = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_ezm"))
        .env("HOME", root.path())
        .env("EZM_OPENCODE_BIN", bin)
        .args([
            "__internal",
            "opencode",
            "--server",
            "http://localhost:4096",
            "--directory",
            "/tmp",
            "--key",
            "stall",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(began.elapsed() < Duration::from_secs(15));
    assert!(String::from_utf8_lossy(&out.stderr).contains("timed out"));
}
