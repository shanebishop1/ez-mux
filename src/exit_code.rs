use crate::app::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Success = 0,
    RuntimeFailure = 1,
    UsageOrConfigFailure = 2,
    Interrupt = 130,
}

impl ExitCode {
    #[must_use]
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    #[must_use]
    pub fn from_app_error(error: &AppError) -> Self {
        match error {
            AppError::Config(_) => Self::UsageOrConfigFailure,
            AppError::Logging(_) | AppError::Runtime(_) => Self::RuntimeFailure,
            AppError::Interrupted => Self::Interrupt,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::AppError;
    use crate::config::ConfigError;
    use crate::logging::LoggingError;

    use super::ExitCode;

    #[test]
    fn success_code_is_zero() {
        assert_eq!(ExitCode::Success.as_i32(), 0);
    }

    #[test]
    fn config_errors_map_to_usage_code() {
        let error = AppError::Config(ConfigError::MissingHome { os: "Linux" });
        assert_eq!(
            ExitCode::from_app_error(&error),
            ExitCode::UsageOrConfigFailure
        );
    }

    #[test]
    fn runtime_errors_map_to_runtime_code() {
        let error = AppError::Runtime(String::from("boom"));
        assert_eq!(ExitCode::from_app_error(&error), ExitCode::RuntimeFailure);
    }

    #[test]
    fn logging_errors_map_to_runtime_code() {
        let error = AppError::Logging(LoggingError::NoLogFiles {
            root: std::path::PathBuf::from("/tmp/logs"),
        });
        assert_eq!(ExitCode::from_app_error(&error), ExitCode::RuntimeFailure);
    }

    #[test]
    fn interrupt_errors_map_to_interrupt_code() {
        let error = AppError::Interrupted;
        assert_eq!(ExitCode::from_app_error(&error), ExitCode::Interrupt);
    }
}
