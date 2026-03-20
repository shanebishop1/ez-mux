use std::io;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("unable to resolve default log root for {os}: HOME is not set")]
    MissingHome { os: &'static str },
    #[error("unsupported platform for log behavior: {os}")]
    UnsupportedPlatform { os: &'static str },
    #[error("failed creating log directory at {path}: {source}")]
    CreateDirFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed creating launch log file at {path}: {source}")]
    CreateLogFileFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed formatting launch timestamp: {0}")]
    TimestampFormat(time::error::Format),
    #[error("failed reading log root at {path}: {source}")]
    ReadLogRootFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("no log files found under {root}")]
    NoLogFiles { root: PathBuf },
    #[error("failed opening log file at {path}: {source}")]
    OpenLogFailed {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}
