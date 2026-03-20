use std::path::{Path, PathBuf};

use super::SessionError;

const MAX_SLUG_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionIdentity {
    pub project_dir: PathBuf,
    pub project_key: String,
    pub session_name: String,
}

/// Resolves deterministic identity values for a project path.
///
/// # Errors
/// Returns an error when the provided project path cannot be canonicalized.
pub fn resolve_session_identity(project_dir: &Path) -> Result<SessionIdentity, SessionError> {
    let canonical_dir =
        project_dir
            .canonicalize()
            .map_err(|source| SessionError::CanonicalizeProjectPath {
                path: project_dir.to_path_buf(),
                source,
            })?;

    let slug = project_slug(&canonical_dir);
    let project_key = stable_project_key(&canonical_dir);
    let short_key = &project_key[..12];
    let session_name = format!("ezm-{slug}-{short_key}");

    Ok(SessionIdentity {
        project_dir: canonical_dir,
        project_key,
        session_name,
    })
}

fn project_slug(project_dir: &Path) -> String {
    let name = project_dir
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("project");

    let mut slug = String::new();
    let mut previous_dash = false;

    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        return String::from("project");
    }

    if slug.len() > MAX_SLUG_LEN {
        slug.truncate(MAX_SLUG_LEN);
        while slug.ends_with('-') {
            slug.pop();
        }
    }

    if slug.is_empty() {
        String::from("project")
    } else {
        slug
    }
}

fn stable_project_key(project_dir: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    hash_path_bytes(&mut hash, project_dir);

    format!("{hash:016x}")
}

#[cfg(unix)]
fn hash_path_bytes(hash: &mut u64, project_dir: &Path) {
    use std::os::unix::ffi::OsStrExt;

    for byte in project_dir.as_os_str().as_bytes() {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

#[cfg(windows)]
fn hash_path_bytes(hash: &mut u64, project_dir: &Path) {
    use std::os::windows::ffi::OsStrExt;

    for word in project_dir.as_os_str().encode_wide() {
        for byte in word.to_le_bytes() {
            *hash ^= u64::from(byte);
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn hash_path_bytes(hash: &mut u64, project_dir: &Path) {
    for byte in project_dir.to_string_lossy().as_bytes() {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

#[cfg(test)]
mod tests {
    use super::stable_project_key;
    use std::path::Path;

    #[test]
    fn stable_project_key_is_deterministic() {
        let key_a = stable_project_key(Path::new("/tmp/project"));
        let key_b = stable_project_key(Path::new("/tmp/project"));
        assert_eq!(key_a, key_b);
    }

    #[cfg(unix)]
    #[test]
    fn stable_project_key_uses_raw_unix_path_bytes() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let raw_bytes = vec![0x2f, 0x74, 0x6d, 0x70, 0x2f, 0x66, 0x6f, 0x80, 0x6f];
        let os = OsString::from_vec(raw_bytes);
        let path = std::path::PathBuf::from(os);

        let key = stable_project_key(&path);
        assert_eq!(key.len(), 16);
    }
}
