use super::SlotMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeardownHook {
    SendCtrlC,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeLaunchContract {
    pub mode: SlotMode,
    pub launch_command: String,
    pub teardown_hooks: Vec<TeardownHook>,
}

#[must_use]
pub fn mode_launch_contract(mode: SlotMode) -> ModeLaunchContract {
    match mode {
        SlotMode::Shell => ModeLaunchContract {
            mode,
            launch_command: String::from("exec ${SHELL:-/bin/sh} -l"),
            teardown_hooks: Vec::new(),
        },
        SlotMode::Agent => ModeLaunchContract {
            mode,
            launch_command: String::from(
                "if command -v opencode >/dev/null 2>&1; then opencode || true; fi; exec ${SHELL:-/bin/sh} -l",
            ),
            teardown_hooks: vec![TeardownHook::SendCtrlC],
        },
        SlotMode::Neovim => ModeLaunchContract {
            mode,
            launch_command: String::from(
                "if command -v nvim >/dev/null 2>&1; then nvim || true; fi; exec ${SHELL:-/bin/sh} -l",
            ),
            teardown_hooks: vec![TeardownHook::SendCtrlC],
        },
        SlotMode::Lazygit => ModeLaunchContract {
            mode,
            launch_command: String::from(
                "if command -v lazygit >/dev/null 2>&1; then lazygit || true; fi; exec ${SHELL:-/bin/sh} -l",
            ),
            teardown_hooks: vec![TeardownHook::SendCtrlC],
        },
    }
}
