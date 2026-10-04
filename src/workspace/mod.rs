//! Generic local workspace composition. Hosts and agent state belong to callers.
mod commands;
mod config;
mod runtime;
mod state;
use crate::app::AppError;
use clap::Subcommand;

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceCommand {
    /// Open or reconnect to a group without replacing running panes.
    Open {
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        slot: Option<u8>,
        #[arg(long)]
        no_attach: bool,
    },
    /// Show local group and slot state.
    Status,
    /// Close terminal views in the selected group or entire workspace.
    Close {
        #[arg(long)]
        group: Option<String>,
    },
}
pub(crate) fn execute(
    file: &std::path::Path,
    command: WorkspaceCommand,
) -> Result<String, AppError> {
    commands::execute(file, command)
}
pub(crate) fn agent(file: &std::path::Path, group: &str, slot: u8) -> Result<String, AppError> {
    commands::agent(file, group, slot)
}
