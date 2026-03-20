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
    let launch_log = match logging::initialize_launch_log_with_defaults(env, os) {
        Ok(launch_log) => launch_log,
        Err(error) => {
            let _ = writeln!(stderr, "error: {error}");
            return ExitCode::RuntimeFailure.as_i32();
        }
    };

    if let Some(warning) = &launch_log.warning {
        let _ = writeln!(stderr, "warning: {warning}");
    }
    let _ = writeln!(
        stderr,
        "active log file: {}",
        launch_log.file_path.display()
    );

    match cli::Cli::try_parse_from(args) {
        Ok(cli) => match app::execute(cli, env, os, &launch_log.root) {
            Ok(message) => {
                let _ = writeln!(stdout, "{message}");
                ExitCode::Success.as_i32()
            }
            Err(error) => {
                let _ = writeln!(stderr, "error: {error}");
                ExitCode::from_app_error(&error).as_i32()
            }
        },
        Err(parse_error) => match parse_error.kind() {
            clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                let _ = write!(stdout, "{parse_error}");
                ExitCode::Success.as_i32()
            }
            _ => {
                let _ = write!(stderr, "{parse_error}");
                ExitCode::UsageOrConfigFailure.as_i32()
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::OperatingSystem;

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

    #[test]
    fn success_writes_stdout_only() {
        let (env, _state) = TestEnv::with_temp_state();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let code = run_with_io(
            ["ezm", "repair"],
            &env,
            OperatingSystem::Linux,
            &mut stdout,
            &mut stderr,
        );

        assert_eq!(code, ExitCode::Success.as_i32());
        let stdout = String::from_utf8(stdout).expect("utf8");
        assert!(stdout.contains("repair contract entrypoint accepted"));
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
            ["ezm", "repair"],
            &env,
            OperatingSystem::Linux,
            &mut stdout,
            &mut stderr,
        );
        let first_stderr = String::from_utf8(stderr).expect("utf8");

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let second_code = run_with_io(
            ["ezm", "repair"],
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
            ["ezm", "repair"],
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

        let code = run_with_io(
            ["ezm", "logs", "open-latest"],
            &env,
            OperatingSystem::Linux,
            &mut stdout,
            &mut stderr,
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
}
