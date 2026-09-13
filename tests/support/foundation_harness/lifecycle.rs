use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use super::{
    E2E_ANCHOR_SESSION, FoundationHarness, TMUX_SERVER_TEARDOWN_POLL_INTERVAL,
    TMUX_SERVER_TEARDOWN_TIMEOUT, TMUX_WATCHDOG_POLL_INTERVAL,
};

impl FoundationHarness {
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

            // tmux's asynchronous run-shell jobs and pane descendants are not
            // guaranteed to be reaped when a session is killed.  They retain
            // macOS PTY resources and can exhaust the runner before the next
            // isolated harness starts.  Keep the anchor alive, but terminate
            // every other process owned by this isolated tmux server before
            // removing its sessions.
            self.cleanup_tmux_processes(true)?;

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

    fn cleanup_tmux_processes(&self, preserve_anchor: bool) -> Result<(), String> {
        let server_pid = self.tmux_server_pid()?;
        let mut process_tree = process_descendants(server_pid)?;

        if preserve_anchor {
            let anchor_pid = self
                .tmux_capture(&["list-panes", "-t", E2E_ANCHOR_SESSION, "-F", "#{pane_pid}"])?
                .lines()
                .map(str::trim)
                .find(|pid| !pid.is_empty())
                .ok_or_else(|| {
                    format!("isolated tmux server lost anchor pane for {E2E_ANCHOR_SESSION:?}")
                })?
                .parse::<u32>()
                .map_err(|error| format!("invalid anchor pane pid: {error}"))?;
            let anchor_tree = process_descendants(anchor_pid)?;
            process_tree.retain(|pid| !anchor_tree.contains(pid) && *pid != anchor_pid);
        }

        terminate_processes(&process_tree);
        Ok(())
    }

    fn tmux_server_pid(&self) -> Result<u32, String> {
        self.tmux_capture(&["display-message", "-p", "#{pid}"])?
            .trim()
            .parse()
            .map_err(|error| format!("invalid isolated tmux server pid: {error}"))
    }

    pub(super) fn start_tmux_server(&self) -> Result<(), String> {
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

    pub(super) fn start_tmux_watchdog(&mut self) -> Result<(), String> {
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
}

impl Drop for FoundationHarness {
    fn drop(&mut self) {
        if let Some(mut watchdog) = self.tmux_watchdog.take() {
            super::process::terminate_background_child(&mut watchdog, "isolated tmux watchdog");
        }
        if let Err(error) = self.cleanup_tmux_processes(false) {
            eprintln!("foundation harness: failed cleaning isolated tmux processes: {error}");
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

fn process_descendants(root_pid: u32) -> Result<BTreeSet<u32>, String> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
        .map_err(|error| format!("failed listing process tree for pid {root_pid}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ps failed listing process tree for pid {root_pid}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let mut children = BTreeMap::<u32, Vec<u32>>::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let mut fields = line.split_whitespace();
        let Some(pid) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let Some(parent_pid) = fields.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        children.entry(parent_pid).or_default().push(pid);
    }

    let mut descendants = BTreeSet::new();
    let mut pending = children.remove(&root_pid).unwrap_or_default();
    while let Some(pid) = pending.pop() {
        if !descendants.insert(pid) {
            continue;
        }
        if let Some(grandchildren) = children.remove(&pid) {
            pending.extend(grandchildren);
        }
    }
    Ok(descendants)
}

fn terminate_processes(pids: &BTreeSet<u32>) {
    for pid in pids {
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status();
    }

    let deadline = Instant::now() + super::PTY_TEARDOWN_TIMEOUT;
    let mut remaining = pids.clone();
    while !remaining.is_empty() && Instant::now() < deadline {
        remaining.retain(|pid| {
            Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        });
        if !remaining.is_empty() {
            thread::sleep(Duration::from_millis(10));
        }
    }

    for pid in remaining {
        let _ = Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
}
