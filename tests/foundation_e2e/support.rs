use std::path::{Path, PathBuf};

use crate::support::foundation_harness::{CmdOutput, FoundationHarness, TmuxSettleEvidence};

use super::evidence::{CommandSample, SettleEvidence};

pub(crate) fn sample(args: &[&str], output: &CmdOutput) -> CommandSample {
    CommandSample {
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        exit_code: output.exit_code,
        stdout: output.stdout.clone(),
        stderr: output.stderr.clone(),
    }
}

pub(crate) fn map_settle(settle: TmuxSettleEvidence) -> SettleEvidence {
    SettleEvidence {
        attempts: settle.attempts,
        poll_interval_ms: settle.poll_interval_ms,
        timeout_ms: settle.timeout_ms,
        stable: settle.stable,
        sessions: settle.sessions,
        windows: settle.windows,
        panes: settle.panes,
    }
}

pub(crate) fn extract_session_name(output: &str) -> Option<String> {
    let value = output.split("session=").nth(1)?;
    Some(value.split(';').next()?.trim().to_owned())
}

pub(crate) fn extract_stdout_field(stdout: &str, field: &str) -> Option<String> {
    stdout
        .lines()
        .find_map(|line| line.split(&format!("{field}=")).nth(1))
        .and_then(|tail| tail.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(crate) fn extract_remote_path_field(stdout: &str) -> Option<String> {
    extract_stdout_field(stdout, "remote_path")
}

pub(crate) fn extract_active_log_path(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("active log file: "))
        .map(str::to_owned)
}

pub(crate) fn expected_safe_log_root(harness: &FoundationHarness) -> PathBuf {
    match std::env::consts::OS {
        "linux" => harness.work_dir().join("state").join("ez-mux").join("logs"),
        "macos" => harness
            .work_dir()
            .join("home")
            .join("Library")
            .join("Logs")
            .join("ez-mux"),
        os => panic!("E2E-15 unsupported host OS for log root assertion: {os}"),
    }
}

pub(crate) fn has_expected_log_name_shape(name: &str) -> bool {
    if !Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
    {
        return false;
    }

    let base = &name[..name.len() - 4];
    if base.len() < 17 {
        return false;
    }

    let timestamp = &base[..15];
    if timestamp.as_bytes().get(8) != Some(&b'-') {
        return false;
    }

    for (index, byte) in timestamp.as_bytes().iter().enumerate() {
        if index == 8 {
            continue;
        }
        if !byte.is_ascii_digit() {
            return false;
        }
    }

    base.as_bytes().get(15) == Some(&b'-')
}

pub(crate) fn extract_remote_path_source(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find_map(|line| line.split("remote_path_source=").nth(1))
        .and_then(|tail| tail.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
