use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::foundation_harness::FoundationHarness;

use super::slot_snapshots::SlotSnapshot;

pub(crate) struct WorktreeFixture {
    pub(crate) project_dir: PathBuf,
    pub(crate) canonical_project_dir: PathBuf,
    pub(crate) extra_worktrees: Vec<PathBuf>,
}

pub(crate) struct RemoteRemapFixture {
    pub(crate) project_dir: PathBuf,
    pub(crate) remote_prefix: PathBuf,
    pub(crate) expected_mapped_path: PathBuf,
}

pub(crate) fn create_worktree_fixture(
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

pub(crate) fn create_remote_remap_fixture(
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

pub(crate) fn expected_worktree_cycle(fixture: &WorktreeFixture) -> Vec<(u8, String)> {
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

pub(crate) fn slot_worktrees_exclude_utility_paths(slots: &[SlotSnapshot]) -> bool {
    slots.iter().all(|slot| {
        let name = Path::new(&slot.worktree)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default();
        !name.starts_with("beads") && !name.contains("beads-sync")
    })
}

pub(crate) fn slot_suffix_priority_holds(slots: &[SlotSnapshot]) -> bool {
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

pub(crate) fn normalize_existing_path(path: &Path) -> Option<String> {
    path.canonicalize()
        .ok()
        .map(|path| path.display().to_string())
}

pub(crate) fn paths_equivalent(left: &str, right: &str) -> bool {
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
