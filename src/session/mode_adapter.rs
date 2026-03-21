use super::SlotMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TeardownHook {
    SendCtrlC,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeToolFailurePolicy {
    FailModeSwitch,
    ContinueToShell,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeLaunchContract {
    pub mode: SlotMode,
    pub launch_command: String,
    pub teardown_hooks: Vec<TeardownHook>,
    pub tool_failure_policy: ModeToolFailurePolicy,
}

#[must_use]
pub fn mode_launch_contract(mode: SlotMode) -> ModeLaunchContract {
    match mode {
        SlotMode::Shell => ModeLaunchContract {
            mode,
            launch_command: String::from("exec \"${SHELL:-/bin/sh}\" -l"),
            teardown_hooks: Vec::new(),
            tool_failure_policy: ModeToolFailurePolicy::ContinueToShell,
        },
        SlotMode::Agent => ModeLaunchContract {
            mode,
            launch_command: launch_tool_command(
                "opencode",
                "opencode",
                ModeToolFailurePolicy::ContinueToShell,
            ),
            teardown_hooks: vec![TeardownHook::SendCtrlC],
            tool_failure_policy: ModeToolFailurePolicy::ContinueToShell,
        },
        SlotMode::Neovim => ModeLaunchContract {
            mode,
            launch_command: launch_tool_command(
                "nvim",
                "nvim",
                ModeToolFailurePolicy::FailModeSwitch,
            ),
            teardown_hooks: vec![TeardownHook::SendCtrlC],
            tool_failure_policy: ModeToolFailurePolicy::FailModeSwitch,
        },
        SlotMode::Lazygit => ModeLaunchContract {
            mode,
            launch_command: launch_tool_command(
                "lazygit",
                "lazygit",
                ModeToolFailurePolicy::FailModeSwitch,
            ),
            teardown_hooks: vec![TeardownHook::SendCtrlC],
            tool_failure_policy: ModeToolFailurePolicy::FailModeSwitch,
        },
    }
}

pub(super) fn launch_tool_command(
    binary_name: &str,
    launch_invocation: &str,
    policy: ModeToolFailurePolicy,
) -> String {
    let on_failure = match policy {
        ModeToolFailurePolicy::FailModeSwitch => String::from("exit \"$status\""),
        ModeToolFailurePolicy::ContinueToShell => String::from(":"),
    };

    format!(
        "if command -v {binary_name} >/dev/null 2>&1; then {launch_invocation}; status=$?; if [ \"$status\" -ne 0 ]; then printf '%s\\n' \"ez-mux mode tool {binary_name} exited with status $status\" >&2; {on_failure}; fi; fi; exec \"${{SHELL:-/bin/sh}}\" -l"
    )
}
