use std::path::{Path, PathBuf};

use super::SessionError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePathResolution {
    pub effective_path: PathBuf,
    pub remapped: bool,
}

/// Resolves an effective path for remote launches.
///
/// # Errors
/// Returns an error when `remote_path` is provided but is not an absolute
/// path.
pub fn resolve_remote_path(
    local_path: &Path,
    remote_path: Option<&str>,
) -> Result<RemotePathResolution, SessionError> {
    let Some(prefix) = remote_path.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(RemotePathResolution {
            effective_path: local_path.to_path_buf(),
            remapped: false,
        });
    };

    let normalized_prefix = Path::new(prefix);
    if !normalized_prefix.is_absolute() {
        return Err(SessionError::InvalidRemotePathMappingPrefix {
            prefix: prefix.to_owned(),
        });
    }

    let top = discover_git_top(local_path);
    let effective_path = if let Some(repo_top) = top {
        let repo_base = repo_top
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("project");
        let mut path = normalized_prefix.join(repo_base);
        if let Ok(relative) = local_path.strip_prefix(&repo_top) {
            if !relative.as_os_str().is_empty() {
                path = path.join(relative);
            }
        }
        path
    } else {
        let fallback_base = local_path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("project");
        normalized_prefix.join(fallback_base)
    };

    Ok(RemotePathResolution {
        effective_path,
        remapped: true,
    })
}

fn discover_git_top(local_path: &Path) -> Option<PathBuf> {
    let mut cursor = Some(local_path);
    while let Some(current) = cursor {
        if current.join(".git").exists() {
            return Some(current.to_path_buf());
        }
        cursor = current.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{RemotePathResolution, resolve_remote_path};

    #[test]
    fn canonical_mapping_uses_repo_root_basename_plus_relative_suffix() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("alpha");
        let nested = repo_root.join("worktrees").join("feature-x");
        std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
        std::fs::create_dir_all(&nested).expect("create nested");

        let resolved = resolve_remote_path(&nested, Some("/srv/remotes/"))
            .expect("canonical mapping should resolve");

        assert_eq!(
            resolved,
            RemotePathResolution {
                effective_path: Path::new("/srv/remotes/alpha/worktrees/feature-x").to_path_buf(),
                remapped: true,
            }
        );
    }

    #[test]
    fn missing_mapping_returns_local_path_without_remap() {
        let local = Path::new("/tmp/local-only");

        let resolved = resolve_remote_path(local, None).expect("missing mapping should be allowed");

        assert_eq!(
            resolved,
            RemotePathResolution {
                effective_path: local.to_path_buf(),
                remapped: false,
            }
        );
    }

    #[test]
    fn invalid_mapping_prefix_fails_fast() {
        let local = Path::new("/tmp/local-only");

        let error =
            resolve_remote_path(local, Some("relative/prefix")).expect_err("invalid should fail");

        let rendered = error.to_string();
        assert!(rendered.contains("invalid remote path"));
    }
}
