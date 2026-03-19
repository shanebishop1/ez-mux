use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    let mut latest_name: Option<String> = None;

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

        let should_replace = latest_name
            .as_ref()
            .is_none_or(|current| file_name > *current);
        if should_replace {
            latest_name = Some(file_name);
        }
    }

    let Some(name) = latest_name else {
        return Err(LoggingError::NoLogFiles {
            root: root.to_path_buf(),
        });
    };

    Ok(root.join(name))
}
