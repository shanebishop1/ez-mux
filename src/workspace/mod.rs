//! Named project/group lifecycle. Legacy cwd-based entrypoints remain available.
mod backend;
mod commands;
mod config;
mod runtime;
use crate::app::AppError;
use clap::Subcommand;

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum WorkspaceCommand {
    /// Ensure a project/group exists and reconnect without creating a conversation.
    Open {
        project: String,
        #[arg(long)]
        group: Option<String>,
        #[arg(long)]
        slot: Option<u8>,
        #[arg(long)]
        no_attach: bool,
        #[arg(long, hide = true)]
        local: bool,
    },
    /// Start/check only the configured backend; do not create terminal views.
    Start {
        project: String,
        #[arg(long, hide = true)]
        local: bool,
    },
    /// Attach directly to a slot conversation on its execution host.
    Attach {
        project: String,
        #[arg(long)]
        group: Option<String>,
        #[arg(long, default_value = "1")]
        slot: u8,
        #[arg(long, hide = true)]
        local: bool,
    },
    /// Start a new conversation explicitly, retaining prior history.
    New {
        project: String,
        #[arg(long)]
        group: Option<String>,
        #[arg(long, default_value = "1")]
        slot: u8,
        #[arg(long, hide = true)]
        local: bool,
    },
    /// Map an existing conversation without changing a running terminal.
    Adopt {
        project: String,
        #[arg(long)]
        group: Option<String>,
        #[arg(long, default_value = "1")]
        slot: u8,
        #[arg(long)]
        session: String,
        #[arg(long, hide = true)]
        local: bool,
    },
    /// Show project, service, group and slot state.
    Status {
        project: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long, hide = true)]
        local: bool,
    },
    /// Check configuration, service and authenticated API readiness.
    Doctor {
        project: String,
        #[arg(long, hide = true)]
        local: bool,
    },
    /// Close terminal views; retain backend and saved conversations.
    Close {
        project: String,
        #[arg(long)]
        group: Option<String>,
        #[arg(long, hide = true)]
        local: bool,
    },
    /// Close project views and stop its configured backend service.
    Stop {
        project: String,
        #[arg(long, hide = true)]
        local: bool,
    },
}
pub(crate) fn execute(command: WorkspaceCommand) -> Result<String, AppError> {
    commands::execute(command)
}
pub(crate) fn agent(project: &str, group: &str, slot: u8) -> Result<String, AppError> {
    commands::agent(project, group, slot)
}
