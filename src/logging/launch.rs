use std::fs::{self, OpenOptions};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::{EnvProvider, OperatingSystem};

use super::LOG_FILE_EXTENSION;
use super::LoggingError;
use super::fallback_log_root;
use super::resolve_primary_log_root;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchLog {
    pub root: PathBuf,
    pub file_path: PathBuf,
    pub warning: Option<String>,
}

pub trait Clock {
    fn now_utc(&self) -> time::OffsetDateTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_utc(&self) -> time::OffsetDateTime {
        time::OffsetDateTime::now_utc()
    }
}

pub trait RunIdSource {
    fn next_run_id(&self) -> String;
}

pub struct SystemRunIdSource;

impl RunIdSource for SystemRunIdSource {
    fn next_run_id(&self) -> String {
        static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let count = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);

        format!("{:x}{nanos:x}{count:x}", std::process::id())
    }
}

/// Initializes launch logging for one `ezm` invocation.
///
/// This creates a fresh per-launch log file in the primary log root, and if that
/// path cannot be created, falls back to a deterministic safe directory.
///
/// # Errors
///
/// Returns an error if both primary and fallback directory setup fail, or if the
/// launch log file itself cannot be created.
pub fn initialize_launch_log(
    env: &impl EnvProvider,
    os: OperatingSystem,
    clock: &impl Clock,
    run_id_source: &impl RunIdSource,
    fallback_base: &Path,
) -> Result<LaunchLog, LoggingError> {
    let primary_root = resolve_primary_log_root(env, os);
    let fallback_root = fallback_log_root(fallback_base);

    let (active_root, warning) = match primary_root {
        Ok(primary_root) => match fs::create_dir_all(&primary_root) {
            Ok(()) => (primary_root, None),
            Err(source) => {
                fs::create_dir_all(&fallback_root).map_err(|fallback_source| {
                    LoggingError::CreateDirFailed {
                        path: fallback_root.clone(),
                        source: fallback_source,
                    }
                })?;

                (
                    fallback_root.clone(),
                    Some(format!(
                        "failed to create primary log root {}: {source}; using fallback {}",
                        primary_root.display(),
                        fallback_root.display()
                    )),
                )
            }
        },
        Err(error) => {
            fs::create_dir_all(&fallback_root).map_err(|source| LoggingError::CreateDirFailed {
                path: fallback_root.clone(),
                source,
            })?;

            (
                fallback_root.clone(),
                Some(format!(
                    "failed to resolve primary log root: {error}; using fallback {}",
                    fallback_root.display()
                )),
            )
        }
    };

    let file_path = create_unique_log_file(&active_root, clock, run_id_source)?;

    Ok(LaunchLog {
        root: active_root,
        file_path,
        warning,
    })
}

/// Initializes launch logging using process defaults.
///
/// # Errors
///
/// Returns the same errors as [`initialize_launch_log`].
pub fn initialize_launch_log_with_defaults(
    env: &impl EnvProvider,
    os: OperatingSystem,
) -> Result<LaunchLog, LoggingError> {
    initialize_launch_log(
        env,
        os,
        &SystemClock,
        &SystemRunIdSource,
        &std::env::temp_dir(),
    )
}

fn create_unique_log_file(
    root: &Path,
    clock: &impl Clock,
    run_id_source: &impl RunIdSource,
) -> Result<PathBuf, LoggingError> {
    for _ in 0..8 {
        let name = log_filename(clock, run_id_source)?;
        let path = root.join(name);

        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(mut file) => {
                writeln!(file, "event=launch-log-created").map_err(|source| {
                    LoggingError::CreateLogFileFailed {
                        path: path.clone(),
                        source,
                    }
                })?;
                return Ok(path);
            }
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(LoggingError::CreateLogFileFailed { path, source }),
        }
    }

    let exhausted_path = root.join("exhausted-run-id-space.log");
    Err(LoggingError::CreateLogFileFailed {
        path: exhausted_path,
        source: io::Error::new(
            io::ErrorKind::AlreadyExists,
            "failed to create unique log filename after retries",
        ),
    })
}

fn log_filename(
    clock: &impl Clock,
    run_id_source: &impl RunIdSource,
) -> Result<String, LoggingError> {
    let timestamp = clock
        .now_utc()
        .format(&time::macros::format_description!(
            "[year][month][day]-[hour][minute][second]"
        ))
        .map_err(LoggingError::TimestampFormat)?;

    Ok(format!(
        "{timestamp}-{}.{}",
        run_id_source.next_run_id(),
        LOG_FILE_EXTENSION
    ))
}
