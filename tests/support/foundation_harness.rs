#![allow(dead_code)]

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use portable_pty::{Child as PtyChild, CommandBuilder, MasterPty, PtySize, native_pty_system};
use tempfile::{Builder as TempDirBuilder, TempDir};

pub const MAX_TMUX_SOCKET_PATH_LEN: usize = 90;
const MAX_TERMINAL_OUTPUT: usize = 16 * 1024;
const PTY_TEARDOWN_TIMEOUT: Duration = Duration::from_secs(1);
const PTY_ATTACH_TIMEOUT: Duration = Duration::from_secs(20);
const PTY_INPUT_READY_TIMEOUT: Duration = Duration::from_secs(5);
const TMUX_SERVER_TEARDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const TMUX_SERVER_TEARDOWN_POLL_INTERVAL: Duration = Duration::from_millis(10);
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

impl EzmBackgroundProcess {
    pub fn wait_for_exit(&mut self, timeout: Duration) -> Result<i32, String> {
        if self.child.is_none() {
            return Err(format!("{} process was already reaped", self.context));
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self
                .child
                .as_mut()
                .expect("child checked above")
                .try_wait()
                .map_err(|error| {
                    format!("failed checking {} process status: {error}", self.context)
                })?
            {
                let _ = self.child.take();
                return Ok(status.code().unwrap_or(-1));
            }
            if Instant::now() >= deadline {
                if let Some(mut child) = self.child.take() {
                    terminate_background_child(&mut child, &self.context);
                }
                return Err(format!(
                    "timed out waiting for {} process to exit after {} ms",
                    self.context,
                    timeout.as_millis()
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for EzmBackgroundProcess {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        terminate_background_child(&mut child, &self.context);
    }
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
}

pub struct PtyTmuxClient {
    client_tty: String,
    tmux_bin: PathBuf,
    tmux_socket_name: String,
    tmux_tmpdir: PathBuf,
    master: Option<Box<dyn MasterPty>>,
    child: Option<Box<dyn PtyChild + Send + Sync>>,
    writer: Option<Box<dyn Write + Send>>,
    terminal_output: Arc<Mutex<Vec<u8>>>,
    reader_thread: Option<JoinHandle<()>>,
}

impl PtyTmuxClient {
    pub fn client_tty(&self) -> &str {
        &self.client_tty
    }

    pub fn send_prefix_key(&mut self, key: &str) -> Result<(), String> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| String::from("tmux client PTY writer is closed"))?;
        writer
            .write_all(&[0x02])
            .and_then(|()| writer.write_all(key.as_bytes()))
            .and_then(|()| writer.flush())
            .map_err(|error| format!("failed writing prefix key {key:?} to tmux client: {error}"))
    }

    pub fn terminal_output(&self) -> String {
        terminal_output_for_diagnostic(&self.terminal_output)
    }
}

impl Drop for PtyTmuxClient {
    fn drop(&mut self) {
        let _ = Command::new(&self.tmux_bin)
            .arg("-S")
            .arg(&self.tmux_socket_name)
            .arg("-f")
            .arg("/dev/null")
            .arg("detach-client")
            .arg("-t")
            .arg(&self.client_tty)
            .env("TMUX_TMPDIR", &self.tmux_tmpdir)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .output();
        let master = self.master.take();
        let writer = self.writer.take();
        let reader_thread = self.reader_thread.take();

        if let Some(child) = self.child.as_mut() {
            teardown_pty_resources(
                master,
                writer,
                &mut **child,
                reader_thread,
                "tmux client teardown",
            );
        }
    }
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
    open_capture_path: PathBuf,
    project_root: PathBuf,
    verbose_default_launch: bool,
}

impl FoundationHarness {
    #[allow(dead_code)]
    pub fn new() -> Result<Self, String> {
        Self::new_for_suite("foundation")
    }

    pub fn new_for_suite(suite_name: &str) -> Result<Self, String> {
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let target_dir = project_root
            .join("target")
            .join("e2e-evidence")
            .join(suite_name);
        fs::create_dir_all(&target_dir)
            .map_err(|error| format!("failed creating evidence base directory: {error}"))?;

        let run_id = build_run_id();
        let artifact_dir = target_dir.join(&run_id);
        let work_dir = artifact_dir.join("tmp");
        let tmux_tmpdir = create_short_private_tmux_dir()?;
        let tmux_socket_name = tmux_tmpdir
            .path()
            .canonicalize()
            .map_err(|error| format!("failed canonicalizing tmux socket directory: {error}"))?
            .join("s")
            .to_string_lossy()
            .into_owned();
        let fake_bin_dir = work_dir.join("bin");
        let open_capture_path = work_dir.join("open-latest-arg.txt");

        fs::create_dir_all(&artifact_dir)
            .map_err(|error| format!("failed creating artifact directory: {error}"))?;
        fs::create_dir_all(&fake_bin_dir)
            .map_err(|error| format!("failed creating fake bin directory: {error}"))?;

        let tmux_bin = resolve_tool_path("tmux")?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| String::from("unknown"));
        let ezm_bin = resolve_ezm_bin(&project_root)?;

        install_fake_opener_scripts(&fake_bin_dir)?;
        install_tmux_wrapper(&fake_bin_dir, &tmux_bin)?;

        let harness = Self {
            run_id,
            artifact_dir,
            tmux_socket_name,
            tmux_tmpdir,
            tmux_watchdog: None,
            tmux_bin,
            shell,
            ezm_bin,
            work_dir,
            fake_bin_dir,
            open_capture_path,
            project_root,
            verbose_default_launch: suite_name != "foundation",
        };

        harness.start_tmux_server()?;
        let mut harness = harness;
        harness.start_tmux_watchdog()?;
        Ok(harness)
    }

    pub fn tmux_version(&self) -> Result<String, String> {
        let output = Command::new(&self.tmux_bin)
            .arg("-V")
            .output()
            .map_err(|error| format!("failed reading tmux version: {error}"))?;

        if !output.status.success() {
            return Err(format!(
                "tmux -V failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn tmux_socket_path(&self) -> &Path {
        Path::new(&self.tmux_socket_name)
    }

    pub fn work_dir(&self) -> &Path {
        &self.work_dir
    }

    pub fn reset_scenario_state(&self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let clients =
                self.tmux_capture(&["list-clients", "-F", "#{client_tty}|#{session_name}"])?;
            for line in clients.lines().filter(|line| !line.trim().is_empty()) {
                let client_tty = line
                    .split_once('|')
                    .map_or(line, |(client_tty, _)| client_tty)
                    .trim();
                if !client_tty.is_empty() {
                    let _ = self.tmux_capture(&["detach-client", "-t", client_tty]);
                }
            }

            let sessions =
                self.tmux_capture(&["list-sessions", "-F", "#{session_id}|#{session_name}"])?;
            let mut anchor_present = false;
            let mut scenario_session_ids = Vec::new();
            for line in sessions.lines().filter(|line| !line.trim().is_empty()) {
                let Some((session_id, session_name)) = line.split_once('|') else {
                    return Err(format!(
                        "unexpected tmux session record during cleanup: {line:?}"
                    ));
                };
                if session_name.trim() == E2E_ANCHOR_SESSION {
                    anchor_present = true;
                } else {
                    scenario_session_ids.push(session_id.trim().to_owned());
                }
            }

            if !anchor_present {
                return Err(format!(
                    "isolated tmux server lost required anchor session {E2E_ANCHOR_SESSION:?}; sessions={sessions:?}"
                ));
            }
            if clients.trim().is_empty() && scenario_session_ids.is_empty() {
                return Ok(());
            }

            for session_id in scenario_session_ids {
                let _ = self.tmux_capture(&["kill-session", "-t", &session_id]);
            }

            if Instant::now() >= deadline {
                let remaining_clients = self
                    .tmux_capture(&["list-clients", "-F", "#{client_tty}|#{session_name}"])
                    .unwrap_or_else(|error| format!("<unavailable: {error}>"));
                let remaining_sessions = self
                    .tmux_capture(&["list-sessions", "-F", "#{session_id}|#{session_name}"])
                    .unwrap_or_else(|error| format!("<unavailable: {error}>"));
                return Err(format!(
                    "timed out restoring scenario baseline; clients={remaining_clients:?}; sessions={remaining_sessions:?}"
                ));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[allow(dead_code)]
    pub fn open_capture_path(&self) -> &Path {
        &self.open_capture_path
    }

    #[allow(dead_code)]
    pub fn write_file(path: &Path, content: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed creating parent directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        fs::write(path, content)
            .map_err(|error| format!("failed writing file {}: {error}", path.display()))
    }

    pub fn run_ezm(
        &self,
        args: &[&str],
        env_overrides: &[(&str, &str)],
        opener_exit_code: i32,
    ) -> Result<CmdOutput, String> {
        self.run_ezm_in_dir(self.project_root(), args, env_overrides, opener_exit_code)
    }

    pub fn run_ezm_in_dir(
        &self,
        project_dir: &Path,
        args: &[&str],
        env_overrides: &[(&str, &str)],
        opener_exit_code: i32,
    ) -> Result<CmdOutput, String> {
        if args.first().is_some_and(|arg| *arg == "__internal") {
            let _ = self.settle_tmux_snapshot(Duration::from_millis(25), Duration::from_secs(1));
        }

        let output = self
            .ezm_command(project_dir, args, env_overrides, opener_exit_code)?
            .output()
            .map_err(|error| format!("failed running ezm {args:?}: {error}"))?;

        Ok(CmdOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    pub fn spawn_ezm(
        &self,
        args: &[&str],
        env_overrides: &[(&str, &str)],
        opener_exit_code: i32,
        context: &str,
    ) -> Result<EzmBackgroundProcess, String> {
        if args.first().is_some_and(|arg| *arg == "__internal") {
            let _ = self.settle_tmux_snapshot(Duration::from_millis(25), Duration::from_secs(1));
        }
        let mut command =
            self.ezm_command(self.project_root(), args, env_overrides, opener_exit_code)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command
            .spawn()
            .map_err(|error| format!("failed spawning ezm {args:?} ({context}): {error}"))?;
        Ok(EzmBackgroundProcess {
            child: Some(child),
            context: context.to_owned(),
        })
    }

    fn ezm_command(
        &self,
        project_dir: &Path,
        args: &[&str],
        env_overrides: &[(&str, &str)],
        opener_exit_code: i32,
    ) -> Result<Command, String> {
        let state_root = self.work_dir.join("state");
        let config_root = self.work_dir.join("config");
        let home_root = self.work_dir.join("home");

        fs::create_dir_all(&state_root)
            .map_err(|error| format!("failed creating state root: {error}"))?;
        fs::create_dir_all(&config_root)
            .map_err(|error| format!("failed creating config root: {error}"))?;
        fs::create_dir_all(&home_root)
            .map_err(|error| format!("failed creating home root: {error}"))?;

        let current_path = std::env::var("PATH").unwrap_or_default();
        let merged_path = format!("{}:{}", self.fake_bin_dir.display(), current_path);

        let mut command = Command::new(&self.ezm_bin);
        if args.is_empty() && self.verbose_default_launch {
            command.arg("--verbose");
        }
        command.args(args);
        command.current_dir(project_dir);
        command.env_remove("TMUX");
        command.env_remove("TMUX_PANE");
        command.env_remove("EZM_REMOTE_PATH");
        command.env_remove("EZM_REMOTE_SERVER_URL");
        command.env_remove("EZM_USE_TSSH");
        command.env_remove("EZM_USE_MOSH");
        command.env_remove("PERLES_DIR");
        command.env_remove("PERLES_DB");
        command.env_remove("BEADS_DIR");
        command.env_remove("BEADS_DB");
        command.env_remove("OPENCODE_SERVER_URL");
        command.env_remove("OPENCODE_SERVER_PASSWORD");
        command.env_remove("OPENCODE_CONFIG_DIR");
        command.env_remove("OPENCODE_TUI_CONFIG");
        command.env_remove("OPENCODE_TEST_MANAGED_CONFIG_DIR");
        command.env("HOME", &home_root);
        command.env("XDG_STATE_HOME", &state_root);
        command.env("XDG_CONFIG_HOME", &config_root);
        command.env("TMUX_TMPDIR", self.tmux_tmpdir.path());
        command.env("E2E_TMUX_SOCKET", &self.tmux_socket_name);
        command.env("E2E_OPEN_CAPTURE", &self.open_capture_path);
        command.env("E2E_OPEN_EXIT", opener_exit_code.to_string());
        command.env("PATH", merged_path);

        for (key, value) in env_overrides {
            command.env(key, value);
        }

        Ok(command)
    }

    fn build_pty_command(
        &self,
        project_dir: &Path,
        args: &[&str],
        env_overrides: &[(&str, &str)],
        opener_exit_code: i32,
        reset_remote_env: bool,
    ) -> Result<CommandBuilder, String> {
        let state_root = self.work_dir.join("state");
        let config_root = self.work_dir.join("config");
        let home_root = self.work_dir.join("home");

        fs::create_dir_all(&state_root)
            .map_err(|error| format!("failed creating state root: {error}"))?;
        fs::create_dir_all(&config_root)
            .map_err(|error| format!("failed creating config root: {error}"))?;
        fs::create_dir_all(&home_root)
            .map_err(|error| format!("failed creating home root: {error}"))?;

        let current_path = std::env::var("PATH").unwrap_or_default();
        let merged_path = format!("{}:{}", self.fake_bin_dir.display(), current_path);
        let mut command = CommandBuilder::new(
            self.ezm_bin
                .to_str()
                .ok_or_else(|| String::from("ezm binary path is not valid UTF-8"))?,
        );
        for arg in args {
            command.arg(arg);
        }
        command.cwd(project_dir);
        command.env_remove("TMUX");
        command.env_remove("TMUX_PANE");
        command.env_remove("EZM_REMOTE_PATH");
        command.env_remove("EZM_REMOTE_SERVER_URL");
        command.env_remove("EZM_USE_TSSH");
        command.env_remove("EZM_USE_MOSH");
        command.env_remove("PERLES_DIR");
        command.env_remove("PERLES_DB");
        command.env_remove("BEADS_DIR");
        command.env_remove("BEADS_DB");
        command.env("TERM", "xterm-256color");
        if reset_remote_env {
            command.env("EZM_REMOTE_PATH", "");
            command.env("EZM_REMOTE_SERVER_URL", "");
            command.env("OPENCODE_SERVER_URL", "");
            command.env("OPENCODE_SERVER_PASSWORD", "");
            command.env("OPENCODE_CONFIG_DIR", "");
            command.env("OPENCODE_TUI_CONFIG", "");
            command.env("OPENCODE_TEST_MANAGED_CONFIG_DIR", "");
        }

        command.env("HOME", home_root);
        command.env("XDG_STATE_HOME", state_root);
        command.env("XDG_CONFIG_HOME", config_root);
        command.env("TMUX_TMPDIR", self.tmux_tmpdir.path());
        command.env("E2E_TMUX_SOCKET", &self.tmux_socket_name);
        command.env("E2E_OPEN_CAPTURE", &self.open_capture_path);
        command.env("E2E_OPEN_EXIT", opener_exit_code.to_string());
        command.env("PATH", merged_path);

        for (key, value) in env_overrides {
            command.env(key, value);
        }

        Ok(command)
    }

    #[allow(dead_code)]
    #[allow(clippy::too_many_lines)]
    pub fn run_ezm_with_pty_attach_probe(
        &self,
        project_dir: &Path,
        args: &[&str],
        env_overrides: &[(&str, &str)],
        opener_exit_code: i32,
        session_name: &str,
    ) -> Result<PtyAttachProbe, String> {
        let command =
            self.build_pty_command(project_dir, args, env_overrides, opener_exit_code, true)?;

        let clients_before = self
            .tmux_capture(&["list-clients", "-F", "#{client_tty}"])
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|tty| !tty.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();

        let pty = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("failed creating PTY pair: {error}"))?;

        let mut child = pty
            .slave
            .spawn_command(command)
            .map_err(|error| format!("failed spawning PTY child for ezm {args:?}: {error}"))?;

        let mut writer = match pty.master.take_writer() {
            Ok(writer) => writer,
            Err(error) => {
                let message = format!("failed taking attach probe PTY writer: {error}");
                teardown_pty_resources(
                    Some(pty.master),
                    None,
                    &mut *child,
                    None,
                    "attach probe writer setup",
                );
                return Err(message);
            }
        };

        let mut reader = match pty.master.try_clone_reader() {
            Ok(reader) => reader,
            Err(error) => {
                let message = format!("failed cloning attach probe PTY reader: {error}");
                teardown_pty_resources(
                    Some(pty.master),
                    Some(writer),
                    &mut *child,
                    None,
                    "attach probe reader setup",
                );
                return Err(message);
            }
        };
        let terminal_output = Arc::new(Mutex::new(Vec::new()));
        let output_for_reader = Arc::clone(&terminal_output);
        let reader_thread = thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        let Ok(mut output) = output_for_reader.lock() else {
                            break;
                        };
                        append_terminal_output(&mut output, &buffer[..count]);
                    }
                }
            }
        });

        let mut observed_attached_client = false;
        let mut observed_client_tty = None;
        let mut last_clients = String::new();
        let mut last_client_query_error = None;
        let mut last_readiness_error = None;
        let mut last_detach_result = String::from("not attempted");
        let start = Instant::now();
        let poll_interval = Duration::from_millis(10);

        let stop_reason = loop {
            if !observed_attached_client {
                let (clients, query_error) = match self.tmux_capture(&[
                    "list-clients",
                    "-F",
                    "#{session_name}|#{client_tty}",
                ]) {
                    Ok(clients) => (clients, None),
                    Err(error) => (String::new(), Some(error)),
                };
                last_clients.clone_from(&clients);
                last_client_query_error = query_error;
                let attached_client_tty = clients.lines().find_map(|line| {
                    let (attached_session, client_tty) = line.split_once('|')?;
                    let client_tty = client_tty.trim();
                    (attached_session.trim() == session_name
                        && !client_tty.is_empty()
                        && !clients_before.iter().any(|existing| existing == client_tty))
                    .then(|| client_tty.to_owned())
                });
                if let Some(client_tty) = attached_client_tty {
                    match self.wait_for_tmux_client_input_ready(
                        &client_tty,
                        &mut *writer,
                        PTY_INPUT_READY_TIMEOUT,
                    ) {
                        Ok(()) => {
                            observed_attached_client = true;
                            observed_client_tty = Some(client_tty.clone());
                            last_readiness_error = None;
                            last_detach_result = self
                                .tmux_capture(&["detach-client", "-t", &client_tty])
                                .map_or_else(
                                    |error| format!("detach failed: {error}"),
                                    |_| String::from("detached observed ready client"),
                                );
                        }
                        Err(error) => last_readiness_error = Some(error),
                    }
                }
            }

            if child
                .try_wait()
                .map_err(|error| format!("failed waiting for PTY child status: {error}"))?
                .is_some()
            {
                break if observed_attached_client {
                    String::from("pty child exited after attached-client predicate")
                } else {
                    String::from("pty child exited before attached-client predicate")
                };
            }

            if start.elapsed() >= PTY_ATTACH_TIMEOUT {
                if let Some(pid) = child.process_id() {
                    let _ = Command::new("kill")
                        .arg("-TERM")
                        .arg(pid.to_string())
                        .status();
                }
                let _ = child.kill();
                break String::from("attach probe deadline expired");
            }

            thread::sleep(poll_interval);
        };

        drop(writer);
        drop(pty.master);
        join_reader_thread(Some(reader_thread));

        let exit_code = wait_for_pty_child_exit(
            &mut *child,
            Duration::from_secs(5),
            poll_interval,
            "attach probe",
        )?;

        Ok(PtyAttachProbe {
            exit_code,
            observed_attached_client,
            diagnostics: format!(
                "stop_reason={stop_reason}; clients_before={clients_before:?}; last_clients={last_clients:?}; client_query_error={last_client_query_error:?}; readiness_error={last_readiness_error:?}; observed_client_tty={observed_client_tty:?}; detach={last_detach_result}; terminal_output={:?}",
                terminal_output_for_diagnostic(&terminal_output)
            ),
        })
    }

    #[allow(dead_code)]
    #[allow(clippy::too_many_lines)]
    pub fn run_ezm_with_pty_interrupt(
        &self,
        project_dir: &Path,
        args: &[&str],
        env_overrides: &[(&str, &str)],
        opener_exit_code: i32,
        session_name: &str,
    ) -> Result<PtyInterruptProbe, String> {
        let command =
            self.build_pty_command(project_dir, args, env_overrides, opener_exit_code, false)?;

        let pty = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("failed creating PTY pair: {error}"))?;

        let mut child = pty
            .slave
            .spawn_command(command)
            .map_err(|error| format!("failed spawning PTY child for ezm {args:?}: {error}"))?;

        let mut observed_attached_client = false;
        let mut signal_sent = false;
        let start = Instant::now();
        let timeout = Duration::from_secs(5);
        let poll_interval = Duration::from_millis(30);
        let signal_fallback_delay = Duration::from_millis(500);

        loop {
            if !observed_attached_client {
                observed_attached_client = self
                    .tmux_capture(&["list-clients", "-F", "#{session_name}|#{client_tty}"])
                    .ok()
                    .is_some_and(|clients| {
                        clients.lines().any(|line| {
                            let Some((attached_session, client_tty)) = line.split_once('|') else {
                                return false;
                            };

                            attached_session.trim() == session_name && !client_tty.trim().is_empty()
                        })
                    });
            }

            if !signal_sent
                && (observed_attached_client || start.elapsed() >= signal_fallback_delay)
            {
                if let Some(pid) = child.process_id() {
                    signal_sent = Command::new("kill")
                        .arg("-INT")
                        .arg(pid.to_string())
                        .status()
                        .is_ok_and(|status| status.success());
                }
            }

            if child
                .try_wait()
                .map_err(|error| format!("failed waiting for PTY child status: {error}"))?
                .is_some()
            {
                break;
            }

            if start.elapsed() >= timeout {
                if let Some(pid) = child.process_id() {
                    let _ = Command::new("kill")
                        .arg("-TERM")
                        .arg(pid.to_string())
                        .status();
                }
                let _ = child.kill();
                break;
            }

            thread::sleep(poll_interval);
        }

        drop(pty.master);

        let exit_code = wait_for_pty_child_exit(
            &mut *child,
            Duration::from_secs(5),
            poll_interval,
            "interrupt probe",
        )?;

        Ok(PtyInterruptProbe {
            exit_code,
            observed_attached_client,
            signal_sent,
        })
    }

    pub fn settle_tmux_snapshot(
        &self,
        poll_interval: Duration,
        timeout: Duration,
    ) -> Result<TmuxSettleEvidence, String> {
        let mut attempts = 0_u32;
        let mut previous: Option<(String, String, String)> = None;
        let start = Instant::now();

        loop {
            attempts += 1;
            let current = (
                self.tmux_list("list-sessions", &["-F", "#{session_name}"])?
                    .trim()
                    .to_owned(),
                self.tmux_list(
                    "list-windows",
                    &["-a", "-F", "#{session_name}:#{window_index}:#{window_name}"],
                )?
                .trim()
                .to_owned(),
                self.tmux_list(
                    "list-panes",
                    &[
                        "-a",
                        "-F",
                        "#{session_name}:#{window_index}.#{pane_index}:#{pane_width}x#{pane_height}",
                    ],
                )?
                .trim()
                .to_owned(),
            );

            if previous.as_ref() == Some(&current) {
                return Ok(TmuxSettleEvidence {
                    attempts,
                    poll_interval_ms: duration_to_millis_u64(poll_interval),
                    timeout_ms: duration_to_millis_u64(timeout),
                    stable: true,
                    sessions: current.0,
                    windows: current.1,
                    panes: current.2,
                });
            }

            previous = Some(current);
            if start.elapsed() >= timeout {
                let (sessions, windows, panes) = previous.unwrap_or_default();
                return Ok(TmuxSettleEvidence {
                    attempts,
                    poll_interval_ms: duration_to_millis_u64(poll_interval),
                    timeout_ms: duration_to_millis_u64(timeout),
                    stable: false,
                    sessions,
                    windows,
                    panes,
                });
            }

            thread::sleep(poll_interval);
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn spawn_tmux_client(
        &self,
        session_name: &str,
        rows: u16,
        cols: u16,
    ) -> Result<PtyTmuxClient, String> {
        let clients_before = self
            .tmux_capture(&["list-clients", "-F", "#{client_tty}"])
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|tty| !tty.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let pty = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("failed creating tmux client PTY: {error}"))?;

        let mut command = CommandBuilder::new(
            self.tmux_bin
                .to_str()
                .ok_or_else(|| String::from("tmux binary path is not valid UTF-8"))?,
        );
        command.args([
            "-S",
            &self.tmux_socket_name,
            "-f",
            "/dev/null",
            "attach-session",
            "-t",
            session_name,
        ]);
        command.env("TMUX_TMPDIR", self.tmux_tmpdir.path());
        command.env("TERM", "xterm-256color");
        command.env_remove("TMUX");
        command.env_remove("TMUX_PANE");

        let mut child = pty
            .slave
            .spawn_command(command)
            .map_err(|error| format!("failed spawning tmux client PTY: {error}"))?;
        let mut reader = match pty.master.try_clone_reader() {
            Ok(reader) => reader,
            Err(error) => {
                let message = format!("failed cloning tmux client PTY reader: {error}");
                teardown_pty_resources(
                    Some(pty.master),
                    None,
                    &mut *child,
                    None,
                    "tmux client reader setup",
                );
                return Err(message);
            }
        };
        let terminal_output = Arc::new(Mutex::new(Vec::new()));
        let output_for_reader = Arc::clone(&terminal_output);
        let reader_thread = thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        let Ok(mut output) = output_for_reader.lock() else {
                            break;
                        };
                        append_terminal_output(&mut output, &buffer[..count]);
                    }
                }
            }
        });
        let mut writer = match pty.master.take_writer() {
            Ok(writer) => writer,
            Err(error) => {
                let message = format!("failed taking tmux client PTY writer: {error}");
                teardown_pty_resources(
                    Some(pty.master),
                    None,
                    &mut *child,
                    Some(reader_thread),
                    "tmux client writer setup",
                );
                return Err(message);
            }
        };

        let deadline = Instant::now() + PTY_ATTACH_TIMEOUT;
        loop {
            let clients = self
                .tmux_capture(&["list-clients", "-F", "#{session_name}|#{client_tty}"])
                .unwrap_or_default();
            let attached_client_tty = clients.lines().find_map(|line| {
                let (attached_session, client_tty) = line.split_once('|')?;
                let client_tty = client_tty.trim();
                (attached_session.trim() == session_name
                    && !client_tty.is_empty()
                    && !clients_before.iter().any(|existing| existing == client_tty))
                .then(|| client_tty.to_owned())
            });
            if let Some(client_tty) = attached_client_tty {
                if let Err(error) = self.wait_for_tmux_client_input_ready(
                    &client_tty,
                    &mut *writer,
                    PTY_INPUT_READY_TIMEOUT,
                ) {
                    teardown_pty_resources(
                        Some(pty.master),
                        Some(writer),
                        &mut *child,
                        Some(reader_thread),
                        "tmux client input readiness failure",
                    );
                    let terminal_output = terminal_output_for_diagnostic(&terminal_output);
                    return Err(format!(
                        "attached tmux client for session {session_name:?} never accepted PTY input: {error}; clients={clients}; terminal_output={terminal_output:?}"
                    ));
                }
                return Ok(PtyTmuxClient {
                    client_tty,
                    tmux_bin: self.tmux_bin.clone(),
                    tmux_socket_name: self.tmux_socket_name.clone(),
                    tmux_tmpdir: self.tmux_tmpdir.path().to_owned(),
                    master: Some(pty.master),
                    child: Some(child),
                    writer: Some(writer),
                    terminal_output,
                    reader_thread: Some(reader_thread),
                });
            }
            if child
                .try_wait()
                .map_err(|error| format!("failed checking tmux client PTY status: {error}"))?
                .is_some()
            {
                teardown_pty_resources(
                    Some(pty.master),
                    Some(writer),
                    &mut *child,
                    Some(reader_thread),
                    "tmux client exited before readiness",
                );
                let terminal_output = terminal_output_for_diagnostic(&terminal_output);
                return Err(format!(
                    "tmux client exited before becoming ready for session {session_name:?}; clients={clients}; terminal_output={terminal_output:?}"
                ));
            }
            if Instant::now() >= deadline {
                teardown_pty_resources(
                    Some(pty.master),
                    Some(writer),
                    &mut *child,
                    Some(reader_thread),
                    "tmux client attach timeout",
                );
                let terminal_output = terminal_output_for_diagnostic(&terminal_output);
                return Err(format!(
                    "timed out waiting for attached tmux client for session {session_name:?}; clients={clients}; terminal_output={terminal_output:?}"
                ));
            }
            thread::sleep(Duration::from_millis(30));
        }
    }

    fn wait_for_tmux_client_input_ready(
        &self,
        client_tty: &str,
        writer: &mut dyn Write,
        timeout: Duration,
    ) -> Result<(), String> {
        let token = format!("{}:{client_tty}", self.run_id);
        self.tmux_capture(&[
            "bind-key",
            "-T",
            E2E_READY_TABLE,
            "r",
            "set-option",
            "-g",
            E2E_READY_OPTION,
            &token,
        ])?;

        let result = (|| {
            let deadline = Instant::now() + timeout;
            loop {
                self.tmux_capture(&["switch-client", "-c", client_tty, "-T", E2E_READY_TABLE])?;
                writer
                    .write_all(b"r")
                    .and_then(|()| writer.flush())
                    .map_err(|error| {
                        format!("failed writing tmux client readiness key: {error}")
                    })?;

                if self
                    .tmux_capture(&["show-options", "-gv", E2E_READY_OPTION])
                    .is_ok_and(|value| value.trim() == token)
                {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "timed out waiting for exact client {client_tty:?} to consume readiness key"
                    ));
                }
                thread::sleep(Duration::from_millis(20));
            }
        })();

        let _ = self.tmux_capture(&["switch-client", "-c", client_tty, "-T", "root"]);
        let _ = self.tmux_capture(&["unbind-key", "-T", E2E_READY_TABLE, "r"]);
        let _ = self.tmux_capture(&["set-option", "-gu", E2E_READY_OPTION]);
        result
    }

    #[allow(dead_code)]
    pub fn tmux_capture(&self, args: &[&str]) -> Result<String, String> {
        self.tmux_raw(args)
    }

    fn start_tmux_server(&self) -> Result<(), String> {
        self.tmux_raw(&["start-server"])?;
        self.tmux_raw(&[
            "new-session",
            "-d",
            "-s",
            E2E_ANCHOR_SESSION,
            "sh",
            "-lc",
            "sleep 3600",
        ])?;
        Ok(())
    }

    fn start_tmux_watchdog(&mut self) -> Result<(), String> {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(
                "while [ -S \"$2\" ] && kill -0 \"$1\" 2>/dev/null; do sleep \"$5\"; done; if [ -S \"$2\" ]; then TMUX_TMPDIR=\"$4\" \"$3\" -S \"$2\" -f /dev/null kill-server >/dev/null 2>&1; fi; rm -f -- \"$2\"",
            )
            .arg("ezm-tmux-watchdog")
            .arg(std::process::id().to_string())
            .arg(&self.tmux_socket_name)
            .arg(&self.tmux_bin)
            .arg(self.tmux_tmpdir.path())
            .arg(TMUX_WATCHDOG_POLL_INTERVAL)
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        command.process_group(0);
        let child = command
            .spawn()
            .map_err(|error| format!("failed starting isolated tmux watchdog: {error}"))?;
        self.tmux_watchdog = Some(child);
        Ok(())
    }

    fn tmux_list(&self, command_name: &str, args: &[&str]) -> Result<String, String> {
        let mut full_args = Vec::with_capacity(args.len() + 1);
        full_args.push(command_name);
        full_args.extend_from_slice(args);

        self.tmux_raw(&full_args)
            .map_err(|error| format!("{command_name} failed: {error}"))
    }

    fn tmux_raw(&self, args: &[&str]) -> Result<String, String> {
        let output = Command::new(&self.tmux_bin)
            .arg("-S")
            .arg(&self.tmux_socket_name)
            .arg("-f")
            .arg("/dev/null")
            .args(args)
            .env("TMUX_TMPDIR", self.tmux_tmpdir.path())
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .output()
            .map_err(|error| format!("failed running tmux {args:?}: {error}"))?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
        }

        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

impl Drop for FoundationHarness {
    fn drop(&mut self) {
        if let Some(mut watchdog) = self.tmux_watchdog.take() {
            terminate_background_child(&mut watchdog, "isolated tmux watchdog");
        }

        let _ = Command::new(&self.tmux_bin)
            .arg("-S")
            .arg(&self.tmux_socket_name)
            .arg("-f")
            .arg("/dev/null")
            .arg("kill-server")
            .env("TMUX_TMPDIR", self.tmux_tmpdir.path())
            .env_remove("TMUX")
            .env_remove("TMUX_PANE")
            .output();

        let deadline = Instant::now() + TMUX_SERVER_TEARDOWN_TIMEOUT;
        while self.tmux_socket_path().exists() && Instant::now() < deadline {
            thread::sleep(TMUX_SERVER_TEARDOWN_POLL_INTERVAL);
        }
        let _ = fs::remove_file(&self.tmux_socket_name);
    }
}

fn build_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seq = next_unique_sequence();
    format!("run-{nanos:x}-{:x}-{:x}", std::process::id(), seq)
}

fn create_short_private_tmux_dir() -> Result<TempDir, String> {
    let mut roots = vec![PathBuf::from("/tmp")];
    let default_root = std::env::temp_dir();
    if !roots.iter().any(|root| root == &default_root) {
        roots.push(default_root);
    }

    for root in roots {
        let canonical_root = match root.canonicalize() {
            Ok(root) if root.is_dir() => root,
            _ => continue,
        };
        let Ok(directory) = TempDirBuilder::new()
            .prefix("e")
            .tempdir_in(&canonical_root)
        else {
            continue;
        };
        let effective_socket_path = directory
            .path()
            .canonicalize()
            .unwrap_or_else(|_| directory.path().to_path_buf())
            .join("s");
        if effective_socket_path.as_os_str().to_string_lossy().len() <= MAX_TMUX_SOCKET_PATH_LEN {
            return Ok(directory);
        }
    }

    Err(format!(
        "could not create a private tmux directory with socket path <= {MAX_TMUX_SOCKET_PATH_LEN} bytes"
    ))
}

fn append_terminal_output(output: &mut Vec<u8>, chunk: &[u8]) {
    output.extend_from_slice(chunk);
    if output.len() > MAX_TERMINAL_OUTPUT {
        let discard = output.len() - MAX_TERMINAL_OUTPUT;
        output.drain(..discard);
    }
}

fn terminal_output_for_diagnostic(output: &Arc<Mutex<Vec<u8>>>) -> String {
    output.lock().map_or_else(
        |_| String::from("terminal output unavailable (reader lock poisoned)"),
        |output| String::from_utf8_lossy(&output).into_owned(),
    )
}

fn teardown_pty_resources(
    master: Option<Box<dyn MasterPty>>,
    writer: Option<Box<dyn Write + Send>>,
    child: &mut dyn PtyChild,
    reader_thread: Option<JoinHandle<()>>,
    context: &str,
) {
    let _ = child.kill();
    drop(writer);
    drop(master);

    if let Err(error) = wait_for_pty_child_exit(
        child,
        PTY_TEARDOWN_TIMEOUT,
        Duration::from_millis(10),
        context,
    ) {
        eprintln!("foundation harness: {error}");
    }
    join_reader_thread(reader_thread);
}

fn terminate_background_child(child: &mut Child, context: &str) {
    let _ = child.kill();
    let deadline = Instant::now() + PTY_TEARDOWN_TIMEOUT;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                eprintln!("foundation harness: failed checking {context} during cleanup: {error}");
                return;
            }
        }
    }
    eprintln!(
        "foundation harness: timed out cleaning up {context} (pid={})",
        child.id()
    );
}

fn join_reader_thread(reader_thread: Option<JoinHandle<()>>) {
    let Some(reader_thread) = reader_thread else {
        return;
    };

    let deadline = Instant::now() + PTY_TEARDOWN_TIMEOUT;
    while !reader_thread.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }

    if reader_thread.is_finished() {
        let _ = reader_thread.join();
    }
}

fn wait_for_pty_child_exit(
    child: &mut dyn PtyChild,
    timeout: Duration,
    poll_interval: Duration,
    context: &str,
) -> Result<i32, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(exit_status) = child
            .try_wait()
            .map_err(|error| format!("failed checking PTY child status ({context}): {error}"))?
        {
            return Ok(i32::try_from(exit_status.exit_code()).unwrap_or(i32::MAX));
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err(format!(
                "timed out waiting for PTY child to exit ({context}) after {} ms",
                timeout.as_millis()
            ));
        }

        thread::sleep(poll_interval);
    }
}

fn next_unique_sequence() -> u64 {
    static UNIQUE_SEQ: AtomicU64 = AtomicU64::new(0);
    UNIQUE_SEQ.fetch_add(1, Ordering::Relaxed)
}

fn install_fake_opener_scripts(fake_bin_dir: &Path) -> Result<(), String> {
    write_executable(
        &fake_bin_dir.join("xdg-open"),
        "#!/usr/bin/env sh\nprintf '%s' \"$1\" > \"${E2E_OPEN_CAPTURE}\"\nexit \"${E2E_OPEN_EXIT:-0}\"\n",
    )?;
    write_executable(
        &fake_bin_dir.join("open"),
        "#!/usr/bin/env sh\nprintf '%s' \"$1\" > \"${E2E_OPEN_CAPTURE}\"\nexit \"${E2E_OPEN_EXIT:-0}\"\n",
    )?;
    Ok(())
}

fn install_tmux_wrapper(fake_bin_dir: &Path, real_tmux_bin: &Path) -> Result<(), String> {
    let script = format!(
        "#!/usr/bin/env sh\nexec '{}' -S \"${{E2E_TMUX_SOCKET}}\" -f /dev/null \"$@\"\n",
        real_tmux_bin.display()
    );
    write_executable(&fake_bin_dir.join("tmux"), &script)
}

fn write_executable(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content)
        .map_err(|error| format!("failed writing script {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(path)
            .map_err(|error| format!("failed reading metadata for {}: {error}", path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).map_err(|error| {
            format!(
                "failed setting executable mode for {}: {error}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn resolve_tool_path(tool: &str) -> Result<PathBuf, String> {
    let output = Command::new("which")
        .arg("-a")
        .arg(tool)
        .output()
        .map_err(|error| format!("failed resolving `{tool}`: {error}"))?;

    if !output.status.success() {
        return Err(format!(
            "required tool `{tool}` is not available: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let candidates = String::from_utf8_lossy(&output.stdout);
    for candidate in candidates
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if candidate.contains("/shims/") {
            continue;
        }

        let probe = Command::new(candidate)
            .arg("-V")
            .output()
            .map_err(|error| format!("failed probing `{tool}` candidate {candidate}: {error}"))?;
        if probe.status.success() {
            return Ok(PathBuf::from(candidate));
        }
    }

    Err(format!(
        "required tool `{tool}` is available in PATH but no candidate responded to -V"
    ))
}

fn resolve_ezm_bin(project_root: &Path) -> Result<PathBuf, String> {
    static EZM_BIN: OnceLock<Result<PathBuf, String>> = OnceLock::new();

    EZM_BIN
        .get_or_init(|| resolve_ezm_bin_once(project_root))
        .clone()
}

fn resolve_ezm_bin_once(project_root: &Path) -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_ezm") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    let status = Command::new("cargo")
        .arg("build")
        .arg("--bin")
        .arg("ezm")
        .current_dir(project_root)
        .status()
        .map_err(|error| format!("failed building ezm binary for E2E tests: {error}"))?;

    if !status.success() {
        return Err(String::from(
            "`cargo build --bin ezm` failed while preparing E2E harness",
        ));
    }

    let candidate = project_root.join("target").join("debug").join("ezm");
    if candidate.exists() {
        return Ok(candidate);
    }

    Err(format!(
        "ezm binary not found at expected path {}",
        candidate.display()
    ))
}

fn duration_to_millis_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::process::Command;

    use super::{FoundationHarness, MAX_TERMINAL_OUTPUT, append_terminal_output};

    #[test]
    fn terminal_output_buffer_handles_multibyte_read_and_truncation_boundaries() {
        let mut output = Vec::new();
        let euro_sign = [0xf0, 0x9f, 0x92, 0xa9];

        append_terminal_output(&mut output, &euro_sign[..1]);
        append_terminal_output(&mut output, &euro_sign[1..3]);
        append_terminal_output(&mut output, &euro_sign[3..]);
        append_terminal_output(&mut output, &vec![b'x'; MAX_TERMINAL_OUTPUT - 3]);

        assert_eq!(output.len(), MAX_TERMINAL_OUTPUT);
        assert_eq!(&output[..3], &euro_sign[1..]);

        let rendered = String::from_utf8_lossy(&output);
        assert!(rendered.contains('\u{fffd}'));
        assert!(rendered.ends_with(&"x".repeat(MAX_TERMINAL_OUTPUT - 3)));
    }

    #[test]
    fn concurrent_harnesses_use_distinct_sockets_and_cleanup_only_owned_state() {
        let first =
            FoundationHarness::new_for_suite("harness-isolation").expect("first isolated harness");
        let second =
            FoundationHarness::new_for_suite("harness-isolation").expect("second isolated harness");
        let first_socket = first.tmux_socket_path().to_owned();
        let second_socket = second.tmux_socket_path().to_owned();

        assert_ne!(first_socket, second_socket);
        first
            .tmux_capture(&["new-session", "-d", "-s", "first-owned", "sleep", "60"])
            .expect("first harness session");
        second
            .tmux_capture(&["new-session", "-d", "-s", "second-owned", "sleep", "60"])
            .expect("second harness session");

        first
            .reset_scenario_state()
            .expect("first harness scenario cleanup");
        assert!(
            first
                .tmux_capture(&["has-session", "-t", "first-owned"])
                .is_err()
        );
        assert!(
            second
                .tmux_capture(&["has-session", "-t", "second-owned"])
                .is_ok()
        );

        drop(first);
        assert!(!first_socket.exists());
        assert!(second.tmux_capture(&["list-sessions"]).is_ok());
        assert!(second_socket.exists());
    }

    #[test]
    fn panic_cleanup_stops_only_the_panicking_harness_server() {
        let survivor = FoundationHarness::new_for_suite("harness-isolation")
            .expect("surviving isolated harness");
        let survivor_socket = survivor.tmux_socket_path().to_owned();

        let mut panicking_socket = None;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let panicking = FoundationHarness::new_for_suite("harness-isolation")
                .expect("panicking isolated harness");
            panicking_socket = Some(panicking.tmux_socket_path().to_owned());
            assert!(
                panicking_socket
                    .as_ref()
                    .is_some_and(|socket| socket.exists())
            );
            panic!("exercise harness cleanup during assertion unwinding");
        }));
        assert!(result.is_err());

        let panicking_socket = panicking_socket.expect("panicking harness socket path");
        assert!(!panicking_socket.exists());
        assert!(survivor_socket.exists());
        assert!(survivor.tmux_capture(&["list-sessions"]).is_ok());
    }

    #[test]
    fn completed_harness_removes_its_exact_server_and_socket() {
        let (tmux_bin, socket) = {
            let harness =
                FoundationHarness::new_for_suite("harness-lifecycle").expect("isolated harness");
            harness
                .tmux_capture(&["new-session", "-d", "-s", "owned-project", "sleep", "60"])
                .expect("owned project session");
            harness
                .tmux_capture(&["new-window", "-t", "owned-project", "sleep", "60"])
                .expect("second owned project window");
            assert_eq!(
                harness
                    .tmux_capture(&["list-windows", "-t", "owned-project"])
                    .expect("owned project windows")
                    .lines()
                    .count(),
                2
            );
            (
                harness.tmux_bin.clone(),
                harness.tmux_socket_path().to_owned(),
            )
        };

        assert!(!socket.exists(), "owned socket survived harness completion");
        assert!(
            !Command::new(tmux_bin)
                .arg("-S")
                .arg(&socket)
                .arg("-f")
                .arg("/dev/null")
                .arg("list-sessions")
                .status()
                .expect("probe exact owned server")
                .success()
        );
    }
}
