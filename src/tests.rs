use std::io;
use std::path::Path;

use super::*;

use crate::config::OperatingSystem;
use crate::logging::LogOpener;

#[derive(Default)]
struct TestEnv {
    vars: std::collections::HashMap<String, String>,
}

impl config::EnvProvider for TestEnv {
    fn get_var(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }
}

impl TestEnv {
    fn with_temp_state() -> (Self, tempfile::TempDir) {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut vars = std::collections::HashMap::new();
        vars.insert(String::from("HOME"), String::from("/tmp"));
        vars.insert(
            String::from("XDG_STATE_HOME"),
            temp.path().display().to_string(),
        );
        (Self { vars }, temp)
    }
}

struct FailingOpener;

impl LogOpener for FailingOpener {
    fn open(&self, _: OperatingSystem, _: &Path) -> io::Result<()> {
        Err(io::Error::other("simulated opener failure"))
    }
}

#[test]
fn success_writes_stdout_only() {
    let (env, _state) = TestEnv::with_temp_state();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = run_with_io(
        ["ezm", "--help"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::Success.as_i32());
    let stdout = String::from_utf8(stdout).expect("utf8");
    assert!(stdout.contains("Usage:"));
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(!stderr.contains("active log file:"));
}

#[test]
fn usage_errors_return_usage_code() {
    let (env, _state) = TestEnv::with_temp_state();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = run_with_io(
        ["ezm", "unknown"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::UsageOrConfigFailure.as_i32());
    assert_eq!(String::from_utf8(stdout).expect("utf8"), "");
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(stderr.contains("error:"));
    assert!(!stderr.contains("active log file:"));
}

#[test]
fn help_writes_to_stdout_only() {
    let (env, _state) = TestEnv::with_temp_state();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = run_with_io(
        ["ezm", "--help"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::Success.as_i32());
    let stdout = String::from_utf8(stdout).expect("utf8");
    assert!(stdout.contains("Usage:"));
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(!stderr.contains("active log file:"));
}

#[test]
fn each_launch_creates_a_new_log_file() {
    let (env, _state) = TestEnv::with_temp_state();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let first_code = run_with_io(
        ["ezm", "-v", "--help"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );
    let first_stderr = String::from_utf8(stderr).expect("utf8");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let second_code = run_with_io(
        ["ezm", "-v", "--help"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );
    let second_stderr = String::from_utf8(stderr).expect("utf8");

    assert_eq!(first_code, ExitCode::Success.as_i32());
    assert_eq!(second_code, ExitCode::Success.as_i32());

    let first_path = first_stderr
        .lines()
        .find_map(|line| line.strip_prefix("active log file: "))
        .expect("first path");
    let second_path = second_stderr
        .lines()
        .find_map(|line| line.strip_prefix("active log file: "))
        .expect("second path");

    assert_ne!(first_path, second_path);
}

#[test]
fn warns_and_continues_when_primary_log_root_creation_fails() {
    let temp = tempfile::tempdir().expect("tempdir");
    let primary_base_file = temp.path().join("xdg-state-file");
    std::fs::write(&primary_base_file, "not a directory").expect("write xdg state file");

    let mut vars = std::collections::HashMap::new();
    vars.insert(String::from("HOME"), String::from("/tmp"));
    vars.insert(
        String::from("XDG_STATE_HOME"),
        primary_base_file.display().to_string(),
    );
    let env = TestEnv { vars };

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with_io(
        ["ezm", "-v", "--help"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::Success.as_i32());
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(stderr.contains("warning: failed to create primary log root"));
    assert!(stderr.contains("using fallback"));
    assert!(stderr.contains("active log file:"));
}

#[test]
fn runtime_failures_write_stderr_only_and_use_runtime_code() {
    let (env, _state) = TestEnv::with_temp_state();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = run_with_io_and_opener(
        ["ezm", "-v", "logs", "open-latest"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
        &FailingOpener,
    );

    assert_eq!(code, ExitCode::RuntimeFailure.as_i32());
    assert_eq!(String::from_utf8(stdout).expect("utf8"), "");
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(stderr.contains("active log file:"));
    assert!(stderr.contains("error:"));
    assert!(stderr.contains("failed opening log file"));
}

#[test]
fn config_failures_write_stderr_only_and_use_usage_code() {
    let temp = tempfile::tempdir().expect("tempdir");
    let invalid_config = temp.path().join("invalid-config.toml");
    std::fs::write(&invalid_config, "operator = [").expect("write invalid config");

    let mut vars = std::collections::HashMap::new();
    vars.insert(String::from("HOME"), String::from("/tmp"));
    vars.insert(
        String::from("XDG_STATE_HOME"),
        temp.path().display().to_string(),
    );
    vars.insert(
        String::from(crate::config::EZM_CONFIG_ENV),
        invalid_config.display().to_string(),
    );
    let env = TestEnv { vars };

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with_io(
        ["ezm"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::UsageOrConfigFailure.as_i32());
    assert_eq!(String::from_utf8(stdout).expect("utf8"), "");
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(!stderr.contains("active log file:"));
    assert!(stderr.contains("error:"));
    assert!(stderr.contains("invalid TOML"));
}

#[test]
fn remote_path_without_remote_server_url_does_not_require_operator() {
    let (mut env, state) = TestEnv::with_temp_state();
    env.vars.insert(
        String::from(crate::config::EZM_REMOTE_PATH_ENV),
        String::from("/srv/remotes"),
    );
    env.vars.insert(
        String::from(crate::config::EZM_CONFIG_ENV),
        state.path().join("empty-config.toml").display().to_string(),
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with_io(
        [
            "ezm",
            "-v",
            "__internal",
            "mode",
            "--session",
            "ezm-test-session",
            "--slot",
            "4",
            "--mode",
            "shell",
        ],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::RuntimeFailure.as_i32());
    assert_eq!(String::from_utf8(stdout).expect("utf8"), "");
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(stderr.contains("active log file:"));
    assert!(!stderr.contains("OPERATOR"));

    let active_log = extract_active_log_path(&stderr).expect("active log path");
    let content = std::fs::read_to_string(active_log).expect("read launch log");
    assert!(content.contains("event=launch-failure"));
    assert!(!content.contains("OPERATOR"));
}

#[test]
fn invalid_shared_server_url_fails_fast_without_leaking_password() {
    let (mut env, _state) = TestEnv::with_temp_state();
    env.vars.insert(
        String::from(crate::config::OPENCODE_SERVER_URL_ENV),
        String::from("invalid-url"),
    );
    env.vars.insert(
        String::from(crate::config::OPENCODE_SERVER_PASSWORD_ENV),
        String::from("top-secret-token"),
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with_io(
        ["ezm"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::UsageOrConfigFailure.as_i32());
    assert_eq!(String::from_utf8(stdout).expect("utf8"), "");
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(!stderr.contains("active log file:"));
    assert!(stderr.contains("invalid OpenCode server URL"));
    assert!(!stderr.contains("top-secret-token"));
}

#[test]
fn verbose_mode_emits_active_log_path() {
    let (env, _state) = TestEnv::with_temp_state();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let code = run_with_io(
        ["ezm", "-v", "--help"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::Success.as_i32());
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(stderr.contains("active log file:"));
}

struct BrokenPipeWriter;

impl std::io::Write for BrokenPipeWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "closed",
        ))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct OtherIoErrorWriter;

impl std::io::Write for OtherIoErrorWriter {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("disk full"))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn broken_pipe_on_stdout_returns_success() {
    let (env, _state) = TestEnv::with_temp_state();
    let mut stdout = BrokenPipeWriter;
    let mut stderr = Vec::new();

    let code = run_with_io(
        ["ezm", "--help"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::Success.as_i32());
}

#[test]
fn non_broken_pipe_stdout_write_failure_is_runtime_failure() {
    let (env, _state) = TestEnv::with_temp_state();
    let mut stdout = OtherIoErrorWriter;
    let mut stderr = Vec::new();

    let code = run_with_io(
        ["ezm", "--help"],
        &env,
        OperatingSystem::Linux,
        &mut stdout,
        &mut stderr,
    );

    assert_eq!(code, ExitCode::RuntimeFailure.as_i32());
    let stderr = String::from_utf8(stderr).expect("utf8");
    assert!(stderr.contains("failed writing output"));
}

fn extract_active_log_path(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("active log file: "))
        .map(str::to_owned)
}
