use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::support::foundation_harness::{CmdOutput, FoundationHarness, TmuxSettleEvidence};

use super::layout_snapshots::{LayoutSnapshot, SessionSnapshot};
use super::lifecycle::HelperLifecycleEvidence;
use super::slot_snapshots::SlotSnapshot;

pub(crate) const CORE_IDS: [&str; 17] = [
    "E2E-01", "E2E-02", "E2E-03", "E2E-04", "E2E-05", "E2E-06", "E2E-07", "E2E-08", "E2E-09",
    "E2E-10", "E2E-11", "E2E-12", "E2E-13", "E2E-16", "E2E-19", "E2E-20", "E2E-21",
];

#[derive(Serialize)]
pub(crate) struct RunMetadata {
    pub(crate) run_id: String,
    pub(crate) commit_sha: String,
    pub(crate) os: String,
    pub(crate) shell: String,
    pub(crate) tmux_version: String,
    pub(crate) artifact_dir: String,
    pub(crate) test_ids: Vec<String>,
    pub(crate) pass_total: usize,
    pub(crate) fail_total: usize,
}

#[derive(Serialize)]
pub(crate) struct CommandSample {
    pub(crate) args: Vec<String>,
    pub(crate) exit_code: i32,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(Serialize)]
pub(crate) struct SettleEvidence {
    pub(crate) attempts: u32,
    pub(crate) poll_interval_ms: u64,
    pub(crate) timeout_ms: u64,
    pub(crate) stable: bool,
    pub(crate) sessions: String,
    pub(crate) windows: String,
    pub(crate) panes: String,
}

#[derive(Serialize)]
pub(crate) struct RemotePathEvidence {
    pub(crate) local_project_dir: String,
    pub(crate) remote_path: String,
    pub(crate) remote_path_source: String,
    pub(crate) expected_mapped_path: String,
    pub(crate) effective_mapped_path: String,
    pub(crate) remap_applied: bool,
    pub(crate) opencode_attach_url: String,
    pub(crate) opencode_server_url_source: String,
    pub(crate) opencode_server_password_set: bool,
    pub(crate) opencode_server_password_source: String,
}

#[derive(Serialize)]
pub(crate) struct CaseEvidence {
    pub(crate) id: String,
    pub(crate) pass: bool,
    pub(crate) assertions: Vec<String>,
    pub(crate) samples: Vec<CommandSample>,
    pub(crate) settle: SettleEvidence,
    pub(crate) snapshot: SessionSnapshot,
    pub(crate) layout: Option<LayoutSnapshot>,
    pub(crate) slots: Option<Vec<SlotSnapshot>>,
    pub(crate) remote_path: Option<RemotePathEvidence>,
    pub(crate) helper_state: Option<HelperLifecycleEvidence>,
}

#[derive(Serialize)]
pub(crate) struct SuiteEvidence {
    pub(crate) metadata: RunMetadata,
    pub(crate) cases: Vec<CaseEvidence>,
}

pub(crate) fn sample(args: &[&str], output: &CmdOutput) -> CommandSample {
    CommandSample {
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        exit_code: output.exit_code,
        stdout: output.stdout.clone(),
        stderr: output.stderr.clone(),
    }
}

pub(crate) fn settle_snapshot(harness: &FoundationHarness, test_id: &str) -> TmuxSettleEvidence {
    harness
        .settle_tmux_snapshot(
            super::lifecycle::DEFAULT_POLL_INTERVAL,
            super::lifecycle::DEFAULT_TIMEOUT,
        )
        .unwrap_or_else(|error| panic!("{test_id} settle evidence failed: {error}"))
}

pub(crate) fn map_settle(settle: TmuxSettleEvidence) -> SettleEvidence {
    SettleEvidence {
        attempts: settle.attempts,
        poll_interval_ms: settle.poll_interval_ms,
        timeout_ms: settle.timeout_ms,
        stable: settle.stable,
        sessions: settle.sessions,
        windows: settle.windows,
        panes: settle.panes,
    }
}

pub(crate) fn read_commit_sha(project_root: &Path) -> String {
    let output = std::process::Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir(project_root)
        .output();

    match output {
        Ok(result) if result.status.success() => {
            String::from_utf8_lossy(&result.stdout).trim().to_owned()
        }
        _ => String::from("unknown"),
    }
}

pub(crate) fn write_case_artifacts(dir: &Path, cases: &[CaseEvidence]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| format!("failed creating case directory: {error}"))?;
    for case in cases {
        let path = dir.join(format!("{}.json", case.id));
        write_json(&path, case)?;
    }
    Ok(())
}

pub(crate) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed serializing json for {}: {error}", path.display()))?;
    fs::write(path, json)
        .map_err(|error| format!("failed writing json {}: {error}", path.display()))
}
