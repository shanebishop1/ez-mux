use std::path::Path;
use std::process::Command;

use super::AuxiliaryViewerOutcome;
use super::CANONICAL_SLOT_IDS;
use super::DEFAULT_CENTER_WIDTH_PCT;
use super::LayoutPreset;
use super::PaneWidthSample;
use super::PopupShellOutcome;
use super::SessionError;
use super::SharedServerAttachConfig;
use super::SlotMode;
use super::SlotRegistry;
use super::TeardownOutcome;
use super::ZoomFlagSupport;
use super::build_registry_for_canonical_panes;
use super::canonical_five_pane_column_widths;
use super::pick_center_pane;
use super::tmux_diagnostics_exit_status;
use super::zoom_flag_support_for_command;

mod attach;
mod auxiliary;
mod command;
mod keybinds;
mod layout;
mod mode_runtime;
mod options;
mod popup;
mod repair;
mod slot_focus;
mod slot_swap;
mod style;
mod teardown;
mod worktree;

pub trait TmuxClient {
    /// Returns whether the named tmux session is currently present.
    ///
    /// # Errors
    /// Returns an error when tmux cannot be started or when tmux reports
    /// an unexpected failure while checking the session.
    fn session_exists(&self, session_name: &str) -> Result<bool, SessionError>;

    /// Creates a detached tmux session rooted at `cwd`.
    ///
    /// # Errors
    /// Returns an error when tmux cannot be started or when tmux rejects
    /// the create-session command.
    fn create_detached_session(&self, session_name: &str, cwd: &Path) -> Result<(), SessionError>;

    /// Attaches the current terminal to a tmux session when interactive.
    ///
    /// # Errors
    /// Returns an error when attach is attempted and tmux cannot be started
    /// or the attach command fails.
    fn attach_session(&self, session_name: &str) -> Result<(), SessionError>;

    /// Verifies ez-mux slot metadata invariants inside tmux options.
    ///
    /// # Errors
    /// Returns an error when required session or pane options are missing,
    /// invalid, or inconsistent.
    fn validate_session_invariants(&self, session_name: &str) -> Result<(), SessionError>;

    /// Builds the canonical five-pane layout and persists slot metadata.
    ///
    /// # Errors
    /// Returns an error when tmux operations fail or slot metadata cannot
    /// be persisted consistently.
    fn bootstrap_default_layout(
        &self,
        session_name: &str,
        project_dir: &Path,
    ) -> Result<(), SessionError>;

    /// Applies one supported layout preset to an existing session.
    ///
    /// # Errors
    /// Returns an error when tmux cannot apply the preset deterministically.
    fn apply_layout_preset(
        &self,
        session_name: &str,
        preset: LayoutPreset,
    ) -> Result<(), SessionError>;

    /// Swaps a target slot with the center pane while preserving zoom state.
    ///
    /// # Errors
    /// Returns an error when slot metadata is invalid, target slot is
    /// outside the canonical range, or tmux swap/select operations fail.
    fn swap_slot_with_center(&self, session_name: &str, slot_id: u8) -> Result<(), SessionError>;

    /// Moves one canonical slot pane into center focus position.
    ///
    /// # Errors
    /// Returns an error when slot metadata is invalid, target slot is
    /// outside the canonical range, or tmux cannot perform swap/select steps.
    fn focus_slot(&self, session_name: &str, slot_id: u8) -> Result<(), SessionError>;

    /// Switches a canonical slot to one runtime mode.
    ///
    /// # Errors
    /// Returns an error when slot metadata is invalid or tmux cannot execute
    /// teardown/respawn actions for the target mode.
    fn switch_slot_mode(
        &self,
        session_name: &str,
        slot_id: u8,
        mode: SlotMode,
        operator: Option<&str>,
        remote_prefix: Option<&str>,
        shared_server: Option<&SharedServerAttachConfig>,
    ) -> Result<(), SessionError>;

    /// Toggles popup shell helper session for one canonical slot.
    ///
    /// # Errors
    /// Returns an error when slot metadata is invalid or popup orchestration
    /// fails.
    fn toggle_popup_shell(
        &self,
        session_name: &str,
        slot_id: u8,
        client_tty: Option<&str>,
    ) -> Result<PopupShellOutcome, SessionError>;

    /// Creates/reuses or closes the auxiliary viewer window.
    ///
    /// # Errors
    /// Returns an error when tmux cannot reconcile auxiliary window state.
    fn auxiliary_viewer(
        &self,
        session_name: &str,
        open: bool,
    ) -> Result<AuxiliaryViewerOutcome, SessionError>;

    /// Removes helper sessions/processes and the project session.
    ///
    /// # Errors
    /// Returns an error when teardown reconciliation fails.
    fn teardown_session(&self, session_name: &str) -> Result<TeardownOutcome, SessionError>;

    /// Reports missing visible panes and required backing panes.
    ///
    /// # Errors
    /// Returns an error when slot metadata cannot be inspected.
    fn analyze_session_damage(
        &self,
        session_name: &str,
    ) -> Result<super::SessionDamageAnalysis, SessionError>;

    /// Recreates only missing panes required to restore canonical slots.
    ///
    /// # Errors
    /// Returns an error when selective reconcile cannot proceed safely.
    fn reconcile_session_damage(
        &self,
        session_name: &str,
    ) -> Result<super::SessionRepairOutcome, SessionError>;
}

pub struct ProcessTmuxClient;

impl TmuxClient for ProcessTmuxClient {
    fn session_exists(&self, session_name: &str) -> Result<bool, SessionError> {
        let args = ["-q", "has-session", "-t", session_name];
        let output = command::tmux_output(&args)?;

        if output.status.success() {
            return Ok(true);
        }

        if output.status.code() == Some(1) {
            return Ok(false);
        }

        Err(SessionError::TmuxCommandFailed {
            command: args.join(" "),
            stderr: command::format_output_diagnostics(&output),
        })
    }

    fn create_detached_session(&self, session_name: &str, cwd: &Path) -> Result<(), SessionError> {
        let output = Command::new("tmux")
            .arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(session_name)
            .arg("-c")
            .arg(cwd)
            .output()
            .map_err(|source| SessionError::TmuxSpawnFailed {
                command: format!("new-session -d -s {session_name} -c {}", cwd.display()),
                source,
            })?;

        if output.status.success() {
            return Ok(());
        }

        Err(SessionError::TmuxCommandFailed {
            command: format!("new-session -d -s {session_name} -c {}", cwd.display()),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }

    fn attach_session(&self, session_name: &str) -> Result<(), SessionError> {
        attach::attach_session(session_name)
    }

    fn validate_session_invariants(&self, session_name: &str) -> Result<(), SessionError> {
        slot_swap::validate_canonical_slot_registry(session_name)?;
        popup::reconcile_popup_parent_cleanup_hook()?;
        keybinds::install_runtime_keybinds()?;
        style::apply_runtime_style_defaults(session_name)
    }

    fn bootstrap_default_layout(
        &self,
        session_name: &str,
        project_dir: &Path,
    ) -> Result<(), SessionError> {
        layout::bootstrap_default_layout(session_name, project_dir)?;
        popup::reconcile_popup_parent_cleanup_hook()
    }

    fn swap_slot_with_center(&self, session_name: &str, slot_id: u8) -> Result<(), SessionError> {
        slot_swap::swap_slot_with_center(session_name, slot_id)
    }

    fn focus_slot(&self, session_name: &str, slot_id: u8) -> Result<(), SessionError> {
        slot_focus::focus_slot(session_name, slot_id)
    }

    fn apply_layout_preset(
        &self,
        session_name: &str,
        preset: LayoutPreset,
    ) -> Result<(), SessionError> {
        layout::apply_layout_preset(session_name, preset)
    }

    fn switch_slot_mode(
        &self,
        session_name: &str,
        slot_id: u8,
        mode: SlotMode,
        operator: Option<&str>,
        remote_prefix: Option<&str>,
        shared_server: Option<&SharedServerAttachConfig>,
    ) -> Result<(), SessionError> {
        mode_runtime::switch_slot_mode(
            session_name,
            slot_id,
            mode,
            operator,
            remote_prefix,
            shared_server,
        )
    }

    fn toggle_popup_shell(
        &self,
        session_name: &str,
        slot_id: u8,
        client_tty: Option<&str>,
    ) -> Result<PopupShellOutcome, SessionError> {
        popup::toggle_popup_shell(session_name, slot_id, client_tty)
    }

    fn auxiliary_viewer(
        &self,
        session_name: &str,
        open: bool,
    ) -> Result<AuxiliaryViewerOutcome, SessionError> {
        auxiliary::auxiliary_viewer(session_name, open)
    }

    fn teardown_session(&self, session_name: &str) -> Result<TeardownOutcome, SessionError> {
        teardown::teardown_session(session_name)
    }

    fn analyze_session_damage(
        &self,
        session_name: &str,
    ) -> Result<super::SessionDamageAnalysis, SessionError> {
        repair::analyze_session_damage(session_name)
    }

    fn reconcile_session_damage(
        &self,
        session_name: &str,
    ) -> Result<super::SessionRepairOutcome, SessionError> {
        repair::reconcile_session_damage(session_name)
    }
}
