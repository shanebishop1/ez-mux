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
    let launch_invocation = sanitize_tool_environment(binary_name, launch_invocation);

    format!(
        "if command -v {binary_name} >/dev/null 2>&1; then {launch_invocation}; status=$?; if [ \"$status\" -ne 0 ]; then printf '%s\\n' \"ez-mux mode tool {binary_name} exited with status $status\" >&2; {on_failure}; fi; fi; exec \"${{SHELL:-/bin/sh}}\" -l"
    )
}

fn sanitize_tool_environment(binary_name: &str, launch_invocation: &str) -> String {
    if binary_name != "opencode" {
        return launch_invocation.to_owned();
    }

    format!(
        "unset OPENCODE_SERVER_URL OPENCODE_SERVER_HOST OPENCODE_SERVER_PORT OPENCODE_SERVER_PASSWORD; {launch_invocation}"
    )
}

#[cfg(test)]
mod tests {
    use super::{ModeToolFailurePolicy, launch_tool_command};

    #[test]
    fn opencode_launch_clears_shared_server_environment_overrides() {
        let command = launch_tool_command(
            "opencode",
            "opencode attach 'http://127.0.0.1:4096' --dir '/repo'",
            ModeToolFailurePolicy::ContinueToShell,
        );

        assert!(command.contains("unset OPENCODE_SERVER_URL OPENCODE_SERVER_HOST OPENCODE_SERVER_PORT OPENCODE_SERVER_PASSWORD;"));
        assert!(command.contains("opencode attach 'http://127.0.0.1:4096' --dir '/repo'"));
    }

    #[test]
    fn non_opencode_launch_is_not_modified() {
        let command = launch_tool_command("nvim", "nvim", ModeToolFailurePolicy::FailModeSwitch);

        assert!(!command.contains("unset OPENCODE_SERVER_URL"));
        assert!(command.contains("then nvim;"));
    }
}
