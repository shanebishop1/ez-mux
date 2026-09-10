use ez_mux::session::SlotMode;
use ez_mux::session::SlotModeLaunchContext;
use ez_mux::session::focus_slot;
use ez_mux::session::mode_launch_contract;
use ez_mux::session::switch_slot_mode;

use super::support::FakeTmux;

#[test]
fn slot_targeted_mode_switch_routes_to_tmux_client() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let outcome = switch_slot_mode(
        "ezm-session-42",
        3,
        SlotMode::Neovim,
        SlotModeLaunchContext::default(),
        &tmux,
    )
    .expect("mode switch should succeed");

    assert_eq!(outcome.session_name, "ezm-session-42");
    assert_eq!(outcome.slot_id, 3);
    assert_eq!(outcome.mode, SlotMode::Neovim);
    assert_eq!(tmux.mode_switches.borrow().len(), 1);
    assert_eq!(
        tmux.mode_switches.borrow()[0],
        (String::from("ezm-session-42"), 3, SlotMode::Neovim)
    );
}

#[test]
fn slot_targeted_mode_switch_surfaces_tmux_failures() {
    let tmux = FakeTmux {
        interactive_attach: true,
        mode_switch_error: std::cell::RefCell::new(Some(String::from("respawn-pane failed"))),
        ..FakeTmux::default()
    };

    let error = switch_slot_mode(
        "ezm-session-77",
        4,
        SlotMode::Agent,
        SlotModeLaunchContext::default(),
        &tmux,
    )
    .expect_err("mode switch should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("respawn-pane failed"));
    assert_eq!(tmux.mode_switches.borrow().len(), 1);
}

#[test]
fn slot_targeted_mode_switch_rejects_non_canonical_slot_id_at_runtime_boundary() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let error = switch_slot_mode(
        "ezm-session-77",
        9,
        SlotMode::Agent,
        SlotModeLaunchContext::default(),
        &tmux,
    )
    .expect_err("mode switch should reject non-canonical slot id");

    let rendered = error.to_string();
    assert!(rendered.contains("outside canonical range 1..5"));
    assert!(tmux.mode_switches.borrow().is_empty());
}

#[test]
fn slot_targeted_focus_routes_to_tmux_client() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let outcome = focus_slot("ezm-session-55", 4, &tmux).expect("focus should succeed");

    assert_eq!(outcome.session_name, "ezm-session-55");
    assert_eq!(outcome.slot_id, 4);
    assert_eq!(
        tmux.focus_calls.borrow().as_slice(),
        &[(String::from("ezm-session-55"), 4)]
    );
}

#[test]
fn slot_targeted_swap_routes_to_tmux_client() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    ez_mux::session::TmuxClient::swap_slot_with_center(&tmux, "ezm-session-66", 1)
        .expect("swap should succeed");

    assert_eq!(
        tmux.swap_calls.borrow().as_slice(),
        &[(String::from("ezm-session-66"), 1)]
    );
}

#[test]
fn slot_targeted_swap_surfaces_tmux_failures() {
    let tmux = FakeTmux {
        interactive_attach: true,
        swap_error: std::cell::RefCell::new(Some(String::from("swap-pane failed"))),
        ..FakeTmux::default()
    };

    let error = ez_mux::session::TmuxClient::swap_slot_with_center(&tmux, "ezm-session-66", 3)
        .expect_err("swap should fail");

    assert!(error.to_string().contains("swap-pane failed"));
    assert_eq!(tmux.swap_calls.borrow().len(), 1);
}

#[test]
fn slot_targeted_focus_rejects_non_canonical_slot_id_at_runtime_boundary() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let error =
        focus_slot("ezm-session-55", 9, &tmux).expect_err("focus should reject slot outside 1..5");

    assert!(error.to_string().contains("outside canonical range 1..5"));
    assert!(tmux.focus_calls.borrow().is_empty());
}

#[test]
fn per_mode_launch_contracts_define_runtime_command_and_hooks() {
    let shell = mode_launch_contract(SlotMode::Shell);
    let agent = mode_launch_contract(SlotMode::Agent);
    let neovim = mode_launch_contract(SlotMode::Neovim);
    let lazygit = mode_launch_contract(SlotMode::Lazygit);

    assert!(shell.launch_command.contains("SHELL"));
    assert!(shell.launch_command.contains("\"${SHELL:-/bin/sh}\""));
    assert!(agent.launch_command.contains("opencode"));
    assert!(neovim.launch_command.contains("nvim"));
    assert!(lazygit.launch_command.contains("lazygit"));
    assert!(!agent.launch_command.contains("|| true"));
    assert!(!neovim.launch_command.contains("|| true"));
    assert!(!lazygit.launch_command.contains("|| true"));
    assert!(
        agent
            .launch_command
            .contains("mode tool opencode exited with status")
    );
    assert!(agent.launch_command.contains("\"${SHELL:-/bin/sh}\""));
    assert_eq!(
        format!("{:?}", shell.tool_failure_policy),
        "ContinueToShell"
    );
    assert_eq!(
        format!("{:?}", agent.tool_failure_policy),
        "ContinueToShell"
    );
    assert_eq!(
        format!("{:?}", neovim.tool_failure_policy),
        "FailModeSwitch"
    );
    assert_eq!(
        format!("{:?}", lazygit.tool_failure_policy),
        "ContinueToShell"
    );
    assert_eq!(shell.teardown_hooks.len(), 0);
    assert_eq!(agent.teardown_hooks.len(), 1);
    assert_eq!(neovim.teardown_hooks.len(), 1);
    assert_eq!(lazygit.teardown_hooks.len(), 1);
    assert!(!lazygit.launch_command.contains("exit \"$exit_code\""));
}
