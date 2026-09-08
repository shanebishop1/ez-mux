use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use ez_mux::config::SessionRuntimeContext;
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
    bootstrap_error: RefCell<Option<String>>,
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
    teardown_error: RefCell<Option<String>>,
    teardown_project_removed: RefCell<bool>,
    helpers_created_during_bootstrap: RefCell<Vec<String>>,
    damage_analysis_calls: RefCell<Vec<String>>,
    repair_calls: RefCell<Vec<String>>,
    damage_analysis: RefCell<SessionDamageAnalysis>,
    repair_outcome: RefCell<SessionRepairOutcome>,
    skipped_non_interactive_attach: RefCell<u32>,
    interactive_attach: bool,
    runtime_contexts: RefCell<HashMap<String, SessionRuntimeContext>>,
    runtime_passwords: RefCell<HashMap<String, Option<String>>>,
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

    fn reconcile_session_runtime_context(
        &self,
        session_name: &str,
        context: &SessionRuntimeContext,
    ) -> Result<(), ez_mux::session::SessionError> {
        self.runtime_contexts
            .borrow_mut()
            .entry(session_name.to_owned())
            .or_insert_with(|| context.clone());
        Ok(())
    }

    fn reconcile_session_runtime_auth(
        &self,
        session_name: &str,
        password: Option<&str>,
    ) -> Result<(), ez_mux::session::SessionError> {
        self.runtime_passwords
            .borrow_mut()
            .insert(session_name.to_owned(), password.map(str::to_owned));
        Ok(())
    }

    fn resolve_session_runtime_context(
        &self,
        session_name: &str,
        context: &SessionRuntimeContext,
    ) -> Result<SessionRuntimeContext, ez_mux::session::SessionError> {
        Ok(self
            .runtime_contexts
            .borrow()
            .get(session_name)
            .cloned()
            .unwrap_or_else(|| context.clone()))
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
        if let Some(stderr) = self.bootstrap_error.borrow_mut().take() {
            return Err(ezmux_session_error("bootstrap-default-layout", stderr));
        }
        for helper in self.helpers_created_during_bootstrap.borrow().iter() {
            self.sessions.borrow_mut().insert(helper.clone());
        }
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

        if let Some(stderr) = self.teardown_error.borrow().as_ref() {
            return Err(ezmux_session_error("teardown-session", stderr.clone()));
        }

        let was_present = *self.teardown_project_removed.borrow();
        *self.teardown_project_removed.borrow_mut() = true;
        self.sessions.borrow_mut().remove(session_name);

        Ok(ez_mux::session::TeardownOutcome {
            session_name: session_name.to_owned(),
            helper_sessions_removed: if was_present { 0 } else { 2 },
            helper_processes_removed: if was_present { 0 } else { 3 },
            project_session_removed: !was_present,
        })
    }

    fn teardown_owned_session(
        &self,
        session_name: &str,
        ownership: &ez_mux::session::TeardownOwnership,
    ) -> Result<ez_mux::session::TeardownOutcome, ez_mux::session::SessionError> {
        self.teardown_calls
            .borrow_mut()
            .push(session_name.to_string());

        if let Some(stderr) = self.teardown_error.borrow().as_ref() {
            return Err(ezmux_session_error(
                "teardown-owned-session",
                stderr.clone(),
            ));
        }

        let was_present = self.sessions.borrow_mut().remove(session_name);
        let helper_sessions_removed = ownership
            .helper_sessions
            .iter()
            .filter(|helper| self.sessions.borrow_mut().remove(*helper))
            .count();

        Ok(ez_mux::session::TeardownOutcome {
            session_name: session_name.to_owned(),
            helper_sessions_removed,
            helper_processes_removed: 0,
            project_session_removed: was_present,
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
            bootstrap_error: RefCell::new(None),
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
            teardown_error: RefCell::new(None),
            teardown_project_removed: RefCell::new(false),
            helpers_created_during_bootstrap: RefCell::new(Vec::new()),
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
            runtime_contexts: RefCell::new(HashMap::new()),
            runtime_passwords: RefCell::new(HashMap::new()),
        }
    }
}

fn ezmux_session_error(command: &str, stderr: String) -> ez_mux::session::SessionError {
    ez_mux::session::SessionError::TmuxCommandFailed {
        command: command.to_owned(),
        stderr,
    }
}

fn ensure_local_project_session(
    project_dir: &Path,
    tmux: &impl TmuxClient,
) -> Result<ez_mux::session::SessionLaunchOutcome, ez_mux::session::SessionError> {
    ensure_project_session_with_remote_path(
        project_dir,
        None,
        None,
        RemoteTransportFlags::default(),
        5,
        tmux,
    )
}

#[test]
fn runtime_create_path_rolls_back_after_attach_failure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        interactive_attach: true,
        attach_error: RefCell::new(Some(String::from("attach failed"))),
        ..FakeTmux::default()
    };

    let error =
        ensure_local_project_session(project_dir, &tmux).expect_err("create path should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("attach failed"));
    assert_eq!(tmux.created.borrow().len(), 1);
    assert_eq!(tmux.bootstrapped.borrow().len(), 1);
    assert_eq!(tmux.attached.borrow().len(), 1);
    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[error_session_name(project_dir)]
    );
    assert!(
        !tmux
            .sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );
}

fn error_session_name(project_dir: &Path) -> String {
    resolve_session_identity(project_dir)
        .expect("resolve session identity")
        .session_name
}

#[test]
fn runtime_rolls_back_when_layout_bootstrap_fails() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        bootstrap_error: RefCell::new(Some(String::from("injected layout failure"))),
        ..FakeTmux::default()
    };

    let error = ensure_local_project_session(project_dir, &tmux)
        .expect_err("injected bootstrap failure should be returned");

    assert!(error.to_string().contains("injected layout failure"));
    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[error_session_name(project_dir)]
    );
    assert!(
        !tmux
            .sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );
    assert!(tmux.attached.borrow().is_empty());
}

#[test]
fn runtime_rolls_back_when_auxiliary_bootstrap_fails() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        auxiliary_error: RefCell::new(Some(String::from("injected auxiliary failure"))),
        ..FakeTmux::default()
    };

    let error = ensure_local_project_session(project_dir, &tmux)
        .expect_err("injected auxiliary failure should be returned");

    assert!(error.to_string().contains("injected auxiliary failure"));
    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[error_session_name(project_dir)]
    );
    assert!(
        !tmux
            .sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );
}

#[test]
fn runtime_reports_bootstrap_and_cleanup_failures_together() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        attach_error: RefCell::new(Some(String::from("injected attach failure"))),
        teardown_error: RefCell::new(Some(String::from("injected cleanup failure"))),
        ..FakeTmux::default()
    };

    let error = ensure_local_project_session(project_dir, &tmux)
        .expect_err("injected attach failure should be returned");
    let rendered = error.to_string();

    assert!(rendered.contains("injected attach failure"));
    assert!(rendered.contains("injected cleanup failure"));
    assert_eq!(
        tmux.teardown_calls.borrow().as_slice(),
        &[error_session_name(project_dir)]
    );
    assert!(
        tmux.sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );
}

#[test]
fn runtime_never_rolls_back_a_preexisting_session_after_attach_failure() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let session_name = error_session_name(project_dir);
    let tmux = FakeTmux {
        sessions: RefCell::new(HashSet::from([session_name.clone()])),
        attach_error: RefCell::new(Some(String::from("pre-existing attach failure"))),
        ..FakeTmux::default()
    };

    let error = ensure_local_project_session(project_dir, &tmux)
        .expect_err("pre-existing attach failure should be returned");

    assert!(error.to_string().contains("pre-existing attach failure"));
    assert!(tmux.teardown_calls.borrow().is_empty());
    assert!(tmux.sessions.borrow().contains(&session_name));
    assert!(tmux.created.borrow().is_empty());
}

#[test]
fn runtime_rollback_preserves_preexisting_helpers_and_same_prefix_sessions() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let session_name = error_session_name(project_dir);
    let mode_cache = format!("{session_name}__mode_cache");
    let popup = format!("{session_name}__popup_slot_1");
    let unrelated = format!("{session_name}__user-owned");
    let tmux = FakeTmux {
        sessions: RefCell::new(HashSet::from([
            mode_cache.clone(),
            popup.clone(),
            unrelated.clone(),
        ])),
        attach_error: RefCell::new(Some(String::from("attach failed after bootstrap"))),
        interactive_attach: true,
        ..FakeTmux::default()
    };

    ensure_local_project_session(project_dir, &tmux).expect_err("attach should fail");

    let sessions = tmux.sessions.borrow();
    assert!(sessions.contains(&mode_cache));
    assert!(sessions.contains(&popup));
    assert!(sessions.contains(&unrelated));
    assert!(!sessions.contains(&session_name));
}

#[test]
fn runtime_rollback_removes_only_known_helpers_created_during_bootstrap() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let session_name = error_session_name(project_dir);
    let mode_cache = format!("{session_name}__mode_cache");
    let popup = format!("{session_name}__popup_slot_2");
    let unrelated = format!("{session_name}__user-owned");
    let tmux = FakeTmux {
        helpers_created_during_bootstrap: RefCell::new(vec![
            mode_cache.clone(),
            popup.clone(),
            unrelated.clone(),
        ]),
        attach_error: RefCell::new(Some(String::from("attach failed after bootstrap"))),
        interactive_attach: true,
        ..FakeTmux::default()
    };

    ensure_local_project_session(project_dir, &tmux).expect_err("attach should fail");

    let sessions = tmux.sessions.borrow();
    assert!(!sessions.contains(&mode_cache));
    assert!(!sessions.contains(&popup));
    assert!(sessions.contains(&unrelated));
    assert!(!sessions.contains(&session_name));
}

#[test]
fn runtime_can_relaunch_after_successful_bootstrap_rollback() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_dir = temp.path();
    let tmux = FakeTmux {
        bootstrap_error: RefCell::new(Some(String::from("fail once"))),
        interactive_attach: true,
        ..FakeTmux::default()
    };

    ensure_local_project_session(project_dir, &tmux).expect_err("first run should fail");
    assert!(
        !tmux
            .sessions
            .borrow()
            .contains(&error_session_name(project_dir))
    );

    let outcome = ensure_local_project_session(project_dir, &tmux).expect("second run");

    assert_eq!(outcome.action, SessionAction::Create);
    assert_eq!(tmux.created.borrow().len(), 2);
    assert_eq!(tmux.bootstrapped.borrow().len(), 2);
    assert_eq!(tmux.teardown_calls.borrow().len(), 1);
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

    let first = ensure_local_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_local_project_session(project_dir, &tmux).expect("second run");

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

    let first = ensure_local_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_local_project_session(project_dir, &tmux).expect("second run");

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

    let first = ensure_local_project_session(project_dir, &tmux).expect("first run");
    let second = ensure_local_project_session(project_dir, &tmux).expect("second run");

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
fn runtime_context_isolated_between_projects_and_preserved_on_reopen() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_a = temp.path().join("project-a");
    let project_b = temp.path().join("project-b");
    let project_empty = temp.path().join("project-empty");
    std::fs::create_dir_all(&project_a).expect("create project A");
    std::fs::create_dir_all(&project_b).expect("create project B");
    std::fs::create_dir_all(&project_empty).expect("create empty project");
    let tmux = FakeTmux {
        interactive_attach: true,
        ..FakeTmux::default()
    };
    let context_a = test_runtime_context("/srv/a", "a.example");
    let context_b = test_runtime_context("/srv/b", "b.example");
    let context_empty = ez_mux::config::RuntimeContext::default();

    let first_a = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_a, &context_a, 1, true, &tmux,
    )
    .expect("project A should start");
    let first_b = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_b, &context_b, 1, true, &tmux,
    )
    .expect("project B should start");
    let first_empty = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_empty,
        &context_empty,
        1,
        true,
        &tmux,
    )
    .expect("project without a password should start");
    let reopened_a = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_a, &context_b, 1, true, &tmux,
    )
    .expect("project A should reopen");

    assert_eq!(
        first_a.remote_project_dir,
        PathBuf::from("/srv/a/project-a")
    );
    assert_eq!(
        first_b.remote_project_dir,
        PathBuf::from("/srv/b/project-b")
    );
    assert_eq!(reopened_a.remote_project_dir, first_a.remote_project_dir);
    assert_ne!(first_a.remote_project_dir, first_b.remote_project_dir);
    assert_eq!(
        tmux.runtime_passwords
            .borrow()
            .get(&first_a.identity.session_name),
        Some(&Some(String::from("password-a")))
    );
    assert_eq!(
        tmux.runtime_passwords
            .borrow()
            .get(&first_b.identity.session_name),
        Some(&Some(String::from("password-b")))
    );
    assert_eq!(
        tmux.runtime_passwords
            .borrow()
            .get(&first_empty.identity.session_name),
        Some(&None)
    );
}

#[test]
fn same_url_projects_keep_distinct_session_credentials() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project_a = temp.path().join("project-a");
    let project_b = temp.path().join("project-b");
    std::fs::create_dir_all(&project_a).expect("create project A");
    std::fs::create_dir_all(&project_b).expect("create project B");
    let tmux = FakeTmux::default();
    let context_a = test_runtime_context("/srv/a", "shared.example");
    let mut context_b = test_runtime_context("/srv/b", "shared.example");
    context_b.remote.shared_server.password.value = Some(String::from("password-b"));

    let first_a = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_a, &context_a, 1, true, &tmux,
    )
    .expect("project A should start");
    let first_b = ez_mux::session::ensure_project_session_with_runtime_context(
        &project_b, &context_b, 1, true, &tmux,
    )
    .expect("project B should start");
    let passwords = tmux.runtime_passwords.borrow();

    assert_eq!(
        passwords.get(&first_a.identity.session_name),
        Some(&Some(String::from("password-s")))
    );
    assert_eq!(
        passwords.get(&first_b.identity.session_name),
        Some(&Some(String::from("password-b")))
    );
}

#[test]
fn absent_password_on_reopen_preserves_existing_session_credential() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("project-a");
    std::fs::create_dir_all(&project).expect("create project");
    let tmux = FakeTmux::default();
    let context = test_runtime_context("/srv/a", "a.example");
    let first = ez_mux::session::ensure_project_session_with_runtime_context(
        &project, &context, 1, true, &tmux,
    )
    .expect("project should start");
    let mut context_without_password = context;
    context_without_password.remote.shared_server.password.value = None;

    ez_mux::session::ensure_project_session_with_runtime_context(
        &project,
        &context_without_password,
        1,
        true,
        &tmux,
    )
    .expect("project should reopen without replacing its credential");

    assert_eq!(
        tmux.runtime_passwords
            .borrow()
            .get(&first.identity.session_name),
        Some(&Some(String::from("password-a")))
    );
}

fn test_runtime_context(remote_path: &str, server: &str) -> ez_mux::config::RuntimeContext {
    let mut context = ez_mux::config::RuntimeContext::default();
    context.remote.remote_path.value = Some(remote_path.to_owned());
    context.remote.remote_server_url.value = Some(server.to_owned());
    context.remote.shared_server.url.value = Some(format!("http://{server}:4096"));
    let password_suffix = server.chars().next().unwrap();
    context.remote.shared_server.password.value = Some(format!("password-{password_suffix}"));
    context
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
