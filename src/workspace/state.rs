use crate::app::AppError;
use fs2::FileExt;
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};
pub(crate) fn error(message: impl Into<String>) -> AppError {
    AppError::Runtime(message.into())
}

pub(crate) fn state_root() -> Result<PathBuf, AppError> {
    let root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .ok_or_else(|| error("HOME or XDG_STATE_HOME is required"))?;
    Ok(root.join("ez-mux"))
}

pub(crate) fn stable_key(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
pub(crate) fn private_directory(path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(|e| error(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|e| error(e.to_string()))?;
    }
    Ok(())
}

pub(crate) fn locked_file(path: &Path) -> Result<fs::File, AppError> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(|e| error(e.to_string()))?;
    file.lock_exclusive().map_err(|e| error(e.to_string()))?;
    Ok(file)
}
