mod error;
mod launch;
mod open;
mod pathing;

pub use error::LoggingError;
pub use launch::Clock;
pub use launch::LaunchLog;
pub use launch::RunIdSource;
pub use launch::SystemClock;
pub use launch::SystemRunIdSource;
pub use launch::initialize_launch_log;
pub use launch::initialize_launch_log_with_defaults;
pub use open::LogOpener;
pub use open::ProcessLogOpener;
pub use open::open_latest_log;
pub use pathing::{fallback_log_root, resolve_primary_log_root};

pub(crate) const LOG_FILE_EXTENSION: &str = "log";

#[cfg(test)]
mod tests;
