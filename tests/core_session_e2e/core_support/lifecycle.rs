use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ez_mux::session::resolve_session_identity;
use serde::Serialize;

use crate::support::foundation_harness::FoundationHarness;

pub(crate) const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(50);
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize)]
pub(crate) struct HelperStateSnapshot {
    pub(crate) helper_sessions: Vec<String>,
    pub(crate) helper_pane_pids: Vec<u32>,
}

#[derive(Serialize)]
pub(crate) struct HelperLifecycleEvidence {
    pub(crate) before: HelperStateSnapshot,
    pub(crate) after: HelperStateSnapshot,
    pub(crate) pre_helper_pids_alive_after_teardown: Vec<u32>,
}

pub(crate) fn prepare_fresh_create_path(
    harness: &FoundationHarness,
    project_dir: &Path,
) -> Result<String, String> {
    let identity = resolve_session_identity(project_dir)
        .map_err(|error| format!("failed resolving expected session identity: {error}"))?;

    let _ = harness.tmux_capture(&["kill-session", "-t", &identity.session_name]);

    let gone = poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        match harness.tmux_capture(&["has-session", "-t", &identity.session_name]) {
            Ok(_) => Ok(false),
            Err(_) => Ok(true),
        }
    })?;

    if gone {
        Ok(identity.session_name)
    } else {
        Err(format!(
            "expected no existing session `{}` before create-path test",
            identity.session_name
        ))
    }
}

pub(crate) fn popup_helper_session_name(session_name: &str, slot_id: u8) -> String {
    format!("{session_name}__popup_slot_{slot_id}")
}

pub(crate) fn read_helper_state_snapshot(
    harness: &FoundationHarness,
    session_name: &str,
) -> HelperStateSnapshot {
    let helper_prefix = format!("{session_name}__");
    let sessions = harness
        .tmux_capture(&["list-sessions", "-F", "#{session_name}"])
        .unwrap_or_default();
    let mut helper_sessions = sessions
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty() && name.starts_with(&helper_prefix))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    helper_sessions.sort();

    let mut pids = BTreeSet::new();
    for helper_session in &helper_sessions {
        let pane_dump = harness
            .tmux_capture(&["list-panes", "-t", helper_session, "-F", "#{pane_pid}"])
            .unwrap_or_default();
        for pid in pane_dump
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .filter_map(|line| line.parse::<u32>().ok())
        {
            pids.insert(pid);
        }
    }

    HelperStateSnapshot {
        helper_sessions,
        helper_pane_pids: pids.into_iter().collect(),
    }
}

pub(crate) fn helper_pids_alive(pids: &[u32]) -> Vec<u32> {
    pids.iter()
        .copied()
        .filter(|pid| {
            Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        })
        .collect()
}

pub(crate) fn wait_for_helper_pids_to_exit(
    pids: &[u32],
    timeout: Duration,
    poll_interval: Duration,
) -> Result<Vec<u32>, String> {
    let _all_gone = poll_until(timeout, poll_interval, || {
        Ok(helper_pids_alive(pids).is_empty())
    })?;
    Ok(helper_pids_alive(pids))
}

pub(crate) fn poll_until(
    timeout: Duration,
    poll_interval: Duration,
    mut probe: impl FnMut() -> Result<bool, String>,
) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if probe()? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(poll_interval);
    }
}
