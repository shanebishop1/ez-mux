#![allow(dead_code)]

use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use portable_pty::{Child as PtyChild, MasterPty};
use tempfile::TempDir;

mod fixtures;
mod lifecycle;
mod process;
mod pty;
mod tmux;

#[cfg(test)]
mod tests;

pub const MAX_TMUX_SOCKET_PATH_LEN: usize = 90;
const MAX_TERMINAL_OUTPUT: usize = 16 * 1024;
const PTY_TEARDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
const PTY_ATTACH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
const PTY_INPUT_READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const TMUX_SERVER_TEARDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);
const TMUX_SERVER_TEARDOWN_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(10);
const TMUX_WATCHDOG_POLL_INTERVAL: &str = "0.05";
const E2E_ANCHOR_SESSION: &str = "ezm_e2e_anchor";
const E2E_READY_TABLE: &str = "ezm-e2e-ready";
const E2E_READY_OPTION: &str = "@ezm_e2e_client_ready";

pub struct CmdOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub struct EzmBackgroundProcess {
    child: Option<Child>,
    context: String,
}

#[allow(dead_code)]
pub struct PtyAttachProbe {
    pub exit_code: i32,
    pub observed_attached_client: bool,
    pub diagnostics: String,
}

#[allow(dead_code)]
pub struct PtyInterruptProbe {
    pub exit_code: i32,
    pub observed_attached_client: bool,
    pub signal_sent: bool,
    pub diagnostics: String,
}

pub struct PtyTmuxClient {
    client_tty: String,
    tmux_bin: PathBuf,
    tmux_socket_name: String,
    tmux_tmpdir: PathBuf,
    master: Option<Box<dyn MasterPty>>,
    child: Option<Box<dyn PtyChild + Send + Sync>>,
    writer: Option<Box<dyn std::io::Write + Send>>,
    terminal_output: Arc<Mutex<Vec<u8>>>,
    reader_thread: Option<JoinHandle<()>>,
}

pub struct TmuxSettleEvidence {
    pub attempts: u32,
    pub poll_interval_ms: u64,
    pub timeout_ms: u64,
    pub stable: bool,
    pub sessions: String,
    pub windows: String,
    pub panes: String,
}

pub struct FoundationHarness {
    pub run_id: String,
    pub artifact_dir: PathBuf,
    pub tmux_socket_name: String,
    tmux_tmpdir: TempDir,
    tmux_watchdog: Option<Child>,
    pub tmux_bin: PathBuf,
    pub shell: String,
    pub ezm_bin: PathBuf,
    work_dir: PathBuf,
    fake_bin_dir: PathBuf,
    fake_perles_bin_dir: PathBuf,
    open_capture_path: PathBuf,
    project_root: PathBuf,
    verbose_default_launch: bool,
}

fn duration_to_millis_u64(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
