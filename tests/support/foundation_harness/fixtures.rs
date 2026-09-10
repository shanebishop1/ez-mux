use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use tempfile::{Builder as TempDirBuilder, TempDir};

use super::{CmdOutput, EzmBackgroundProcess, FoundationHarness, MAX_TMUX_SOCKET_PATH_LEN};

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
        let fake_perles_bin_dir = work_dir.join("perles-bin");
        let open_capture_path = work_dir.join("open-latest-arg.txt");

        fs::create_dir_all(&artifact_dir)
            .map_err(|error| format!("failed creating artifact directory: {error}"))?;
        fs::create_dir_all(&fake_bin_dir)
            .map_err(|error| format!("failed creating fake bin directory: {error}"))?;
        fs::create_dir_all(&fake_perles_bin_dir)
            .map_err(|error| format!("failed creating fake perles directory: {error}"))?;

        let tmux_bin = resolve_tool_path("tmux")?;
        let shell = std::env::var("SHELL").unwrap_or_else(|_| String::from("unknown"));
        let ezm_bin = resolve_ezm_bin(&project_root)?;

        install_fake_opener_scripts(&fake_bin_dir)?;
        install_fake_perles_script(&fake_perles_bin_dir)?;
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
            fake_perles_bin_dir,
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

    pub fn path_with_fake_perles(&self) -> String {
        let current_path = std::env::var("PATH").unwrap_or_default();
        format!(
            "{}:{}:{}",
            self.fake_perles_bin_dir.display(),
            self.fake_bin_dir.display(),
            current_path
        )
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
        clear_ezm_environment(&mut command);
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
}

fn clear_ezm_environment(command: &mut Command) {
    for key in [
        "TMUX",
        "TMUX_PANE",
        "EZM_REMOTE_PATH",
        "EZM_REMOTE_SERVER_URL",
        "EZM_USE_TSSH",
        "EZM_USE_MOSH",
        "PERLES_DIR",
        "PERLES_DB",
        "BEADS_DIR",
        "BEADS_DB",
        "OPENCODE_SERVER_URL",
        "OPENCODE_SERVER_PASSWORD",
        "OPENCODE_CONFIG_DIR",
        "OPENCODE_TUI_CONFIG",
        "OPENCODE_TEST_MANAGED_CONFIG_DIR",
    ] {
        command.env_remove(key);
    }
}

fn build_run_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static UNIQUE_SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let seq = UNIQUE_SEQ.fetch_add(1, Ordering::Relaxed);
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

fn install_fake_perles_script(fake_bin_dir: &Path) -> Result<(), String> {
    write_executable(
        &fake_bin_dir.join("perles"),
        "#!/usr/bin/env sh\nexec sleep 3600\n",
    )
}

fn install_tmux_wrapper(fake_bin_dir: &Path, real_tmux_bin: &Path) -> Result<(), String> {
    let script = format!(
        "#!/usr/bin/env sh\nif [ -n \"${{E2E_TMUX_FAIL_MATCH:-}}\" ] && [ -n \"${{E2E_TMUX_FAIL_ONCE:-}}\" ]; then\n  case \" $* \" in\n    *\"${{E2E_TMUX_FAIL_MATCH}}\"*)\n      if mkdir \"${{E2E_TMUX_FAIL_ONCE}}\" 2>/dev/null; then\n        printf '%s\\n' \"injected tmux failure: $E2E_TMUX_FAIL_MATCH\" >&2\n        exit 42\n      fi\n      ;;\n  esac\nfi\nexec '{}' -S \"${{E2E_TMUX_SOCKET}}\" -f /dev/null \"$@\"\n",
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
    use std::sync::OnceLock;
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
