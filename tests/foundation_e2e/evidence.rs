use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

pub(crate) const FOUNDATION_IDS: [&str; 6] =
    ["E2E-00", "E2E-15", "E2E-17", "E2E-18", "E2E-19", "E2E-20"];

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
pub(crate) struct CaseEvidence {
    pub(crate) id: String,
    pub(crate) pass: bool,
    pub(crate) assertions: Vec<String>,
    pub(crate) samples: Vec<CommandSample>,
    pub(crate) settle: SettleEvidence,
}

#[derive(Serialize)]
pub(crate) struct SuiteEvidence {
    pub(crate) metadata: RunMetadata,
    pub(crate) cases: Vec<CaseEvidence>,
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

pub(crate) fn write_json(path: &PathBuf, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed serializing json for {}: {error}", path.display()))?;
    fs::write(path, json)
        .map_err(|error| format!("failed writing json {}: {error}", path.display()))
}
