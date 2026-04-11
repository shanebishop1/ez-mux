use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ez_mux::session::LayoutPreset;
use ez_mux::session::RemoteTransportFlags;
use ez_mux::session::SessionAction;
use ez_mux::session::SessionDamageAnalysis;
use ez_mux::session::SessionRepairOutcome;
use ez_mux::session::SlotMode;
use ez_mux::session::SlotModeLaunchContext;
use ez_mux::session::TmuxClient;
use ez_mux::session::analyze_session_damage;
use ez_mux::session::auxiliary_viewer;
use ez_mux::session::ensure_project_session;
use ez_mux::session::ensure_project_session_with_remote_path;
use ez_mux::session::ensure_project_session_with_remote_path_and_options;
use ez_mux::session::focus_slot;
use ez_mux::session::mode_launch_contract;
use ez_mux::session::reconcile_session_damage;
use ez_mux::session::resolve_session_identity;
use ez_mux::session::switch_slot_mode;
use ez_mux::session::teardown_session;
use ez_mux::session::toggle_popup_shell;

struct FakeTmux {
    sessions: RefCell<HashSet<String>>,
    created: RefCell<Vec<(String, PathBuf)>>,
    bootstrapped: RefCell<Vec<(String, PathBuf, u8, bool)>>,
    attached: RefCell<Vec<String>>,
    attach_error: RefCell<Option<String>>,
    mode_switches: RefCell<Vec<(String, u8, SlotMode)>>,
    mode_switch_error: RefCell<Option<String>>,
    swap_calls: RefCell<Vec<(String, u8)>>,
    swap_error: RefCell<Option<String>>,
    focus_calls: RefCell<Vec<(String, u8)>>,
    focus_error: RefCell<Option<String>>,
    popup_toggles: RefCell<Vec<(String, u8)>>,
    popup_toggle_error: RefCell<Option<String>>,
    popup_toggle_open: RefCell<bool>,
    auxiliary_calls: RefCell<Vec<(String, bool)>>,
    auxiliary_error: RefCell<Option<String>>,
    auxiliary_exists: RefCell<bool>,
    auxiliary_available: RefCell<bool>,
    teardown_calls: RefCell<Vec<String>>,
    teardown_project_removed: RefCell<bool>,
    damage_analysis_calls: RefCell<Vec<String>>,
    repair_calls: RefCell<Vec<String>>,
    damage_analysis: RefCell<SessionDamageAnalysis>,
    repair_outcome: RefCell<SessionRepairOutcome>,
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
        if let Some(stderr) = self.attach_error.borrow().as_ref() {
            return Err(ez_mux::session::SessionError::TmuxCommandFailed {
                command: String::from("attach-session"),
                stderr: stderr.clone(),
            });
        }
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
        _launch_context: ez_mux::session::SlotModeLaunchContext<'_>,
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
        session_name: &str,
        project_dir: &Path,
        pane_count: u8,
        no_worktrees: bool,
    ) -> Result<(), ez_mux::session::SessionError> {
        self.bootstrapped.borrow_mut().push((
            session_name.to_string(),
            project_dir.to_path_buf(),
            pane_count,
            no_worktrees,
        ));
        Ok(())
    }

    fn swap_slot_with_center(
        &self,
        session_name: &str,
        slot_id: u8,
    ) -> Result<(), ez_mux::session::SessionError> {
        self.swap_calls
            .borrow_mut()
            .push((session_name.to_string(), slot_id));

        if let Some(stderr) = self.swap_error.borrow().as_ref() {
            return Err(ez_mux::session::SessionError::TmuxCommandFailed {
                command: String::from("__internal swap"),
                stderr: stderr.clone(),
            });
        }

        Ok(())
    }

    fn focus_slot(
        &self,
        session_name: &str,
        slot_id: u8,
    ) -> Result<(), ez_mux::session::SessionError> {
        self.focus_calls
            .borrow_mut()
            .push((session_name.to_string(), slot_id));

        if let Some(stderr) = self.focus_error.borrow().as_ref() {
            return Err(ez_mux::session::SessionError::TmuxCommandFailed {
                command: String::from("__internal focus"),
                stderr: stderr.clone(),
            });
        }

        Ok(())
    }

    fn apply_layout_preset(
        &self,
        _session_name: &str,
        _preset: LayoutPreset,
    ) -> Result<(), ez_mux::session::SessionError> {
        Ok(())
    }

    fn toggle_popup_shell(
        &self,
        session_name: &str,
        slot_id: u8,
        _client_tty: Option<&str>,
        _remote_path: Option<&str>,
        _remote_server_url: Option<&str>,
        _remote_transport: RemoteTransportFlags,
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
        _use_tssh: bool,
        _use_mosh: bool,
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
            if *self.auxiliary_available.borrow() {
                let existed = *self.auxiliary_exists.borrow();
                *self.auxiliary_exists.borrow_mut() = true;
                if existed {
                    ez_mux::session::AuxiliaryViewerAction::Reused
                } else {
                    ez_mux::session::AuxiliaryViewerAction::Created
                }
            } else {
                ez_mux::session::AuxiliaryViewerAction::SkippedUnavailable
            }
        } else {
            *self.auxiliary_exists.borrow_mut() = false;
            ez_mux::session::AuxiliaryViewerAction::Closed
        };

        let window_id = if matches!(
            action,
            ez_mux::session::AuxiliaryViewerAction::Created
                | ez_mux::session::AuxiliaryViewerAction::Reused
        ) {
            Some(String::from("@9"))
        } else {
            None
        };

        Ok(ez_mux::session::AuxiliaryViewerOutcome {
            session_name: session_name.to_owned(),
            action,
            window_name: String::from("perles"),
            window_id,
        })
    }

    fn teardown_session(
        &self,
        session_name: &str,
    ) -> Result<ez_mux::session::TeardownOutcome, ez_mux::session::SessionError> {
        self.teardown_calls
            .borrow_mut()
            .push(session_name.to_string());

        let was_present = *self.teardown_project_removed.borrow();
        *self.teardown_project_removed.borrow_mut() = true;

        Ok(ez_mux::session::TeardownOutcome {
            session_name: session_name.to_owned(),
            helper_sessions_removed: if was_present { 0 } else { 2 },
            helper_processes_removed: if was_present { 0 } else { 3 },
            project_session_removed: !was_present,
        })
    }

    fn analyze_session_damage(
        &self,
        session_name: &str,
    ) -> Result<SessionDamageAnalysis, ez_mux::session::SessionError> {
        self.damage_analysis_calls
            .borrow_mut()
            .push(session_name.to_string());
        Ok(self.damage_analysis.borrow().clone())
    }

    fn reconcile_session_damage(
        &self,
        session_name: &str,
    ) -> Result<SessionRepairOutcome, ez_mux::session::SessionError> {
        self.repair_calls
            .borrow_mut()
            .push(session_name.to_string());
        Ok(self.repair_outcome.borrow().clone())
    }
}

impl Default for FakeTmux {
    fn default() -> Self {
        Self {
            sessions: RefCell::new(HashSet::new()),
            created: RefCell::new(Vec::new()),
            bootstrapped: RefCell::new(Vec::new()),
            attached: RefCell::new(Vec::new()),
            attach_error: RefCell::new(None),
            mode_switches: RefCell::new(Vec::new()),
            mode_switch_error: RefCell::new(None),
            swap_calls: RefCell::new(Vec::new()),
            swap_error: RefCell::new(None),
            focus_calls: RefCell::new(Vec::new()),
            focus_error: RefCell::new(None),
            popup_toggles: RefCell::new(Vec::new()),
            popup_toggle_error: RefCell::new(None),
            popup_toggle_open: RefCell::new(false),
            auxiliary_calls: RefCell::new(Vec::new()),
            auxiliary_error: RefCell::new(None),
            auxiliary_exists: RefCell::new(false),
            auxiliary_available: RefCell::new(true),
            teardown_calls: RefCell::new(Vec::new()),
            teardown_project_removed: RefCell::new(false),
            damage_analysis_calls: RefCell::new(Vec::new()),
            repair_calls: RefCell::new(Vec::new()),
            damage_analysis: RefCell::new(SessionDamageAnalysis {
                healthy_slots: vec![1, 2, 3, 4, 5],
                missing_visible_slots: Vec::new(),
                missing_backing_slots: Vec::new(),
                recreate_order: Vec::new(),
            }),
            repair_outcome: RefCell::new(SessionRepairOutcome {
                session_name: String::from("ezm-session-default"),
                healthy_slots: vec![1, 2, 3, 4, 5],
                recreated_slots: Vec::new(),
            }),
            skipped_non_interactive_attach: RefCell::new(0),
            interactive_attach: false,
        }
    }
}

#[test]
fn runtime_create_path_surfaces_attach_failure_instead_of_reporting_success() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        attach_error: RefCell::new(Some(String::from("attach failed"))),
        ..FakeTmux::default()
    };

    let error = ensure_project_session(project_dir, &tmux).expect_err("create path should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("attach failed"));
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 1);
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
    assert_eq!(first.remote_project_dir, first.identity.project_dir);
    assert_eq!(second.remote_project_dir, second.identity.project_dir);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow()[0].1, first.identity.project_dir);
    assert_eq!(tmux.attached.borrow().len(), 2);
    assert_eq!(tmux.attached.borrow()[0], first.identity.session_name);
    assert_eq!(tmux.attached.borrow()[1], second.identity.session_name);
    assert_eq!(tmux.auxiliary_calls.borrow().len(), 2);
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
    assert_eq!(first.remote_project_dir, first.identity.project_dir);
    assert_eq!(second.remote_project_dir, second.identity.project_dir);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow()[0].1, first.identity.project_dir);
    assert_eq!(tmux.attached.borrow().len(), 2);
    assert_eq!(tmux.auxiliary_calls.borrow().len(), 2);
    assert_eq!(*tmux.skipped_non_interactive_attach.borrow(), 2);
}

#[test]
fn runtime_perles_missing_skips_auxiliary_window_without_failing_startup() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        auxiliary_available: RefCell::new(false),
        ..FakeTmux::default()
    };

    let first = ensure_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 2);
    assert_eq!(tmux.auxiliary_calls.borrow().len(), 2);

    let skipped =
        auxiliary_viewer("ezm-session-perles-missing", true, false, false, &tmux).expect("skip");
    assert_eq!(
        skipped.action,
        ez_mux::session::AuxiliaryViewerAction::SkippedUnavailable
    );
    assert!(skipped.window_id.is_none());
}

#[test]
fn runtime_create_and_bootstrap_use_local_project_dir_when_remote_path_is_active() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_root = temp.path().join("alpha");
    let project_dir = repo_root.join("worktrees").join("feature-x");
    std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
    std::fs::create_dir_all(&project_dir).expect("create project dir");
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = ensure_project_session_with_remote_path(
        project_dir.as_path(),
        Some("/srv/remotes"),
        Some("https://shell.remote.example:7443"),
        RemoteTransportFlags::default(),
        5,
        &tmux,
    )
    .expect("first run");
    let second = ensure_project_session_with_remote_path(
        project_dir.as_path(),
        Some("/srv/remotes"),
        Some("https://shell.remote.example:7443"),
        RemoteTransportFlags::default(),
        5,
        &tmux,
    )
    .expect("second run");

    assert_eq!(first.action, SessionAction::Create);
    assert_eq!(second.action, SessionAction::Attach);
    assert_eq!(
        first.remote_project_dir,
        PathBuf::from("/srv/remotes/alpha/worktrees/feature-x")
    );
    assert_eq!(
        second.remote_project_dir,
        PathBuf::from("/srv/remotes/alpha/worktrees/feature-x")
    );
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.created.borrow()[0].1, first.identity.project_dir);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow()[0].1, first.identity.project_dir);
    assert!(!tmux.bootstrapped.borrow()[0].3);
    assert_eq!(tmux.attached.borrow().len(), 2);
}

#[test]
fn runtime_can_disable_worktree_bootstrap_assignment() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let outcome = ensure_project_session_with_remote_path_and_options(
        project_dir,
        None,
        None,
        RemoteTransportFlags::default(),
        5,
        true,
        &tmux,
    )
    .expect("run should succeed");

    assert_eq!(outcome.action, SessionAction::Create);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(
        tmux.bootstrapped.borrow()[0].1,
        outcome.identity.project_dir
    );
    assert!(tmux.bootstrapped.borrow()[0].3);
}

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
        mode_switch_error: RefCell::new(Some(String::from("respawn-pane failed"))),
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
        swap_error: RefCell::new(Some(String::from("swap-pane failed"))),
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

#[test]
fn popup_toggle_routes_to_tmux_client_and_toggles_open_then_close() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = toggle_popup_shell(
        "ezm-session-88",
        2,
        None,
        None,
        None,
        RemoteTransportFlags::default(),
        &tmux,
    )
    .expect("first toggle");
    let second = toggle_popup_shell(
        "ezm-session-88",
        2,
        None,
        None,
        None,
        RemoteTransportFlags::default(),
        &tmux,
    )
    .expect("second toggle");

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

    let error = toggle_popup_shell(
        "ezm-session-88",
        2,
        None,
        None,
        None,
        RemoteTransportFlags::default(),
        &tmux,
    )
    .expect_err("popup should fail");

    assert!(error.to_string().contains("display-popup failed"));
    assert_eq!(tmux.popup_toggles.borrow().len(), 1);
}

#[test]
fn auxiliary_viewer_create_reuse_close_is_deterministic() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let created = auxiliary_viewer("ezm-session-91", true, false, false, &tmux).expect("create");
    let reused = auxiliary_viewer("ezm-session-91", true, false, false, &tmux).expect("reuse");
    let closed = auxiliary_viewer("ezm-session-91", false, false, false, &tmux).expect("close");

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

    let error =
        auxiliary_viewer("ezm-session-91", true, false, false, &tmux).expect_err("aux should fail");
    assert!(error.to_string().contains("new-window failed"));
    assert_eq!(tmux.auxiliary_calls.borrow().len(), 1);
}

#[test]
fn teardown_pipeline_is_idempotent_when_helpers_are_absent() {
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };

    let first = teardown_session("ezm-session-91", &tmux).expect("first teardown");
    let second = teardown_session("ezm-session-91", &tmux).expect("second teardown");

    assert_eq!(first.session_name, "ezm-session-91");
    assert!(first.project_session_removed);
    assert_eq!(first.helper_sessions_removed, 2);
    assert_eq!(first.helper_processes_removed, 3);

    assert_eq!(second.session_name, "ezm-session-91");
    assert!(!second.project_session_removed);
    assert_eq!(second.helper_sessions_removed, 0);
    assert_eq!(second.helper_processes_removed, 0);

    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[
            String::from("ezm-session-91"),
            String::from("ezm-session-91")
        ]
    );
}

#[test]
fn session_damage_analysis_routes_to_tmux_client() {
    let tmux = FakeTmux {
        interactive_attach: true,
        damage_analysis: RefCell::new(SessionDamageAnalysis {
            healthy_slots: vec![1, 2, 4],
            missing_visible_slots: vec![3, 5],
            missing_backing_slots: Vec::new(),
            recreate_order: vec![3, 5],
        }),
        ..FakeTmux::default()
    };

    let analysis = analyze_session_damage("ezm-session-92", &tmux).expect("analysis");

    assert_eq!(analysis.healthy_slots, vec![1, 2, 4]);
    assert_eq!(analysis.recreate_order, vec![3, 5]);
    assert_eq!(
        tmux.damage_analysis_calls.borrow().as_slice(),
        &[String::from("ezm-session-92")]
    );
}

#[test]
fn selective_reconcile_routes_to_tmux_client() {
    let tmux = FakeTmux {
        interactive_attach: true,
        repair_outcome: RefCell::new(SessionRepairOutcome {
            session_name: String::from("ezm-session-93"),
            healthy_slots: vec![1, 2, 4],
            recreated_slots: vec![3, 5],
        }),
        ..FakeTmux::default()
    };

    let outcome = reconcile_session_damage("ezm-session-93", &tmux).expect("repair");

    assert_eq!(outcome.session_name, "ezm-session-93");
    assert_eq!(outcome.recreated_slots, vec![3, 5]);
    assert_eq!(
        tmux.repair_calls.borrow().as_slice(),
        &[String::from("ezm-session-93")]
    );
}
