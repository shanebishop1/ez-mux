use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use crate::config::OperatingSystem;

use super::LOG_FILE_EXTENSION;
use super::LoggingError;

pub trait LogOpener {
    /// Opens a log file using the platform-specific opener.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when process spawn or opener execution fails.
    fn open(&self, os: OperatingSystem, path: &Path) -> io::Result<()>;
}

pub struct ProcessLogOpener;

impl LogOpener for ProcessLogOpener {
    fn open(&self, os: OperatingSystem, path: &Path) -> io::Result<()> {
        let command = match os {
            OperatingSystem::Linux => "xdg-open",
            OperatingSystem::MacOs => "open",
            OperatingSystem::Unsupported => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "unsupported platform for log opening",
                ));
            }
        };

        let status = Command::new(command).arg(path).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "command `{command}` exited with status {status}"
            )))
        }
    }
}

/// Selects and opens the latest log file under `root`.
///
/// # Errors
///
/// Returns [`LoggingError::NoLogFiles`] if no logs exist and
/// [`LoggingError::OpenLogFailed`] if opening fails.
pub fn open_latest_log(
    root: &Path,
    os: OperatingSystem,
    opener: &impl LogOpener,
) -> Result<PathBuf, LoggingError> {
    let latest = latest_log_file(root)?;
    opener
        .open(os, &latest)
        .map_err(|source| LoggingError::OpenLogFailed {
            path: latest.clone(),
            source,
        })?;
    Ok(latest)
}

pub(crate) fn latest_log_file(root: &Path) -> Result<PathBuf, LoggingError> {
    let entries = fs::read_dir(root).map_err(|source| LoggingError::ReadLogRootFailed {
        path: root.to_path_buf(),
        source,
    })?;

    let mut latest_from_name: Option<(u64, String)> = None;
    let mut latest_from_mtime: Option<(SystemTime, String)> = None;

    for entry in entries {
        let entry = entry.map_err(|source| LoggingError::ReadLogRootFailed {
            path: root.to_path_buf(),
            source,
        })?;

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let has_log_extension = path
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension == LOG_FILE_EXTENSION);

        if !has_log_extension {
            continue;
        }

        let Some(file_name) = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .map(str::to_owned)
        else {
            continue;
        };

        if let Some(timestamp) = parse_log_filename_timestamp(&file_name) {
            let should_replace = latest_from_name
                .as_ref()
                .is_none_or(|(current, current_name)| {
                    timestamp > *current || (timestamp == *current && file_name > *current_name)
                });
            if should_replace {
                latest_from_name = Some((timestamp, file_name));
            }
            continue;
        }

        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                let should_replace =
                    latest_from_mtime
                        .as_ref()
                        .is_none_or(|(current, current_name)| {
                            modified > *current
                                || (modified == *current && file_name > *current_name)
                        });
                if should_replace {
                    latest_from_mtime = Some((modified, file_name));
                }
            }
        }
    }

    let selected = latest_from_name
        .map(|(_, name)| name)
        .or_else(|| latest_from_mtime.map(|(_, name)| name));

    let Some(name) = selected else {
        return Err(LoggingError::NoLogFiles {
            root: root.to_path_buf(),
        });
    };

    Ok(root.join(name))
}

fn parse_log_filename_timestamp(file_name: &str) -> Option<u64> {
    let stem = file_name.strip_suffix(".log")?;
    if stem.len() < "YYYYMMDD-HHMMSS-a".len() {
        return None;
    }
    if stem.as_bytes()["YYYYMMDD-HHMMSS".len()] != b'-' {
        return None;
    }

    let timestamp = &stem[.."YYYYMMDD-HHMMSS".len()];
    if timestamp.as_bytes()[8] != b'-' {
        return None;
    }

    let mut digits = String::with_capacity(14);
    for (index, byte) in timestamp.bytes().enumerate() {
        if index == 8 {
            continue;
        }
        if !byte.is_ascii_digit() {
            return None;
        }
        digits.push(char::from(byte));
    }

    digits.parse::<u64>().ok()
}
