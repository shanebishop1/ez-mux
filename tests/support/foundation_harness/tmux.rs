use std::io::{Read, Write};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use super::{
    E2E_READY_OPTION, E2E_READY_TABLE, FoundationHarness, PTY_ATTACH_TIMEOUT,
    PTY_INPUT_READY_TIMEOUT, PtyTmuxClient, TmuxSettleEvidence,
};

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
        super::pty::terminal_output_for_diagnostic(&self.terminal_output)
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
            super::pty::teardown_pty_resources(
                master,
                writer,
                &mut **child,
                reader_thread,
                "tmux client teardown",
            );
        }
    }
}

impl FoundationHarness {
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
                    poll_interval_ms: super::duration_to_millis_u64(poll_interval),
                    timeout_ms: super::duration_to_millis_u64(timeout),
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
                    poll_interval_ms: super::duration_to_millis_u64(poll_interval),
                    timeout_ms: super::duration_to_millis_u64(timeout),
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
        let reader = match pty.master.try_clone_reader() {
            Ok(reader) => reader,
            Err(error) => {
                let message = format!("failed cloning tmux client PTY reader: {error}");
                super::pty::teardown_pty_resources(
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
            let mut reader = reader;
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        let Ok(mut output) = output_for_reader.lock() else {
                            break;
                        };
                        super::pty::append_terminal_output(&mut output, &buffer[..count]);
                    }
                }
            }
        });
        let mut writer = match pty.master.take_writer() {
            Ok(writer) => writer,
            Err(error) => {
                let message = format!("failed taking tmux client PTY writer: {error}");
                super::pty::teardown_pty_resources(
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
                    super::pty::teardown_pty_resources(
                        Some(pty.master),
                        Some(writer),
                        &mut *child,
                        Some(reader_thread),
                        "tmux client input readiness failure",
                    );
                    let terminal_output =
                        super::pty::terminal_output_for_diagnostic(&terminal_output);
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
                super::pty::teardown_pty_resources(
                    Some(pty.master),
                    Some(writer),
                    &mut *child,
                    Some(reader_thread),
                    "tmux client exited before readiness",
                );
                let terminal_output = super::pty::terminal_output_for_diagnostic(&terminal_output);
                return Err(format!(
                    "tmux client exited before becoming ready for session {session_name:?}; clients={clients}; terminal_output={terminal_output:?}"
                ));
            }
            if Instant::now() >= deadline {
                super::pty::teardown_pty_resources(
                    Some(pty.master),
                    Some(writer),
                    &mut *child,
                    Some(reader_thread),
                    "tmux client attach timeout",
                );
                let terminal_output = super::pty::terminal_output_for_diagnostic(&terminal_output);
                return Err(format!(
                    "timed out waiting for attached tmux client for session {session_name:?}; clients={clients}; terminal_output={terminal_output:?}"
                ));
            }
            thread::sleep(Duration::from_millis(30));
        }
    }

    pub(super) fn wait_for_tmux_client_input_ready(
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

    pub(super) fn tmux_list(&self, command_name: &str, args: &[&str]) -> Result<String, String> {
        let mut full_args = Vec::with_capacity(args.len() + 1);
        full_args.push(command_name);
        full_args.extend_from_slice(args);
        self.tmux_raw(&full_args)
            .map_err(|error| format!("{command_name} failed: {error}"))
    }

    pub(super) fn tmux_raw(&self, args: &[&str]) -> Result<String, String> {
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
