use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorktreeDiscovery {
    pub(super) worktrees: Vec<PathBuf>,
    pub(super) warning: Option<String>,
}

pub(super) fn discover_worktrees_for_slots(project_dir: &Path) -> WorktreeDiscovery {
    let mut discovered = Vec::new();
    let mut warning = None;

    match Command::new("git")
        .arg("-C")
        .arg(project_dir)
        .arg("worktree")
        .arg("list")
        .arg("--porcelain")
        .output()
    {
        Ok(result) if result.status.success() => {
            for line in String::from_utf8_lossy(&result.stdout).lines() {
                if let Some(path) = line.strip_prefix("worktree ") {
                    let path = path.trim();
                    if !path.is_empty() {
                        discovered.push(PathBuf::from(path));
                    }
                }
            }
        }
        Ok(result) => {
            warning = Some(format_worktree_diagnostic(
                "git worktree list returned non-zero status",
                result.status,
                &result.stdout,
                &result.stderr,
            ));
        }
        Err(error) => {
            warning = Some(format!(
                "git worktree list could not start: project_dir={}; error={error}",
                project_dir.display()
            ));
        }
    }

    discovered.push(project_dir.to_path_buf());
    WorktreeDiscovery {
        worktrees: normalize_worktrees(project_dir, discovered),
        warning,
    }
}

fn format_worktree_diagnostic(
    context: &str,
    status: std::process::ExitStatus,
    stdout: &[u8],
    stderr: &[u8],
) -> String {
    let status = status
        .code()
        .map_or_else(|| String::from("signal"), |code| code.to_string());
    let stdout = String::from_utf8_lossy(stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(stderr).trim().to_owned();

    format!("{context}; status={status}; stdout={stdout:?}; stderr={stderr:?}")
}

fn normalize_worktrees(project_dir: &Path, candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    use std::cmp::Ordering;
    use std::collections::BTreeSet;

    let canonical_project = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let mut unique = BTreeSet::new();

    for candidate in candidates {
        let normalized = candidate.canonicalize().unwrap_or(candidate);
        unique.insert(normalized);
    }

    unique.insert(canonical_project.clone());

    let mut ordered = unique
        .into_iter()
        .filter(|candidate| {
            if *candidate == canonical_project {
                return true;
            }

            !excluded_worktree_candidate(candidate) && slot_suffix_priority(candidate).is_some()
        })
        .collect::<Vec<_>>();

    ordered.sort_by(|left, right| {
        match (slot_suffix_priority(left), slot_suffix_priority(right)) {
            (Some(left_suffix), Some(right_suffix)) => {
                left_suffix.cmp(&right_suffix).then_with(|| left.cmp(right))
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => left.cmp(right),
        }
    });

    ordered
}

fn excluded_worktree_candidate(candidate: &Path) -> bool {
    let Some(name) = candidate.file_name().and_then(std::ffi::OsStr::to_str) else {
        return false;
    };

    name.starts_with("beads") || name.contains("beads-sync")
}

fn slot_suffix_priority(candidate: &Path) -> Option<u8> {
    let name = candidate.file_name().and_then(std::ffi::OsStr::to_str)?;
    (1_u8..=5).find(|slot| name.ends_with(&format!("-{slot}")))
}

#[cfg(test)]
mod tests {
    use super::normalize_worktrees;
    use super::slot_suffix_priority;

    #[test]
    fn normalize_worktrees_applies_exclusion_and_suffix_priority() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        let wt_2 = temp.path().join("feature-2");
        let wt_1 = temp.path().join("feature-1");
        let excluded_beads = temp.path().join("beads-main");
        let excluded_sync = temp.path().join("foo-beads-sync-copy");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        std::fs::create_dir_all(&wt_1).expect("wt-1 dir");
        std::fs::create_dir_all(&wt_2).expect("wt-2 dir");
        std::fs::create_dir_all(&excluded_beads).expect("excluded beads dir");
        std::fs::create_dir_all(&excluded_sync).expect("excluded sync dir");

        let ordered = normalize_worktrees(
            &project_dir,
            vec![
                excluded_beads.clone(),
                wt_2.clone(),
                project_dir.clone(),
                wt_1.clone(),
                excluded_sync.clone(),
                wt_2.clone(),
            ],
        );

        assert_eq!(ordered.len(), 3);
        assert_eq!(ordered[0], wt_1.canonicalize().expect("canonical wt-1"));
        assert_eq!(ordered[1], wt_2.canonicalize().expect("canonical wt-2"));
        assert_eq!(
            ordered[2],
            project_dir.canonicalize().expect("canonical project")
        );
    }

    #[test]
    fn normalize_worktrees_ignores_non_suffix_extra_worktrees() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        let generic = temp.path().join("release-preview");
        let suffixed = temp.path().join("feature-2");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        std::fs::create_dir_all(&generic).expect("generic dir");
        std::fs::create_dir_all(&suffixed).expect("suffixed dir");

        let ordered = normalize_worktrees(
            &project_dir,
            vec![generic.clone(), suffixed.clone(), project_dir.clone()],
        );

        assert_eq!(ordered.len(), 2);
        assert_eq!(
            ordered[0],
            suffixed.canonicalize().expect("canonical suffix")
        );
        assert_eq!(
            ordered[1],
            project_dir.canonicalize().expect("canonical project")
        );
    }

    #[test]
    fn slot_suffix_priority_is_constrained_to_one_through_five() {
        assert_eq!(
            slot_suffix_priority(std::path::Path::new("/tmp/worktree-1")),
            Some(1)
        );
        assert_eq!(
            slot_suffix_priority(std::path::Path::new("/tmp/worktree-5")),
            Some(5)
        );
        assert_eq!(
            slot_suffix_priority(std::path::Path::new("/tmp/worktree-9")),
            None
        );
        assert_eq!(
            slot_suffix_priority(std::path::Path::new("/tmp/worktree")),
            None
        );
    }
}
