use std::path::Path;
use std::process::Command;

use super::CANONICAL_SLOT_IDS;
use super::DEFAULT_CENTER_WIDTH_PCT;
use super::PaneWidthSample;
use super::SessionError;
use super::SlotMode;
use super::SlotRegistry;
use super::build_registry_for_canonical_panes;
use super::canonical_five_pane_column_widths;
use super::pick_center_pane;
use super::supports_zoom_flag_fallback;

mod attach;
mod command;
mod layout;
mod mode_runtime;
mod options;
mod slot_swap;
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
    fn validate_session_invariants(&self, _session_name: &str) -> Result<(), SessionError> {
        Ok(())
    }

    /// Builds the canonical five-pane layout and persists slot metadata.
    ///
    /// # Errors
    /// Returns an error when tmux operations fail or slot metadata cannot
    /// be persisted consistently.
    fn bootstrap_default_layout(
        &self,
        _session_name: &str,
        _project_dir: &Path,
    ) -> Result<(), SessionError> {
        Ok(())
    }

    /// Swaps a target slot with the center pane while preserving zoom state.
    ///
    /// # Errors
    /// Returns an error when slot metadata is invalid, target slot is
    /// outside the canonical range, or tmux swap/select operations fail.
    fn swap_slot_with_center(&self, _session_name: &str, _slot_id: u8) -> Result<(), SessionError> {
        Ok(())
    }

    /// Switches a canonical slot to one runtime mode.
    ///
    /// # Errors
    /// Returns an error when slot metadata is invalid or tmux cannot execute
    /// teardown/respawn actions for the target mode.
    fn switch_slot_mode(
        &self,
        _session_name: &str,
        _slot_id: u8,
        _mode: SlotMode,
    ) -> Result<(), SessionError> {
        Ok(())
    }
}

pub struct ProcessTmuxClient;

impl TmuxClient for ProcessTmuxClient {
    fn session_exists(&self, session_name: &str) -> Result<bool, SessionError> {
        let args = ["has-session", "-t", session_name];
        let output = command::tmux_output(&args)?;

        if output.status.success() {
            return Ok(true);
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.contains("can't find session") {
            return Ok(false);
        }

        Err(SessionError::TmuxCommandFailed {
            command: args.join(" "),
            stderr,
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
        slot_swap::validate_canonical_slot_registry(session_name)
    }

    fn bootstrap_default_layout(
        &self,
        session_name: &str,
        project_dir: &Path,
    ) -> Result<(), SessionError> {
        layout::bootstrap_default_layout(session_name, project_dir)
    }

    fn swap_slot_with_center(&self, session_name: &str, slot_id: u8) -> Result<(), SessionError> {
        slot_swap::swap_slot_with_center(session_name, slot_id)
    }

    fn switch_slot_mode(
        &self,
        session_name: &str,
        slot_id: u8,
        mode: SlotMode,
    ) -> Result<(), SessionError> {
        mode_runtime::switch_slot_mode(session_name, slot_id, mode)
    }
}
