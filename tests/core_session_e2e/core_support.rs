use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use ez_mux::session::resolve_session_identity;
use serde::Serialize;

use crate::support::foundation_harness::{CmdOutput, FoundationHarness, TmuxSettleEvidence};

pub(super) const CORE_IDS: [&str; 8] = [
    "E2E-01", "E2E-02", "E2E-03", "E2E-04", "E2E-05", "E2E-06", "E2E-07", "E2E-08",
];
const CENTER_WIDTH_TARGET_PCT: i32 = 38;
const CENTER_WIDTH_TOLERANCE_PCT: i32 = 3;
pub(super) const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(50);
pub(super) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize)]
pub(super) struct RunMetadata {
    pub(super) run_id: String,
    pub(super) commit_sha: String,
    pub(super) os: String,
    pub(super) shell: String,
    pub(super) tmux_version: String,
    pub(super) artifact_dir: String,
    pub(super) test_ids: Vec<String>,
    pub(super) pass_total: usize,
    pub(super) fail_total: usize,
}

#[derive(Serialize)]
pub(super) struct CommandSample {
    pub(super) args: Vec<String>,
    pub(super) exit_code: i32,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

#[derive(Serialize)]
pub(super) struct SettleEvidence {
    pub(super) attempts: u32,
    pub(super) poll_interval_ms: u64,
    pub(super) timeout_ms: u64,
    pub(super) stable: bool,
    pub(super) sessions: String,
    pub(super) windows: String,
    pub(super) panes: String,
}

#[derive(Serialize)]
pub(super) struct SessionSnapshot {
    pub(super) name: String,
    pub(super) exists: bool,
    pub(super) count: usize,
}

#[derive(Serialize)]
pub(super) struct LayoutSnapshot {
    pub(super) pane_count: usize,
    pub(super) window_width: i32,
    pub(super) center_width: i32,
    pub(super) center_width_pct: i32,
    pub(super) center_width_target_pct: i32,
    pub(super) center_width_tolerance_pct: i32,
    pub(super) center_within_tolerance: bool,
    pub(super) left_column_panes: usize,
    pub(super) center_column_panes: usize,
    pub(super) right_column_panes: usize,
}

#[derive(Serialize)]
pub(super) struct SlotSnapshot {
    pub(super) slot_id: u8,
    pub(super) pane_id: String,
    pub(super) worktree: String,
}

#[derive(Clone)]
pub(super) struct PaneGeometry {
    pub(super) id: String,
    pub(super) left: i32,
    pub(super) width: i32,
}

pub(super) struct WorktreeFixture {
    pub(super) project_dir: PathBuf,
    pub(super) canonical_project_dir: PathBuf,
    pub(super) extra_worktrees: Vec<PathBuf>,
}

#[derive(Serialize)]
pub(super) struct CaseEvidence {
    pub(super) id: String,
    pub(super) pass: bool,
    pub(super) assertions: Vec<String>,
    pub(super) samples: Vec<CommandSample>,
    pub(super) settle: SettleEvidence,
    pub(super) snapshot: SessionSnapshot,
    pub(super) layout: Option<LayoutSnapshot>,
    pub(super) slots: Option<Vec<SlotSnapshot>>,
}

#[derive(Serialize)]
pub(super) struct SuiteEvidence {
    pub(super) metadata: RunMetadata,
    pub(super) cases: Vec<CaseEvidence>,
}

pub(super) fn sample(args: &[&str], output: &CmdOutput) -> CommandSample {
    CommandSample {
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        exit_code: output.exit_code,
        stdout: output.stdout.clone(),
        stderr: output.stderr.clone(),
    }
}

pub(super) fn settle_snapshot(harness: &FoundationHarness, test_id: &str) -> TmuxSettleEvidence {
    harness
        .settle_tmux_snapshot(DEFAULT_POLL_INTERVAL, DEFAULT_TIMEOUT)
        .unwrap_or_else(|error| panic!("{test_id} settle evidence failed: {error}"))
}

pub(super) fn map_settle(settle: TmuxSettleEvidence) -> SettleEvidence {
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

pub(super) fn extract_stdout_field(stdout: &str, key: &str) -> Option<String> {
    let marker = format!("{key}=");
    let start = stdout.find(&marker)? + marker.len();
    let tail = &stdout[start..];
    let end = tail.find(';').unwrap_or(tail.len());
    Some(tail[..end].trim().trim_end_matches('.').to_owned())
}

#[allow(clippy::too_many_lines)]
pub(super) fn inspect_layout(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<(LayoutSnapshot, Vec<String>), String> {
    let window_width_raw = harness.tmux_capture(&[
        "display-message",
        "-p",
        "-t",
        &format!("{session_name}:0"),
        "#{window_width}",
    ])?;
    let window_width = window_width_raw
        .trim()
        .parse::<i32>()
        .map_err(|error| format!("invalid window width `{window_width_raw}`: {error}"))?;

    let pane_dump = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session_name}:0"),
        "-F",
        "#{pane_id}|#{pane_width}|#{pane_height}|#{pane_left}",
    ])?;

    let mut panes = Vec::new();
    for line in pane_dump
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split('|');
        let pane_id = parts.next().unwrap_or_default().to_owned();
        let pane_width = parts
            .next()
            .ok_or_else(|| format!("missing pane width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane width in `{line}`: {error}"))?;
        let pane_height = parts
            .next()
            .ok_or_else(|| format!("missing pane height in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane height in `{line}`: {error}"))?;
        let pane_left = parts
            .next()
            .ok_or_else(|| format!("missing pane left in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane left in `{line}`: {error}"))?;
        panes.push((pane_id, pane_width, pane_height, pane_left));
    }

    let max_height = panes
        .iter()
        .map(|(_, _, height, _)| *height)
        .max()
        .unwrap_or(0);

    let mut columns = std::collections::BTreeMap::<i32, Vec<(String, i32, i32)>>::new();
    for (pane_id, pane_width, pane_height, pane_left) in &panes {
        columns
            .entry(*pane_left)
            .or_default()
            .push((pane_id.clone(), *pane_width, *pane_height));
    }

    let left_column_panes = columns.values().next().map_or(0, std::vec::Vec::len);
    let right_column_panes = columns.values().last().map_or(0, std::vec::Vec::len);

    let mut center_width = 0;
    let mut center_column_panes = 0;
    for panes_in_column in columns.values() {
        if panes_in_column.len() == 1 {
            center_column_panes = 1;
            center_width = panes_in_column[0].1;
            if panes_in_column[0].2 < max_height {
                center_column_panes = 0;
            }
            break;
        }
    }

    let center_width_pct = if window_width > 0 {
        (center_width * 100) / window_width
    } else {
        0
    };
    let delta = (center_width_pct - CENTER_WIDTH_TARGET_PCT).abs();
    let center_within_tolerance = delta <= CENTER_WIDTH_TOLERANCE_PCT;

    let assertions = vec![
        format!("pane count = {}", panes.len()),
        format!("window width = {window_width}"),
        format!("center width = {center_width}"),
        format!(
            "center width pct = {center_width_pct} (target={} +/- {})",
            CENTER_WIDTH_TARGET_PCT, CENTER_WIDTH_TOLERANCE_PCT
        ),
        format!(
            "left/center/right panes = {left_column_panes}/{center_column_panes}/{right_column_panes}"
        ),
        format!("center width within tolerance = {center_within_tolerance}"),
    ];

    Ok((
        LayoutSnapshot {
            pane_count: panes.len(),
            window_width,
            center_width,
            center_width_pct,
            center_width_target_pct: CENTER_WIDTH_TARGET_PCT,
            center_width_tolerance_pct: CENTER_WIDTH_TOLERANCE_PCT,
            center_within_tolerance,
            left_column_panes,
            center_column_panes,
            right_column_panes,
        },
        assertions,
    ))
}

pub(super) fn read_slot_snapshot(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<Vec<SlotSnapshot>, String> {
    let mut slots = Vec::new();
    for slot_id in 1_u8..=5 {
        let pane_key = format!("@ezm_slot_{slot_id}_pane");
        let worktree_key = format!("@ezm_slot_{slot_id}_worktree");

        let pane_id = harness
            .tmux_capture(&["show-options", "-v", "-t", session_name, &pane_key])?
            .trim()
            .to_owned();
        let worktree = harness
            .tmux_capture(&["show-options", "-v", "-t", session_name, &worktree_key])?
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

pub(super) fn slot_snapshots_match(left: &[SlotSnapshot], right: &[SlotSnapshot]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter().zip(right.iter()).all(|(lhs, rhs)| {
        lhs.slot_id == rhs.slot_id && lhs.pane_id == rhs.pane_id && lhs.worktree == rhs.worktree
    })
}

pub(super) fn create_worktree_fixture(
    harness: &FoundationHarness,
) -> Result<WorktreeFixture, String> {
    let fixture_root = harness.work_dir().join("e2e03-worktree-fixture");
    let project_dir = fixture_root.join("project");
    let wt_a = fixture_root.join("wt-a");
    let wt_b = fixture_root.join("wt-b");

    if fixture_root.exists() {
        fs::remove_dir_all(&fixture_root).map_err(|error| {
            format!(
                "failed resetting fixture root {}: {error}",
                fixture_root.display()
            )
        })?;
    }
    fs::create_dir_all(&project_dir).map_err(|error| {
        format!(
            "failed creating fixture project {}: {error}",
            project_dir.display()
        )
    })?;

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

    let primary_worktree_arg = wt_a.to_string_lossy().into_owned();
    let secondary_checkout_path = wt_b.to_string_lossy().into_owned();
    run_git(
        &project_dir,
        &["worktree", "add", "--detach", &primary_worktree_arg, "HEAD"],
    )?;
    run_git(
        &project_dir,
        &[
            "worktree",
            "add",
            "--detach",
            &secondary_checkout_path,
            "HEAD",
        ],
    )?;

    Ok(WorktreeFixture {
        project_dir: project_dir.clone(),
        canonical_project_dir: project_dir
            .canonicalize()
            .map_err(|error| format!("failed canonicalizing fixture project: {error}"))?,
        extra_worktrees: vec![
            wt_a.canonicalize()
                .map_err(|error| format!("failed canonicalizing fixture wt-a: {error}"))?,
            wt_b.canonicalize()
                .map_err(|error| format!("failed canonicalizing fixture wt-b: {error}"))?,
        ],
    })
}

pub(super) fn expected_worktree_cycle(fixture: &WorktreeFixture) -> Vec<(u8, String)> {
    let mut ordered = vec![fixture.canonical_project_dir.clone()];
    let mut extras = fixture.extra_worktrees.clone();
    extras.sort();
    ordered.extend(extras);

    (1_u8..=5)
        .enumerate()
        .map(|(index, slot_id)| {
            (
                slot_id,
                ordered[index % ordered.len()].display().to_string(),
            )
        })
        .collect()
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

pub(super) fn read_pane_geometry(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<Vec<PaneGeometry>, String> {
    let raw = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session_name}:0"),
        "-F",
        "#{pane_id}|#{pane_left}|#{pane_width}",
    ])?;

    let mut panes = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut parts = line.split('|');
        let pane_id = parts.next().unwrap_or_default().to_owned();
        let pane_left = parts
            .next()
            .ok_or_else(|| format!("missing pane_left in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_left in `{line}`: {error}"))?;
        let pane_width = parts
            .next()
            .ok_or_else(|| format!("missing pane_width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_width in `{line}`: {error}"))?;

        panes.push(PaneGeometry {
            id: pane_id,
            left: pane_left,
            width: pane_width,
        });
    }

    Ok(panes)
}

pub(super) fn center_pane_from_geometry(geometry: &[PaneGeometry]) -> String {
    geometry
        .iter()
        .max_by_key(|pane| (pane.width, -pane.left))
        .map(|pane| pane.id.clone())
        .unwrap_or_default()
}

pub(super) fn pane_geometry_by_id<'a>(
    geometry: &'a [PaneGeometry],
    pane_id: &str,
) -> Option<&'a PaneGeometry> {
    geometry.iter().find(|pane| pane.id == pane_id)
}

pub(super) fn normalize_existing_path(path: &Path) -> Option<String> {
    path.canonicalize()
        .ok()
        .map(|path| path.display().to_string())
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

pub(super) fn prepare_fresh_create_path(
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

pub(super) fn read_commit_sha(project_root: &Path) -> String {
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

pub(super) fn write_case_artifacts(dir: &Path, cases: &[CaseEvidence]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| format!("failed creating case directory: {error}"))?;
    for case in cases {
        let path = dir.join(format!("{}.json", case.id));
        write_json(&path, case)?;
    }
    Ok(())
}

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed serializing json for {path:?}: {error}"))?;
    fs::write(path, json).map_err(|error| format!("failed writing json {path:?}: {error}"))
}

pub(super) fn poll_until(
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
