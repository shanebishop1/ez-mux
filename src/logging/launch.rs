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
    fn now(&self) -> SystemTime;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
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

        format!("{:08x}-{nanos:032x}-{count:016x}", std::process::id())
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
    let fallback_base = default_fallback_base(env);
    initialize_launch_log(env, os, &SystemClock, &SystemRunIdSource, &fallback_base)
}

/// Appends one launch lifecycle event to an active launch log file.
///
/// # Errors
///
/// Returns [`LoggingError`] when the log file cannot be opened for append or
/// when writing the event line fails.
pub fn append_launch_log_event(
    file_path: &Path,
    event: &str,
    detail: &str,
) -> Result<(), LoggingError> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(file_path)
        .map_err(|source| LoggingError::OpenLogFailed {
            path: file_path.to_path_buf(),
            source,
        })?;

    writeln!(file, "event={event}; detail={}", escape_log_detail(detail)).map_err(|source| {
        LoggingError::WriteLogFileFailed {
            path: file_path.to_path_buf(),
            source,
        }
    })
}

fn default_fallback_base(env: &impl EnvProvider) -> PathBuf {
    if let Some(home) = env.get_var("HOME") {
        let trimmed = home.trim().to_owned();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(".ez-mux-fallback");
        }
    }
    std::env::temp_dir().join("ez-mux-fallback")
}

fn create_unique_log_file(
    root: &Path,
    clock: &impl Clock,
    run_id_source: &impl RunIdSource,
) -> Result<PathBuf, LoggingError> {
    for _ in 0..8 {
        let name = log_filename(clock, run_id_source);
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
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {}
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

fn log_filename(clock: &impl Clock, run_id_source: &impl RunIdSource) -> String {
    let timestamp = filename_timestamp(clock.now());
    let run_id = safe_run_id(&run_id_source.next_run_id());
    format!("{timestamp}-{run_id}.{LOG_FILE_EXTENSION}")
}

fn filename_timestamp(now: SystemTime) -> String {
    let elapsed = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = elapsed.as_secs();
    let second_of_day = seconds % 86_400;
    let (year, month, day) = civil_date(seconds / 86_400);
    let hour = second_of_day / 3_600;
    let minute = second_of_day % 3_600 / 60;
    let second = second_of_day % 60;
    let nanos = elapsed.subsec_nanos();

    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}-{nanos:09}")
}

// Converts non-negative days since 1970-01-01 to a proleptic Gregorian date.
fn civil_date(days_since_epoch: u64) -> (u64, u64, u64) {
    let shifted_days = days_since_epoch + 719_468;
    let era = shifted_days / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let march_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * march_month + 2) / 5 + 1;
    let month = if march_month < 10 {
        march_month + 3
    } else {
        march_month - 9
    };
    year += u64::from(month <= 2);
    (year, month, day)
}

fn safe_run_id(run_id: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut safe = String::with_capacity(run_id.len().min(64));

    for byte in run_id.bytes().take(64) {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            safe.push(char::from(byte));
        } else {
            safe.push('_');
            safe.push(char::from(HEX[usize::from(byte >> 4)]));
            safe.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }

    if safe.is_empty() {
        safe.push_str("run");
    }
    safe
}

fn escape_log_detail(detail: &str) -> String {
    detail.replace('\n', "\\n")
}
