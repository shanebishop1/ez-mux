use std::io;
use std::path::PathBuf;

use thiserror::Error;

use super::slot_registry::SlotRegistryError;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("failed resolving current working directory: {0}")]
    CurrentDir(#[source] io::Error),
    #[error("failed canonicalizing project path {path}: {source}")]
    CanonicalizeProjectPath {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid remote path `{prefix}`: expected absolute path")]
    InvalidRemotePathMappingPrefix { prefix: String },
    #[error("agent mode requires shared-server attach configuration")]
    MissingSharedServerAttachConfig,
    #[error("tmux command `{command}` failed: {stderr}")]
    TmuxCommandFailed { command: String, stderr: String },
    #[error("failed spawning tmux command `{command}`: {source}")]
    TmuxSpawnFailed {
        command: String,
        #[source]
        source: io::Error,
    },
    #[error("failed registering SIGINT handler: {source}")]
    SignalRegistrationFailed {
        #[source]
        source: io::Error,
    },
    #[error("interrupted")]
    Interrupted,
    #[error(transparent)]
    SlotRegistry(#[from] SlotRegistryError),
}
