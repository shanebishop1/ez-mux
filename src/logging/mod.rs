mod error;
mod launch;
mod open;
mod pathing;

pub use error::LoggingError;
pub use launch::{
    Clock, LaunchLog, RunIdSource, SystemClock, SystemRunIdSource, initialize_launch_log,
    initialize_launch_log_with_defaults,
};
pub use open::{LogOpener, ProcessLogOpener, open_latest_log};
pub use pathing::{fallback_log_root, resolve_primary_log_root};

pub(crate) const LOG_FILE_EXTENSION: &str = "log";

#[cfg(test)]
mod tests;
