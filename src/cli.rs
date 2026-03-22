use clap::{Parser, Subcommand};

use crate::session::{LayoutPreset, SlotMode};

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(
    name = "ezm",
    bin_name = "ezm",
    version,
    about = "Deterministic tmux workspace orchestrator",
    long_about = None
)]
pub struct Cli {
    #[arg(long, global = true, value_name = "OPERATOR")]
    pub operator: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Repair the current project session.
    Repair,

    /// Log utilities.
    #[command(subcommand)]
    Logs(LogsCommand),

    /// Apply a layout preset to the current project session.
    Preset {
        #[arg(long)]
        preset: LayoutPreset,
    },

    #[command(name = "__internal", hide = true)]
    Internal {
        #[command(subcommand)]
        command: InternalCommand,
    },
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum LogsCommand {
    /// Open the latest log file.
    #[command(name = "open-latest")]
    OpenLatest,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum InternalCommand {
    #[command(name = "swap")]
    Swap {
        #[arg(long)]
        session: String,
        #[arg(long)]
        slot: u8,
    },
    #[command(name = "focus")]
    Focus {
        #[arg(long)]
        session: String,
        #[arg(long)]
        slot: u8,
    },
    #[command(name = "mode")]
    Mode {
        #[arg(long)]
        session: String,
        #[arg(long)]
        slot: u8,
        #[arg(long)]
        mode: SlotMode,
    },
    #[command(name = "popup")]
    Popup {
        #[arg(long)]
        session: String,
        #[arg(long)]
        slot: u8,
        #[arg(long)]
        client: Option<String>,
    },
    #[command(name = "auxiliary")]
    Auxiliary {
        #[arg(long)]
        session: String,
        #[arg(long)]
        action: AuxiliaryAction,
    },
    #[command(name = "teardown")]
    Teardown {
        #[arg(long)]
        session: String,
    },
    #[command(name = "preset")]
    Preset {
        #[arg(long)]
        session: String,
        #[arg(long)]
        preset: LayoutPreset,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
pub enum AuxiliaryAction {
    Open,
    Close,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::session::{LayoutPreset, SlotMode};

    use super::{AuxiliaryAction, Cli, Command, InternalCommand, LogsCommand};

    #[test]
    fn parses_default_invocation() {
        let parsed = Cli::try_parse_from(["ezm"]).expect("parse should succeed");
        assert_eq!(parsed.command, None);
    }

    #[test]
    fn parses_repair_subcommand() {
        let parsed = Cli::try_parse_from(["ezm", "repair"]).expect("parse should succeed");
        assert_eq!(parsed.command, Some(Command::Repair));
    }

    #[test]
    fn parses_logs_open_latest_subcommand() {
        let parsed =
            Cli::try_parse_from(["ezm", "logs", "open-latest"]).expect("parse should succeed");
        assert_eq!(parsed.command, Some(Command::Logs(LogsCommand::OpenLatest)));
    }

    #[test]
    fn parses_preset_subcommand() {
        let parsed = Cli::try_parse_from(["ezm", "preset", "--preset", "three-pane"])
            .expect("parse should succeed");
        assert_eq!(
            parsed.command,
            Some(Command::Preset {
                preset: LayoutPreset::ThreePane,
            })
        );
    }

    #[test]
    fn parses_internal_swap_subcommand() {
        let parsed = Cli::try_parse_from([
            "ezm",
            "__internal",
            "swap",
            "--session",
            "ezm-test-session",
            "--slot",
            "4",
        ])
        .expect("parse should succeed");
        assert_eq!(
            parsed.command,
            Some(Command::Internal {
                command: InternalCommand::Swap {
                    session: String::from("ezm-test-session"),
                    slot: 4,
                },
            })
        );
    }

    #[test]
    fn parses_internal_mode_subcommand() {
        let parsed = Cli::try_parse_from([
            "ezm",
            "__internal",
            "mode",
            "--session",
            "ezm-test-session",
            "--slot",
            "4",
            "--mode",
            "neovim",
        ])
        .expect("parse should succeed");
        assert_eq!(
            parsed.command,
            Some(Command::Internal {
                command: InternalCommand::Mode {
                    session: String::from("ezm-test-session"),
                    slot: 4,
                    mode: SlotMode::Neovim,
                },
            })
        );
    }

    #[test]
    fn parses_internal_focus_subcommand() {
        let parsed = Cli::try_parse_from([
            "ezm",
            "__internal",
            "focus",
            "--session",
            "ezm-test-session",
            "--slot",
            "2",
        ])
        .expect("parse should succeed");
        assert_eq!(
            parsed.command,
            Some(Command::Internal {
                command: InternalCommand::Focus {
                    session: String::from("ezm-test-session"),
                    slot: 2,
                },
            })
        );
    }

    #[test]
    fn parses_internal_popup_subcommand() {
        let parsed = Cli::try_parse_from([
            "ezm",
            "__internal",
            "popup",
            "--session",
            "ezm-test-session",
            "--slot",
            "4",
        ])
        .expect("parse should succeed");
        assert_eq!(
            parsed.command,
            Some(Command::Internal {
                command: InternalCommand::Popup {
                    session: String::from("ezm-test-session"),
                    slot: 4,
                    client: None,
                },
            })
        );
    }

    #[test]
    fn parses_internal_popup_subcommand_with_client_target() {
        let parsed = Cli::try_parse_from([
            "ezm",
            "__internal",
            "popup",
            "--session",
            "ezm-test-session",
            "--slot",
            "4",
            "--client",
            "/dev/pts/10",
        ])
        .expect("parse should succeed");
        assert_eq!(
            parsed.command,
            Some(Command::Internal {
                command: InternalCommand::Popup {
                    session: String::from("ezm-test-session"),
                    slot: 4,
                    client: Some(String::from("/dev/pts/10")),
                },
            })
        );
    }

    #[test]
    fn parses_internal_auxiliary_subcommand() {
        let parsed = Cli::try_parse_from([
            "ezm",
            "__internal",
            "auxiliary",
            "--session",
            "ezm-test-session",
            "--action",
            "open",
        ])
        .expect("parse should succeed");
        assert_eq!(
            parsed.command,
            Some(Command::Internal {
                command: InternalCommand::Auxiliary {
                    session: String::from("ezm-test-session"),
                    action: AuxiliaryAction::Open,
                },
            })
        );
    }

    #[test]
    fn parses_internal_teardown_subcommand() {
        let parsed = Cli::try_parse_from([
            "ezm",
            "__internal",
            "teardown",
            "--session",
            "ezm-test-session",
        ])
        .expect("parse should succeed");
        assert_eq!(
            parsed.command,
            Some(Command::Internal {
                command: InternalCommand::Teardown {
                    session: String::from("ezm-test-session"),
                },
            })
        );
    }

    #[test]
    fn parses_internal_preset_subcommand() {
        let parsed = Cli::try_parse_from([
            "ezm",
            "__internal",
            "preset",
            "--session",
            "ezm-test-session",
            "--preset",
            "three-pane",
        ])
        .expect("parse should succeed");
        assert_eq!(
            parsed.command,
            Some(Command::Internal {
                command: InternalCommand::Preset {
                    session: String::from("ezm-test-session"),
                    preset: LayoutPreset::ThreePane,
                },
            })
        );
    }

    #[test]
    fn supports_help_flag() {
        let err =
            Cli::try_parse_from(["ezm", "--help"]).expect_err("help exits through clap error");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn supports_version_flag() {
        let err = Cli::try_parse_from(["ezm", "--version"])
            .expect_err("version exits through clap error");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayVersion);
    }
}
