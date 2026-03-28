#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use portable_pty::{Child as PtyChild, CommandBuilder, PtySize, native_pty_system};

pub struct CmdOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[allow(dead_code)]
pub struct PtyAttachProbe {
    pub exit_code: i32,
    pub observed_attached_client: bool,
}

#[allow(dead_code)]
pub struct PtyInterruptProbe {
    pub exit_code: i32,
    pub observed_attached_client: bool,
    pub signal_sent: bool,
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
    pub tmux_tmpdir: PathBuf,
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
        let socket_token = short_socket_token();
        let tmux_tmpdir = std::env::temp_dir().join(format!("ezm-e2e-tmux-{socket_token}"));
        let fake_bin_dir = work_dir.join("bin");
        let open_capture_path = work_dir.join("open-latest-arg.txt");

        fs::create_dir_all(&artifact_dir)
            .map_err(|error| format!("failed creating artifact directory: {error}"))?;
        fs::create_dir_all(&tmux_tmpdir)
            .map_err(|error| format!("failed creating tmux temp directory: {error}"))?;
        fs::create_dir_all(&fake_bin_dir)
            .map_err(|error| format!("failed creating fake bin directory: {error}"))?;

        let tmux_bin = resolve_tool_path("tmux")?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| String::from("unknown"));
        let tmux_socket_name = format!("ezm-{socket_token}");
        let ezm_bin = resolve_ezm_bin(&project_root)?;

        install_fake_opener_scripts(&fake_bin_dir)?;
        install_tmux_wrapper(&fake_bin_dir, &tmux_bin)?;

        let harness = Self {
            run_id,
            artifact_dir,
            tmux_socket_name,
            tmux_tmpdir,
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

    pub fn work_dir(&self) -> &Path {
        &self.work_dir
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
            let _ =
                self.settle_tmux_snapshot(Duration::from_millis(25), Duration::from_millis(1_000));
        }

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
            command.arg("-v");
        }
        command.args(args);
        command.current_dir(project_dir);
        command.env_remove("TMUX");
        command.env_remove("EZM_REMOTE_PATH");
        command.env_remove("EZM_REMOTE_SERVER_URL");
        command.env_remove("OPENCODE_SERVER_URL");
        command.env_remove("OPENCODE_SERVER_PASSWORD");
        command.env_remove("OPENCODE_CONFIG_DIR");
        command.env_remove("OPENCODE_TUI_CONFIG");
        command.env_remove("OPENCODE_TEST_MANAGED_CONFIG_DIR");
        command.env("HOME", &home_root);
        command.env("XDG_STATE_HOME", &state_root);
        command.env("XDG_CONFIG_HOME", &config_root);
        command.env("TMUX_TMPDIR", &self.tmux_tmpdir);
        command.env("E2E_TMUX_SOCKET", &self.tmux_socket_name);
        command.env("E2E_OPEN_CAPTURE", &self.open_capture_path);
        command.env("E2E_OPEN_EXIT", opener_exit_code.to_string());
        command.env("PATH", merged_path);

        for (key, value) in env_overrides {
            command.env(key, value);
        }

        let output = command
            .output()
            .map_err(|error| format!("failed running ezm {args:?}: {error}"))?;

        Ok(CmdOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
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
        command.env("TMUX", "");
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
        command.env("TMUX_TMPDIR", &self.tmux_tmpdir);
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
        let start = Instant::now();
        let timeout = Duration::from_secs(5);
        let poll_interval = Duration::from_millis(30);

        loop {
            if !observed_attached_client {
                let attached_client_tty = self
                    .tmux_capture(&["list-clients", "-t", session_name, "-F", "#{client_tty}"])
                    .ok()
                    .and_then(|clients| {
                        clients
                            .lines()
                            .map(str::trim)
                            .find(|line| !line.is_empty())
                            .map(str::to_owned)
                    });
                if let Some(client_tty) = attached_client_tty {
                    observed_attached_client = true;
                    let _ = self.tmux_capture(&["detach-client", "-t", &client_tty]);
                    let _ = self.tmux_capture(&["detach-client", "-a", "-s", session_name]);
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
            "attach probe",
        )?;

        Ok(PtyAttachProbe {
            exit_code,
            observed_attached_client,
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
                        .map(|status| status.success())
                        .unwrap_or(false);
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
            "ezm_e2e_anchor",
            "sh",
            "-lc",
            "sleep 300",
        ])?;
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
            .arg("-L")
            .arg(&self.tmux_socket_name)
            .arg("-f")
            .arg("/dev/null")
            .args(args)
            .env("TMUX_TMPDIR", &self.tmux_tmpdir)
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
        let _ = Command::new(&self.tmux_bin)
            .arg("-L")
            .arg(&self.tmux_socket_name)
            .arg("-f")
            .arg("/dev/null")
            .arg("kill-server")
            .env("TMUX_TMPDIR", &self.tmux_tmpdir)
            .output();
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

fn short_socket_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seq = next_unique_sequence();
    format!(
        "{:x}{:x}{:x}",
        nanos & 0xfffff,
        std::process::id() & 0xffff,
        seq & 0xfff
    )
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
        "#!/usr/bin/env sh\nexec '{}' -L \"${{E2E_TMUX_SOCKET}}\" -f /dev/null \"$@\"\n",
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
