#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod app;
pub mod cli;
pub mod config;
pub mod exit_code;
pub mod logging;
pub mod session;

use std::io::Write;

use clap::Parser;
use config::{OperatingSystem, ProcessEnv};
use exit_code::ExitCode;

#[must_use]
pub fn run() -> i32 {
    let env = ProcessEnv;
    run_with_io(
        std::env::args_os(),
        &env,
        OperatingSystem::current(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}

fn run_with_io<I, T>(
    args: I,
    env: &impl config::EnvProvider,
    os: OperatingSystem,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    run_with_io_and_opener(args, env, os, stdout, stderr, &logging::ProcessLogOpener)
}

fn run_with_io_and_opener<I, T>(
    args: I,
    env: &impl config::EnvProvider,
    os: OperatingSystem,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    opener: &impl logging::LogOpener,
) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let launch_log = match logging::initialize_launch_log_with_defaults(env, os) {
        Ok(launch_log) => launch_log,
        Err(error) => {
            if let Err(code) = checked_write(writeln!(stderr, "error: {error}"), stderr) {
                return code;
            }
            return ExitCode::RuntimeFailure.as_i32();
        }
    };

    if let Some(warning) = &launch_log.warning {
        if let Err(code) = checked_write(writeln!(stderr, "warning: {warning}"), stderr) {
            return code;
        }
    }
    if let Err(code) = checked_write(
        writeln!(
            stderr,
            "active log file: {}",
            launch_log.file_path.display()
        ),
        stderr,
    ) {
        return code;
    }

    match cli::Cli::try_parse_from(args) {
        Ok(cli) => match app::execute_with_opener(cli, env, os, &launch_log.root, opener) {
            Ok(message) => {
                if let Err(code) = checked_write(writeln!(stdout, "{message}"), stderr) {
                    return code;
                }
                ExitCode::Success.as_i32()
            }
            Err(error) => {
                if let Err(code) = checked_write(writeln!(stderr, "error: {error}"), stderr) {
                    return code;
                }
                ExitCode::from_app_error(&error).as_i32()
            }
        },
        Err(parse_error) => match parse_error.kind() {
            clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                if let Err(code) = checked_write(write!(stdout, "{parse_error}"), stderr) {
                    return code;
                }
                ExitCode::Success.as_i32()
            }
            _ => {
                if let Err(code) = checked_write(write!(stderr, "{parse_error}"), stderr) {
                    return code;
                }
                ExitCode::UsageOrConfigFailure.as_i32()
            }
        },
    }
}

fn checked_write(result: std::io::Result<()>, stderr: &mut impl Write) -> Result<(), i32> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {
            Err(ExitCode::Success.as_i32())
        }
        Err(error) => {
            let _ = writeln!(stderr, "error: failed writing output: {error}");
            Err(ExitCode::RuntimeFailure.as_i32())
        }
    }
}

#[cfg(test)]
mod tests {
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
        assert!(stderr.contains("active log file:"));
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
        assert!(stderr.contains("active log file:"));
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
        assert!(stderr.contains("active log file:"));
    }

    #[test]
    fn each_launch_creates_a_new_log_file() {
        let (env, _state) = TestEnv::with_temp_state();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let first_code = run_with_io(
            ["ezm", "--help"],
            &env,
            OperatingSystem::Linux,
            &mut stdout,
            &mut stderr,
        );
        let first_stderr = String::from_utf8(stderr).expect("utf8");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let second_code = run_with_io(
            ["ezm", "--help"],
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
            ["ezm", "--help"],
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
            ["ezm", "logs", "open-latest"],
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
        assert!(stderr.contains("active log file:"));
        assert!(stderr.contains("error:"));
        assert!(stderr.contains("invalid TOML"));
    }

    #[test]
    fn remote_prefix_without_operator_writes_clear_stderr_and_non_zero_exit() {
        let (mut env, state) = TestEnv::with_temp_state();
        env.vars.insert(
            String::from(crate::session::OPENCODE_REMOTE_DIR_PREFIX_ENV),
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
        assert!(stderr.contains("remote-prefix routing requires OPERATOR to be set"));
    }

    #[test]
    fn invalid_shared_server_port_fails_fast_without_leaking_password() {
        let (mut env, _state) = TestEnv::with_temp_state();
        env.vars.insert(
            String::from(crate::config::OPENCODE_SERVER_PORT_ENV),
            String::from("invalid-port"),
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
        assert!(stderr.contains("active log file:"));
        assert!(stderr.contains("invalid OpenCode server port"));
        assert!(!stderr.contains("top-secret-token"));
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
}
