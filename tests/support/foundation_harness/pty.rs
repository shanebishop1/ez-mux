use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use portable_pty::{Child as PtyChild, CommandBuilder, MasterPty, PtySize, native_pty_system};

use super::{
    FoundationHarness, MAX_TERMINAL_OUTPUT, PTY_ATTACH_TIMEOUT, PTY_INPUT_READY_TIMEOUT,
    PTY_TEARDOWN_TIMEOUT, PtyAttachProbe, PtyInterruptProbe,
};

impl FoundationHarness {
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
        ] {
            command.env_remove(key);
        }
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
        let reader = match pty.master.try_clone_reader() {
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
        let reader_thread = start_reader_thread(reader, &terminal_output);

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
        after_attach: impl FnOnce() -> Result<(), String>,
    ) -> Result<PtyInterruptProbe, String> {
        let command =
            self.build_pty_command(project_dir, args, env_overrides, opener_exit_code, false)?;
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
                let message = format!("failed taking interrupt probe PTY writer: {error}");
                teardown_pty_resources(
                    Some(pty.master),
                    None,
                    &mut *child,
                    None,
                    "interrupt probe writer setup",
                );
                return Err(message);
            }
        };
        let reader = match pty.master.try_clone_reader() {
            Ok(reader) => reader,
            Err(error) => {
                let message = format!("failed cloning interrupt probe PTY reader: {error}");
                teardown_pty_resources(
                    Some(pty.master),
                    Some(writer),
                    &mut *child,
                    None,
                    "interrupt probe reader setup",
                );
                return Err(message);
            }
        };
        let terminal_output = Arc::new(Mutex::new(Vec::new()));
        let reader_thread = start_reader_thread(reader, &terminal_output);

        let mut observed_attached_client = false;
        let mut observed_client_tty = None;
        let mut signal_sent = false;
        let mut after_attach = Some(after_attach);
        let attach_deadline = Instant::now() + PTY_ATTACH_TIMEOUT;
        let mut exit_deadline = None;
        let poll_interval = Duration::from_millis(10);
        let mut last_clients = String::new();

        loop {
            if !observed_attached_client {
                let clients = self
                    .tmux_capture(&["list-clients", "-F", "#{session_name}|#{client_tty}"])
                    .unwrap_or_default();
                last_clients.clone_from(&clients);
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
                            "interrupt probe client readiness failure",
                        );
                        let terminal_output = terminal_output_for_diagnostic(&terminal_output);
                        return Err(format!(
                            "interrupt probe client {client_tty:?} never accepted PTY input: {error}; clients={clients:?}; terminal_output={terminal_output:?}"
                        ));
                    }
                    observed_attached_client = true;
                    if let Some(callback) = after_attach.take() {
                        if let Err(error) = callback() {
                            teardown_pty_resources(
                                Some(pty.master),
                                Some(writer),
                                &mut *child,
                                Some(reader_thread),
                                "interrupt probe attached callback failure",
                            );
                            let terminal_output = terminal_output_for_diagnostic(&terminal_output);
                            return Err(format!(
                                "interrupt probe attached callback failed: {error}; terminal_output={terminal_output:?}"
                            ));
                        }
                    }
                    observed_client_tty = Some(client_tty);
                    if let Some(pid) = child.process_id() {
                        signal_sent = Command::new("kill")
                            .arg("-INT")
                            .arg(pid.to_string())
                            .status()
                            .is_ok_and(|status| status.success());
                    }
                    exit_deadline = Some(Instant::now() + Duration::from_secs(10));
                }
            }
            if child
                .try_wait()
                .map_err(|error| format!("failed waiting for PTY child status: {error}"))?
                .is_some()
            {
                break;
            }
            let deadline_expired = exit_deadline.map_or_else(
                || Instant::now() >= attach_deadline,
                |deadline| Instant::now() >= deadline,
            );
            if deadline_expired {
                teardown_pty_resources(
                    Some(pty.master),
                    Some(writer),
                    &mut *child,
                    Some(reader_thread),
                    "interrupt probe deadline",
                );
                let terminal_output = terminal_output_for_diagnostic(&terminal_output);
                return Err(format!(
                    "interrupt probe deadline expired; attached={observed_attached_client}; signal_sent={signal_sent}; clients={last_clients:?}; terminal_output={terminal_output:?}"
                ));
            }
            thread::sleep(poll_interval);
        }

        drop(writer);
        drop(pty.master);
        join_reader_thread(Some(reader_thread));
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
            diagnostics: format!(
                "clients_before={clients_before:?}; last_clients={last_clients:?}; observed_client_tty={observed_client_tty:?}; terminal_output={:?}",
                terminal_output_for_diagnostic(&terminal_output)
            ),
        })
    }
}

pub(super) fn append_terminal_output(output: &mut Vec<u8>, chunk: &[u8]) {
    output.extend_from_slice(chunk);
    if output.len() > MAX_TERMINAL_OUTPUT {
        let discard = output.len() - MAX_TERMINAL_OUTPUT;
        output.drain(..discard);
    }
}

pub(super) fn terminal_output_for_diagnostic(output: &Arc<Mutex<Vec<u8>>>) -> String {
    output.lock().map_or_else(
        |_| String::from("terminal output unavailable (reader lock poisoned)"),
        |output| String::from_utf8_lossy(&output).into_owned(),
    )
}

fn start_reader_thread(
    reader: Box<dyn Read + Send>,
    terminal_output: &Arc<Mutex<Vec<u8>>>,
) -> JoinHandle<()> {
    let output_for_reader = Arc::clone(terminal_output);
    thread::spawn(move || {
        let mut reader = reader;
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
    })
}

pub(super) fn teardown_pty_resources(
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

pub(super) fn join_reader_thread(reader_thread: Option<JoinHandle<()>>) {
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

pub(super) fn wait_for_pty_child_exit(
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
