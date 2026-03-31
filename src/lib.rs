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
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let parsed_cli = cli::Cli::try_parse_from(args.clone());
    let show_active_log_path = verbose_flag_present(&args);

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
    if show_active_log_path {
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
    }

    match parsed_cli {
        Ok(cli) => match app::execute_with_opener(cli, env, os, &launch_log.root, opener) {
            Ok(message) => {
                if !message.is_empty() {
                    if let Err(code) = checked_write(writeln!(stdout, "{message}"), stderr) {
                        return code;
                    }
                }
                ExitCode::Success.as_i32()
            }
            Err(error) => {
                append_launch_failure_event(&launch_log.file_path, &error.to_string());
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
                append_launch_failure_event(&launch_log.file_path, &parse_error.to_string());
                if let Err(code) = checked_write(write!(stderr, "{parse_error}"), stderr) {
                    return code;
                }
                ExitCode::UsageOrConfigFailure.as_i32()
            }
        },
    }
}

fn verbose_flag_present(args: &[std::ffi::OsString]) -> bool {
    args.iter()
        .skip(1)
        .any(|arg| arg.to_string_lossy() == "--verbose")
}

fn append_launch_failure_event(log_path: &std::path::Path, detail: &str) {
    let _ = logging::append_launch_log_event(log_path, "launch-failure", detail);
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
mod tests;
