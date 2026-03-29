use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ez_mux::session::THREE_PANE_CENTER_TARGET_PCT;
use ez_mux::session::THREE_PANE_SIDE_TARGET_PCT;
use ez_mux::session::THREE_PANE_TARGET_TOLERANCE_PCT;
use ez_mux::session::resolve_session_identity;
use serde::Serialize;

use crate::support::foundation_harness::{CmdOutput, FoundationHarness, TmuxSettleEvidence};

pub(super) const CORE_IDS: [&str; 15] = [
    "E2E-01", "E2E-02", "E2E-03", "E2E-04", "E2E-05", "E2E-06", "E2E-07", "E2E-08", "E2E-09",
    "E2E-10", "E2E-11", "E2E-12", "E2E-13", "E2E-16", "E2E-19",
];
const CENTER_WIDTH_TARGET_PCT: i32 = 40;
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
    pub(super) left_width: i32,
    pub(super) center_width: i32,
    pub(super) right_width: i32,
    pub(super) left_width_pct: i32,
    pub(super) center_width_pct: i32,
    pub(super) right_width_pct: i32,
    pub(super) left_width_target_pct: i32,
    pub(super) center_width_target_pct: i32,
    pub(super) right_width_target_pct: i32,
    pub(super) center_width_tolerance_pct: i32,
    pub(super) three_pane_within_tolerance: bool,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PaneGraphEntry {
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}

pub(super) struct RemoteRemapFixture {
    pub(super) project_dir: PathBuf,
    pub(super) remote_prefix: PathBuf,
    pub(super) expected_mapped_path: PathBuf,
}

#[derive(Serialize)]
pub(super) struct RemotePathEvidence {
    pub(super) local_project_dir: String,
    pub(super) remote_path: String,
    pub(super) remote_path_source: String,
    pub(super) expected_mapped_path: String,
    pub(super) effective_mapped_path: String,
    pub(super) remap_applied: bool,
    pub(super) opencode_attach_url: String,
    pub(super) opencode_server_url_source: String,
    pub(super) opencode_server_password_set: bool,
    pub(super) opencode_server_password_source: String,
}

#[derive(Serialize)]
pub(super) struct HelperStateSnapshot {
    pub(super) helper_sessions: Vec<String>,
    pub(super) helper_pane_pids: Vec<u32>,
}

#[derive(Serialize)]
pub(super) struct HelperLifecycleEvidence {
    pub(super) before: HelperStateSnapshot,
    pub(super) after: HelperStateSnapshot,
    pub(super) pre_helper_pids_alive_after_teardown: Vec<u32>,
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
    pub(super) remote_path: Option<RemotePathEvidence>,
    pub(super) helper_state: Option<HelperLifecycleEvidence>,
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

pub(super) fn send_prefix_keybind(
    harness: &FoundationHarness,
    session_name: &str,
    key: &str,
) -> Result<(), String> {
    let target = format!("{session_name}:0");

    if harness
        .tmux_capture(&["send-keys", "-K", "-t", &target, "C-b", key])
        .is_ok()
    {
        return Ok(());
    }

    harness
        .tmux_capture(&["send-keys", "-t", &target, "C-b", key])
        .map(|_| ())
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

    let ordered_columns = columns.values().collect::<Vec<_>>();
    let left_column_panes = ordered_columns.first().map_or(0, |column| column.len());
    let right_column_panes = ordered_columns.last().map_or(0, |column| column.len());
    let left_width = ordered_columns
        .first()
        .and_then(|column| column.first())
        .map_or(0, |pane| pane.1);
    let right_width = ordered_columns
        .last()
        .and_then(|column| column.first())
        .map_or(0, |pane| pane.1);

    let mut center_width = 0;
    let mut center_column_panes = 0;
    if ordered_columns.len() >= 3 {
        let center_column = ordered_columns[ordered_columns.len() / 2];
        center_column_panes = center_column.len();
        center_width = center_column.first().map_or(0, |pane| pane.1);
        if center_column.len() != 1 || center_column[0].2 < max_height {
            center_column_panes = 0;
        }
    } else {
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
    }

    let left_width_pct = if window_width > 0 {
        (left_width * 100) / window_width
    } else {
        0
    };
    let center_width_pct = if window_width > 0 {
        (center_width * 100) / window_width
    } else {
        0
    };
    let right_width_pct = if window_width > 0 {
        (right_width * 100) / window_width
    } else {
        0
    };
    let three_pane_target_tolerance_pct = i32::from(THREE_PANE_TARGET_TOLERANCE_PCT);
    let left_three_pane_delta = (left_width_pct - i32::from(THREE_PANE_SIDE_TARGET_PCT)).abs();
    let center_three_pane_delta =
        (center_width_pct - i32::from(THREE_PANE_CENTER_TARGET_PCT)).abs();
    let right_three_pane_delta = (right_width_pct - i32::from(THREE_PANE_SIDE_TARGET_PCT)).abs();
    let three_pane_within_tolerance = left_three_pane_delta <= three_pane_target_tolerance_pct
        && center_three_pane_delta <= three_pane_target_tolerance_pct
        && right_three_pane_delta <= three_pane_target_tolerance_pct;
    let delta = (center_width_pct - CENTER_WIDTH_TARGET_PCT).abs();
    let center_within_tolerance = delta <= CENTER_WIDTH_TOLERANCE_PCT;

    let assertions = vec![
        format!("pane count = {}", panes.len()),
        format!("window width = {window_width}"),
        format!("left width = {left_width}"),
        format!("center width = {center_width}"),
        format!("right width = {right_width}"),
        format!(
            "left width pct = {left_width_pct} (target={} +/- {})",
            THREE_PANE_SIDE_TARGET_PCT, THREE_PANE_TARGET_TOLERANCE_PCT
        ),
        format!(
            "center width pct = {center_width_pct} (target={} +/- {})",
            CENTER_WIDTH_TARGET_PCT, CENTER_WIDTH_TOLERANCE_PCT
        ),
        format!(
            "right width pct = {right_width_pct} (target={} +/- {})",
            THREE_PANE_SIDE_TARGET_PCT, THREE_PANE_TARGET_TOLERANCE_PCT
        ),
        format!(
            "left/center/right panes = {left_column_panes}/{center_column_panes}/{right_column_panes}"
        ),
        format!("three-pane width tolerance satisfied = {three_pane_within_tolerance}"),
        format!("center width within tolerance = {center_within_tolerance}"),
    ];

    Ok((
        LayoutSnapshot {
            pane_count: panes.len(),
            window_width,
            left_width,
            center_width,
            right_width,
            left_width_pct,
            center_width_pct,
            right_width_pct,
            left_width_target_pct: i32::from(THREE_PANE_SIDE_TARGET_PCT),
            center_width_target_pct: CENTER_WIDTH_TARGET_PCT,
            right_width_target_pct: i32::from(THREE_PANE_SIDE_TARGET_PCT),
            center_width_tolerance_pct: CENTER_WIDTH_TOLERANCE_PCT,
            three_pane_within_tolerance,
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

pub(super) fn slot_worktree_mapping_stable(left: &[SlotSnapshot], right: &[SlotSnapshot]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter().zip(right.iter()).all(|(lhs, rhs)| {
        lhs.slot_id == rhs.slot_id && paths_equivalent(&lhs.worktree, &rhs.worktree)
    })
}

pub(super) fn read_pane_graph(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<Vec<PaneGraphEntry>, String> {
    let raw = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session_name}:0"),
        "-F",
        "#{pane_left}|#{pane_top}|#{pane_width}|#{pane_height}",
    ])?;

    let mut graph = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut parts = line.split('|');
        let left = parts
            .next()
            .ok_or_else(|| format!("missing pane_left in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_left in `{line}`: {error}"))?;
        let top = parts
            .next()
            .ok_or_else(|| format!("missing pane_top in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_top in `{line}`: {error}"))?;
        let width = parts
            .next()
            .ok_or_else(|| format!("missing pane_width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_width in `{line}`: {error}"))?;
        let height = parts
            .next()
            .ok_or_else(|| format!("missing pane_height in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_height in `{line}`: {error}"))?;

        graph.push(PaneGraphEntry {
            left,
            top,
            width,
            height,
        });
    }

    graph.sort_by_key(|entry| (entry.left, entry.top, entry.width, entry.height));
    Ok(graph)
}

pub(super) fn pane_graph_stable(left: &[PaneGraphEntry], right: &[PaneGraphEntry]) -> bool {
    left == right
}

pub(super) fn create_worktree_fixture(
    harness: &FoundationHarness,
) -> Result<WorktreeFixture, String> {
    let fixture_root = harness.work_dir().join("e2e03-worktree-fixture");
    let project_dir = fixture_root.join("project");
    let wt_1 = fixture_root.join("feature-1");
    let wt_2 = fixture_root.join("feature-2");
    let excluded_beads = fixture_root.join("beads-main");
    let excluded_sync = fixture_root.join("alpha-beads-sync-copy");

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

    let wt_1_arg = wt_1.to_string_lossy().into_owned();
    let wt_2_arg = wt_2.to_string_lossy().into_owned();
    let excluded_beads_arg = excluded_beads.to_string_lossy().into_owned();
    let excluded_sync_arg = excluded_sync.to_string_lossy().into_owned();
    run_git(
        &project_dir,
        &["worktree", "add", "--detach", &wt_2_arg, "HEAD"],
    )?;
    run_git(
        &project_dir,
        &["worktree", "add", "--detach", &wt_1_arg, "HEAD"],
    )?;
    run_git(
        &project_dir,
        &["worktree", "add", "--detach", &excluded_beads_arg, "HEAD"],
    )?;
    run_git(
        &project_dir,
        &["worktree", "add", "--detach", &excluded_sync_arg, "HEAD"],
    )?;

    Ok(WorktreeFixture {
        project_dir: project_dir.clone(),
        canonical_project_dir: project_dir
            .canonicalize()
            .map_err(|error| format!("failed canonicalizing fixture project: {error}"))?,
        extra_worktrees: vec![
            wt_1.canonicalize()
                .map_err(|error| format!("failed canonicalizing fixture wt-1: {error}"))?,
            wt_2.canonicalize()
                .map_err(|error| format!("failed canonicalizing fixture wt-2: {error}"))?,
        ],
    })
}

pub(super) fn create_remote_remap_fixture(
    harness: &FoundationHarness,
) -> Result<RemoteRemapFixture, String> {
    let fixture_root = harness.work_dir().join("e2e09-remote-remap-fixture");
    let repo_root = fixture_root.join("alpha");
    let project_dir = repo_root.join("worktrees").join("feature-x");
    let remote_prefix =
        std::env::temp_dir().join(format!("ezm-e2e-remote-remap-{}", harness.run_id));
    let expected_mapped_path = remote_prefix
        .join("alpha")
        .join("worktrees")
        .join("feature-x");

    if fixture_root.exists() {
        fs::remove_dir_all(&fixture_root).map_err(|error| {
            format!(
                "failed resetting remote remap fixture root {}: {error}",
                fixture_root.display()
            )
        })?;
    }
    if remote_prefix.exists() {
        fs::remove_dir_all(&remote_prefix).map_err(|error| {
            format!(
                "failed resetting remote remap prefix {}: {error}",
                remote_prefix.display()
            )
        })?;
    }

    fs::create_dir_all(repo_root.join(".git")).map_err(|error| {
        format!(
            "failed creating fixture git root {}: {error}",
            repo_root.display()
        )
    })?;
    fs::create_dir_all(&project_dir).map_err(|error| {
        format!(
            "failed creating fixture project dir {}: {error}",
            project_dir.display()
        )
    })?;
    fs::create_dir_all(&expected_mapped_path).map_err(|error| {
        format!(
            "failed creating expected mapped path {}: {error}",
            expected_mapped_path.display()
        )
    })?;

    Ok(RemoteRemapFixture {
        project_dir: project_dir
            .canonicalize()
            .map_err(|error| format!("failed canonicalizing remote fixture project: {error}"))?,
        remote_prefix: remote_prefix
            .canonicalize()
            .map_err(|error| format!("failed canonicalizing remote fixture prefix: {error}"))?,
        expected_mapped_path: expected_mapped_path.canonicalize().map_err(|error| {
            format!("failed canonicalizing expected mapped path fixture: {error}")
        })?,
    })
}

pub(super) fn expected_worktree_cycle(fixture: &WorktreeFixture) -> Vec<(u8, String)> {
    let mut ordered = fixture.extra_worktrees.clone();
    ordered.sort();
    ordered.push(fixture.canonical_project_dir.clone());
    let fallback = fixture.canonical_project_dir.display().to_string();

    (1_u8..=5)
        .enumerate()
        .map(|(index, slot_id)| {
            let selected = ordered
                .get(index)
                .map_or_else(|| fallback.clone(), |path| path.display().to_string());
            (slot_id, selected)
        })
        .collect()
}

pub(super) fn slot_worktrees_exclude_utility_paths(slots: &[SlotSnapshot]) -> bool {
    slots.iter().all(|slot| {
        let name = Path::new(&slot.worktree)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default();
        !name.starts_with("beads") && !name.contains("beads-sync")
    })
}

pub(super) fn slot_suffix_priority_holds(slots: &[SlotSnapshot]) -> bool {
    let first_non_suffix_slot = slots
        .iter()
        .find(|slot| {
            let name = Path::new(&slot.worktree)
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or_default();
            !(name.ends_with("-1")
                || name.ends_with("-2")
                || name.ends_with("-3")
                || name.ends_with("-4")
                || name.ends_with("-5"))
        })
        .map_or(u8::MAX, |slot| slot.slot_id);

    slots.iter().all(|slot| {
        let name = Path::new(&slot.worktree)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default();
        let suffix = name.ends_with("-1")
            || name.ends_with("-2")
            || name.ends_with("-3")
            || name.ends_with("-4")
            || name.ends_with("-5");
        !suffix || slot.slot_id <= first_non_suffix_slot
    })
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

pub(super) fn popup_helper_session_name(session_name: &str, slot_id: u8) -> String {
    format!("{session_name}__popup_slot_{slot_id}")
}

pub(super) fn read_helper_state_snapshot(
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

pub(super) fn helper_pids_alive(pids: &[u32]) -> Vec<u32> {
    pids.iter()
        .copied()
        .filter(|pid| {
            Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
        })
        .collect()
}

pub(super) fn wait_for_helper_pids_to_exit(
    pids: &[u32],
    timeout: Duration,
    poll_interval: Duration,
) -> Result<Vec<u32>, String> {
    let _all_gone = poll_until(timeout, poll_interval, || {
        Ok(helper_pids_alive(pids).is_empty())
    })?;
    Ok(helper_pids_alive(pids))
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
        .map_err(|error| format!("failed serializing json for {}: {error}", path.display()))?;
    fs::write(path, json)
        .map_err(|error| format!("failed writing json {}: {error}", path.display()))
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

#[cfg(test)]
mod tests {
    use super::{PaneGraphEntry, SlotSnapshot, pane_graph_stable, slot_worktree_mapping_stable};

    #[test]
    fn pane_graph_stability_ignores_runtime_pane_ids() {
        let before = vec![
            PaneGraphEntry {
                left: 0,
                top: 0,
                width: 30,
                height: 20,
            },
            PaneGraphEntry {
                left: 31,
                top: 0,
                width: 40,
                height: 40,
            },
            PaneGraphEntry {
                left: 72,
                top: 0,
                width: 30,
                height: 20,
            },
            PaneGraphEntry {
                left: 0,
                top: 21,
                width: 30,
                height: 19,
            },
            PaneGraphEntry {
                left: 72,
                top: 21,
                width: 30,
                height: 19,
            },
        ];
        let after = before.clone();

        assert!(pane_graph_stable(&before, &after));
    }

    #[test]
    fn slot_worktree_mapping_stability_allows_pane_id_churn() {
        let before = vec![
            SlotSnapshot {
                slot_id: 1,
                pane_id: String::from("%1"),
                worktree: String::from("/tmp/wt-1"),
            },
            SlotSnapshot {
                slot_id: 2,
                pane_id: String::from("%2"),
                worktree: String::from("/tmp/wt-2"),
            },
        ];
        let after = vec![
            SlotSnapshot {
                slot_id: 1,
                pane_id: String::from("%9"),
                worktree: String::from("/tmp/wt-1"),
            },
            SlotSnapshot {
                slot_id: 2,
                pane_id: String::from("%10"),
                worktree: String::from("/tmp/wt-2"),
            },
        ];

        assert!(slot_worktree_mapping_stable(&before, &after));
    }
}
