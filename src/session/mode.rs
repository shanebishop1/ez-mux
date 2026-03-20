use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum SlotMode {
    Agent,
    Shell,
    Neovim,
    Lazygit,
}

impl SlotMode {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Shell => "shell",
            Self::Neovim => "neovim",
            Self::Lazygit => "lazygit",
        }
    }
}
