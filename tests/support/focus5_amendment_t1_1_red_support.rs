#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::foundation_harness::FoundationHarness;

#[derive(Clone, Debug)]
pub(super) struct SlotSnapshot {
    pub(super) slot_id: u8,
    pub(super) pane_id: String,
    pub(super) worktree: String,
}

#[derive(Clone, Debug)]
pub(super) struct PaneWidthSnapshot {
    pub(super) pane_id: String,
    pub(super) width: i32,
}

#[derive(Debug)]
pub(super) struct WorktreeFixture {
    pub(super) project_dir: PathBuf,
    pub(super) expected_slot1_worktree: PathBuf,
}

pub(super) fn extract_stdout_field(stdout: &str, key: &str) -> Option<String> {
    let marker = format!("{key}=");
    let start = stdout.find(&marker)? + marker.len();
    let tail = &stdout[start..];
    let end = tail.find(';').unwrap_or(tail.len());
    Some(tail[..end].trim().trim_end_matches('.').to_owned())
}

pub(super) fn read_slot_snapshot(
    harness: &FoundationHarness,
    session: &str,
) -> Result<Vec<SlotSnapshot>, String> {
    let mut slots = Vec::new();
    for slot_id in 1_u8..=5 {
        let pane_key = format!("@ezm_slot_{slot_id}_pane");
        let worktree_key = format!("@ezm_slot_{slot_id}_worktree");
        let pane_id = harness
            .tmux_capture(&["show-options", "-v", "-t", session, &pane_key])?
            .trim()
            .to_owned();
        let worktree = harness
            .tmux_capture(&["show-options", "-v", "-t", session, &worktree_key])?
            .trim()
            .to_owned();

        slots.push(SlotSnapshot {
            slot_id,
            pane_id,
            worktree,
        });
    }
    Ok(slots)
}

pub(super) fn read_pane_widths(
    harness: &FoundationHarness,
    session: &str,
) -> Result<Vec<PaneWidthSnapshot>, String> {
    let dump = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session}:0"),
        "-F",
        "#{pane_id}|#{pane_width}",
    ])?;

    let mut panes = Vec::new();
    for line in dump.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut parts = line.split('|');
        let pane_id = parts.next().unwrap_or_default().to_owned();
        let width = parts
            .next()
            .ok_or_else(|| format!("missing pane width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane width in `{line}`: {error}"))?;
        panes.push(PaneWidthSnapshot { pane_id, width });
    }

    Ok(panes)
}

pub(super) fn center_pane_id(panes: &[PaneWidthSnapshot]) -> Option<String> {
    panes
        .iter()
        .max_by_key(|pane| pane.width)
        .map(|pane| pane.pane_id.clone())
}

pub(super) fn parse_switch_table(binding: &str) -> Option<String> {
    let marker = "switch-client -T ";
    let start = binding.find(marker)? + marker.len();
    let tail = &binding[start..];
    let table = tail
        .split_whitespace()
        .next()
        .map(str::trim)
        .map(|value| value.trim_matches(';'))
        .map(|value| value.trim_matches('"'))
        .map(|value| value.trim_matches('\''))
        .unwrap_or_default();
    if table.is_empty() {
        None
    } else {
        Some(table.to_owned())
    }
}

pub(super) fn create_worktree_fixture(
    harness: &FoundationHarness,
) -> Result<WorktreeFixture, String> {
    let fixture_root = harness.work_dir().join("t11-red-worktree");
    let project_dir = fixture_root.join("project");
    let wt_1 = fixture_root.join("feature-1");
    let wt_2 = fixture_root.join("feature-2");

    if fixture_root.exists() {
        fs::remove_dir_all(&fixture_root).map_err(|error| {
            format!(
                "failed resetting fixture root {}: {error}",
                fixture_root.display()
            )
        })?;
    }
    fs::create_dir_all(&project_dir)
        .map_err(|error| format!("failed creating fixture project: {error}"))?;

    run_git(&project_dir, &["init"])?;
    run_git(
        &project_dir,
        &["config", "user.email", "e2e@example.invalid"],
    )?;
    run_git(&project_dir, &["config", "user.name", "E2E Harness"])?;
    fs::write(project_dir.join("README.md"), "# fixture\n")
        .map_err(|error| format!("failed writing fixture README: {error}"))?;
    run_git(&project_dir, &["add", "README.md"])?;
    run_git(&project_dir, &["commit", "-m", "fixture init"])?;

    let wt_1_arg = wt_1.display().to_string();
    let wt_2_arg = wt_2.display().to_string();
    run_git(
        &project_dir,
        &["worktree", "add", "--detach", wt_2_arg.as_str(), "HEAD"],
    )?;
    run_git(
        &project_dir,
        &["worktree", "add", "--detach", wt_1_arg.as_str(), "HEAD"],
    )?;

    Ok(WorktreeFixture {
        project_dir,
        expected_slot1_worktree: wt_1,
    })
}

pub(super) fn paths_equivalent(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }

    match (
        normalize_existing_path(Path::new(left)),
        normalize_existing_path(Path::new(right)),
    ) {
        (Some(left_canonical), Some(right_canonical)) => left_canonical == right_canonical,
        _ => false,
    }
}

pub(super) fn write_cluster_evidence(
    harness: &FoundationHarness,
    cluster: &str,
    evidence: &[String],
) -> Result<(), String> {
    let dir = harness.artifact_dir.join("triage-red");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed creating triage evidence directory: {error}"))?;
    fs::write(dir.join(format!("{cluster}.txt")), evidence.join("\n"))
        .map_err(|error| format!("failed writing triage evidence file: {error}"))
}

pub(super) fn pane_current_command(
    harness: &FoundationHarness,
    pane_id: &str,
) -> Result<String, String> {
    harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{pane_current_command}",
        ])
        .map(|value| value.trim().to_owned())
}

pub(super) fn install_failing_opencode_stub(harness: &FoundationHarness) -> Result<(), String> {
    let stub_path = harness.work_dir().join("bin").join("opencode");
    fs::write(
        &stub_path,
        "#!/usr/bin/env sh\nprintf 'red stub: opencode launch failed\\n' >&2\nexit 127\n",
    )
    .map_err(|error| format!("failed writing opencode RED stub: {error}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = fs::metadata(&stub_path)
            .map_err(|error| format!("failed reading opencode RED stub metadata: {error}"))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub_path, perms)
            .map_err(|error| format!("failed making opencode RED stub executable: {error}"))?;
    }

    Ok(())
}

fn run_git(repo_dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .map_err(|error| format!("failed running git {args:?}: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    Err(format!(
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn normalize_existing_path(path: &Path) -> Option<String> {
    path.canonicalize()
        .ok()
        .map(|path| path.display().to_string())
}
