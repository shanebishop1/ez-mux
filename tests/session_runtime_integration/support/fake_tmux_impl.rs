use std::path::Path;

use ez_mux::config::SessionRuntimeContext;
use ez_mux::session::LayoutPreset;
use ez_mux::session::RemoteTransportFlags;
use ez_mux::session::SessionDamageAnalysis;
use ez_mux::session::SessionRepairOutcome;
use ez_mux::session::SlotMode;
use ez_mux::session::TmuxClient;

use super::fake_tmux::FakeTmux;

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

fn ezmux_session_error(command: &str, stderr: String) -> ez_mux::session::SessionError {
    ez_mux::session::SessionError::TmuxCommandFailed {
        command: command.to_owned(),
        stderr,
    }
}
