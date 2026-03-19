use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn discover_worktrees_for_slots(project_dir: &Path) -> Vec<PathBuf> {
    let mut discovered = Vec::new();

    let output = Command::new("git")
        .arg("-C")
        .arg(project_dir)
        .arg("worktree")
        .arg("list")
        .arg("--porcelain")
        .output();

    if let Ok(result) = output {
        if result.status.success() {
            for line in String::from_utf8_lossy(&result.stdout).lines() {
                if let Some(path) = line.strip_prefix("worktree ") {
                    let path = path.trim();
                    if !path.is_empty() {
                        discovered.push(PathBuf::from(path));
                    }
                }
            }
        }
    }

    discovered.push(project_dir.to_path_buf());
    normalize_worktrees(project_dir, discovered)
}

fn normalize_worktrees(project_dir: &Path, candidates: Vec<PathBuf>) -> Vec<PathBuf> {
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

    let mut ordered = unique.into_iter().collect::<Vec<_>>();
    if let Some(index) = ordered.iter().position(|path| *path == canonical_project) {
        let primary = ordered.remove(index);
        ordered.insert(0, primary);
    }

    ordered
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::normalize_worktrees;

    #[test]
    fn normalize_worktrees_is_deterministic_and_project_first() {
        let temp = tempfile::tempdir().expect("tempdir");
        let project_dir = temp.path().join("project");
        let wt_b = temp.path().join("wt-b");
        let wt_a = temp.path().join("wt-a");
        std::fs::create_dir_all(&project_dir).expect("project dir");
        std::fs::create_dir_all(&wt_a).expect("wt-a dir");
        std::fs::create_dir_all(&wt_b).expect("wt-b dir");

        let ordered = normalize_worktrees(
            &project_dir,
            vec![
                wt_b.clone(),
                project_dir.clone(),
                wt_a.clone(),
                wt_a.clone(),
            ],
        );

        assert_eq!(ordered.len(), 3);
        assert_eq!(
            ordered[0],
            project_dir.canonicalize().expect("canonical project")
        );
        assert_eq!(ordered[1], wt_a.canonicalize().expect("canonical wt-a"));
        assert_eq!(ordered[2], wt_b.canonicalize().expect("canonical wt-b"));
        assert!(ordered[0].starts_with(Path::new(temp.path())));
    }
}
