#![cfg_attr(not(test), deny(clippy::unwrap_used))]
//! Project orchestration. The layout executable has no dependency on this module.
use clap::{Parser, Subcommand};
use ez_mux::{app, config, session};
mod opencode;
mod process;
mod workspace;

#[derive(Parser)]
#[command(
    name = "remote-agents",
    version,
    about = "Project, host and agent lifecycle orchestration using ezm layouts"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    #[command(flatten)]
    Workspace(workspace::WorkspaceCommand),
    #[command(name = "__internal", hide = true)]
    Internal {
        #[command(subcommand)]
        command: Internal,
    },
}
#[derive(Subcommand)]
enum Internal {
    WorkspaceAgent {
        #[arg(long)]
        project: String,
        #[arg(long)]
        group: String,
        #[arg(long)]
        slot: u8,
    },
    Opencode {
        #[arg(long)]
        server: String,
        #[arg(long)]
        directory: String,
        #[arg(long)]
        key: String,
        #[arg(long)]
        new: bool,
        #[arg(long)]
        prepare_only: bool,
        #[arg(long)]
        adopt: Option<String>,
        #[arg(long)]
        require_existing: bool,
    },
}
fn execute(command: Command) -> Result<String, app::AppError> {
    match command {
        Command::Workspace(command) => workspace::execute(command),
        Command::Internal {
            command:
                Internal::WorkspaceAgent {
                    project,
                    group,
                    slot,
                },
        } => workspace::agent(&project, &group, slot),
        Command::Internal {
            command:
                Internal::Opencode {
                    server,
                    directory,
                    key,
                    new,
                    prepare_only,
                    adopt,
                    require_existing,
                },
        } => {
            if let Some(id) = adopt {
                return opencode::adopt(&server, &directory, &key, &id);
            }
            if require_existing && !new {
                opencode::require_saved(&server, &key)?;
            }
            if prepare_only {
                opencode::session(&server, &directory, &key, new)
            } else {
                opencode::connect(&server, &directory, &key, new)
            }
        }
    }
}
fn main() {
    match execute(Cli::parse().command) {
        Ok(message) => {
            if !message.is_empty() {
                println!("{message}");
            }
        }
        Err(error) => {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
    }
}
