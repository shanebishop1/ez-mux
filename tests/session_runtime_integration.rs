use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ez_mux::session::SessionAction;
use ez_mux::session::SlotMode;
use ez_mux::session::TmuxClient;
use ez_mux::session::auxiliary_viewer;
use ez_mux::session::ensure_project_session;
use ez_mux::session::mode_launch_contract;
use ez_mux::session::resolve_session_identity;
use ez_mux::session::switch_slot_mode;
use ez_mux::session::toggle_popup_shell;

#[derive(Default)]
struct FakeTmux {
    sessions: RefCell<HashSet<String>>,
    created: RefCell<Vec<(String, PathBuf)>>,
    attached: RefCell<Vec<String>>,
    mode_switches: RefCell<Vec<(String, u8, SlotMode)>>,
    mode_switch_error: RefCell<Option<String>>,
    popup_toggles: RefCell<Vec<(String, u8)>>,
    popup_toggle_error: RefCell<Option<String>>,
    popup_toggle_open: RefCell<bool>,
    auxiliary_calls: RefCell<Vec<(String, bool)>>,
    auxiliary_error: RefCell<Option<String>>,
    auxiliary_exists: RefCell<bool>,
    skipped_non_interactive_attach: RefCell<u32>,
    interactive_attach: bool,
}

impl TmuxClient for FakeTmux {
    fn session_exists(&self, session_name: &str) -> Result<bool, ez_mux::session::SessionError> {
        Ok(self.sessions.borrow().contains(session_name))
    }

    fn create_detached_session(
        &self,
        session_name: &str,
        cwd: &Path,
    ) -> Result<(), ez_mux::session::SessionError> {
        self.sessions.borrow_mut().insert(session_name.to_string());
        self.created
            .borrow_mut()
            .push((session_name.to_string(), cwd.to_path_buf()));
        Ok(())
    }

    fn attach_session(&self, session_name: &str) -> Result<(), ez_mux::session::SessionError> {
        self.attached.borrow_mut().push(session_name.to_string());
        if !self.interactive_attach {
            *self.skipped_non_interactive_attach.borrow_mut() += 1;
        }

        Ok(())
    }

    fn switch_slot_mode(
        &self,
        session_name: &str,
        slot_id: u8,
        mode: SlotMode,
    ) -> Result<(), ez_mux::session::SessionError> {
        self.mode_switches
            .borrow_mut()
            .push((session_name.to_string(), slot_id, mode));

        if let Some(stderr) = self.mode_switch_error.borrow().as_ref() {
            return Err(ez_mux::session::SessionError::TmuxCommandFailed {
                command: String::from("__internal mode"),
                stderr: stderr.clone(),
            });
        }

        Ok(())
    }

    fn validate_session_invariants(
        &self,
        _session_name: &str,
    ) -> Result<(), ez_mux::session::SessionError> {
        Ok(())
    }

    fn bootstrap_default_layout(
        &self,
        _session_name: &str,
        _project_dir: &Path,
    ) -> Result<(), ez_mux::session::SessionError> {
        Ok(())
    }

    fn swap_slot_with_center(
        &self,
        _session_name: &str,
        _slot_id: u8,
    ) -> Result<(), ez_mux::session::SessionError> {
        Ok(())
    }

    fn toggle_popup_shell(
        &self,
        session_name: &str,
        slot_id: u8,
    ) -> Result<ez_mux::session::PopupShellOutcome, ez_mux::session::SessionError> {
        self.popup_toggles
            .borrow_mut()
            .push((session_name.to_string(), slot_id));

        if let Some(stderr) = self.popup_toggle_error.borrow().as_ref() {
            return Err(ez_mux::session::SessionError::TmuxCommandFailed {
                command: String::from("__internal popup"),
                stderr: stderr.clone(),
            });
        }

        let was_open = *self.popup_toggle_open.borrow();
        *self.popup_toggle_open.borrow_mut() = !was_open;

        Ok(ez_mux::session::PopupShellOutcome {
            session_name: session_name.to_owned(),
            slot_id,
            action: if was_open {
                ez_mux::session::PopupShellAction::Closed
            } else {
                ez_mux::session::PopupShellAction::Opened
            },
            cwd: String::from("/tmp/popup-cwd"),
            width_pct: 70,
            height_pct: 70,
        })
    }

    fn auxiliary_viewer(
        &self,
        session_name: &str,
        open: bool,
    ) -> Result<ez_mux::session::AuxiliaryViewerOutcome, ez_mux::session::SessionError> {
        self.auxiliary_calls
            .borrow_mut()
            .push((session_name.to_string(), open));

        if let Some(stderr) = self.auxiliary_error.borrow().as_ref() {
            return Err(ez_mux::session::SessionError::TmuxCommandFailed {
                command: String::from("__internal auxiliary"),
                stderr: stderr.clone(),
            });
        }

        let action = if open {
            let existed = *self.auxiliary_exists.borrow();
            *self.auxiliary_exists.borrow_mut() = true;
            if existed {
                ez_mux::session::AuxiliaryViewerAction::Reused
            } else {
                ez_mux::session::AuxiliaryViewerAction::Created
            }
        } else {
            *self.auxiliary_exists.borrow_mut() = false;
            ez_mux::session::AuxiliaryViewerAction::Closed
        };

        Ok(ez_mux::session::AuxiliaryViewerOutcome {
            session_name: session_name.to_owned(),
            action,
            window_name: String::from("beads-viewer"),
            window_id: Some(String::from("@9")),
        })
    }
}

#[test]
fn resolver_is_deterministic_for_same_project_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path().join("My Project@2026");
    std::fs::create_dir_all(&project_dir).expect("create project dir");

    let first = resolve_session_identity(&project_dir).expect("resolve first");
    let second = resolve_session_identity(&project_dir).expect("resolve second");

    assert_eq!(first.project_key, second.project_key);
    assert_eq!(first.session_name, second.session_name);
    assert!(first.session_name.starts_with("ezm-"));
    assert!(!first.session_name.contains(' '));
}

#[test]
fn resolver_distinguishes_between_different_projects() {
    let temp = tempfile::tempdir().expect("tempdir");
    let first_project = temp.path().join("first");
    let second_project = temp.path().join("second");
    std::fs::create_dir_all(&first_project).expect("create first");
    std::fs::create_dir_all(&second_project).expect("create second");

    let first = resolve_session_identity(&first_project).expect("resolve first");
    let second = resolve_session_identity(&second_project).expect("resolve second");

    assert_ne!(first.project_key, second.project_key);
    assert_ne!(first.session_name, second.session_name);
}

#[test]
fn runtime_creates_first_then_attaches_second_without_duplicate_create() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = ensure_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(first.identity.session_name, second.identity.session_name);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow()[0], second.identity.session_name);
    assert_eq!(*tmux.skipped_non_interactive_attach.borrow(), 0);
}

#[test]
fn runtime_attach_path_is_non_interactive_safe() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: false,
        ..FakeTmux::default()
    };

    let first = ensure_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 1);
    assert_eq!(*tmux.skipped_non_interactive_attach.borrow(), 1);
}

#[test]
fn slot_targeted_mode_switch_routes_to_tmux_client() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let outcome = switch_slot_mode("ezm-session-42", 3, SlotMode::Neovim, &tmux)
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
        mode_switch_error: RefCell::new(Some(String::from("respawn-pane failed"))),
        ..FakeTmux::default()
    };

    let error = switch_slot_mode("ezm-session-77", 4, SlotMode::Agent, &tmux)
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

    let error = switch_slot_mode("ezm-session-77", 9, SlotMode::Agent, &tmux)
        .expect_err("mode switch should reject non-canonical slot id");

    let rendered = error.to_string();
    assert!(rendered.contains("outside canonical range 1..5"));
    assert!(tmux.mode_switches.borrow().is_empty());
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
    assert_eq!(shell.teardown_hooks.len(), 0);
    assert_eq!(agent.teardown_hooks.len(), 1);
    assert_eq!(neovim.teardown_hooks.len(), 1);
    assert_eq!(lazygit.teardown_hooks.len(), 1);
}

#[test]
fn popup_toggle_routes_to_tmux_client_and_toggles_open_then_close() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = toggle_popup_shell("ezm-session-88", 2, &tmux).expect("first toggle");
    let second = toggle_popup_shell("ezm-session-88", 2, &tmux).expect("second toggle");

    assert_eq!(first.action, ez_mux::session::PopupShellAction::Opened);
    assert_eq!(second.action, ez_mux::session::PopupShellAction::Closed);
    assert_eq!(first.width_pct, 70);
    assert_eq!(first.height_pct, 70);
    assert_eq!(
        tmux.popup_toggles.borrow().as_slice(),
        &[
            (String::from("ezm-session-88"), 2),
            (String::from("ezm-session-88"), 2)
        ]
    );
}

#[test]
fn popup_toggle_surfaces_tmux_failures() {
    let tmux = FakeTmux {
        interactive_attach: true,
        popup_toggle_error: RefCell::new(Some(String::from("display-popup failed"))),
        ..FakeTmux::default()
    };

    let error = toggle_popup_shell("ezm-session-88", 2, &tmux).expect_err("popup should fail");

    assert!(error.to_string().contains("display-popup failed"));
    assert_eq!(tmux.popup_toggles.borrow().len(), 1);
}

#[test]
fn auxiliary_viewer_create_reuse_close_is_deterministic() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let created = auxiliary_viewer("ezm-session-91", true, &tmux).expect("create");
    let reused = auxiliary_viewer("ezm-session-91", true, &tmux).expect("reuse");
    let closed = auxiliary_viewer("ezm-session-91", false, &tmux).expect("close");

    assert_eq!(
        created.action,
        ez_mux::session::AuxiliaryViewerAction::Created
    );
    assert_eq!(
        reused.action,
        ez_mux::session::AuxiliaryViewerAction::Reused
    );
    assert_eq!(
        closed.action,
        ez_mux::session::AuxiliaryViewerAction::Closed
    );
    assert_eq!(
        tmux.auxiliary_calls.borrow().as_slice(),
        &[
            (String::from("ezm-session-91"), true),
            (String::from("ezm-session-91"), true),
            (String::from("ezm-session-91"), false)
        ]
    );
}

#[test]
fn auxiliary_viewer_surfaces_tmux_failures() {
    let tmux = FakeTmux {
        interactive_attach: true,
        auxiliary_error: RefCell::new(Some(String::from("new-window failed"))),
        ..FakeTmux::default()
    };

    let error = auxiliary_viewer("ezm-session-91", true, &tmux).expect_err("aux should fail");
    assert!(error.to_string().contains("new-window failed"));
    assert_eq!(tmux.auxiliary_calls.borrow().len(), 1);
}
