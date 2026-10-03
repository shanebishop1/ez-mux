//! Import trusted local remote-agents profiles without rewriting them.
use super::{
    config::{self, Auxiliary, Group, Project, Registry, Slot},
    runtime,
};
use crate::{
    app::AppError,
    opencode::{atomic_write, error, locked_file, private_directory},
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const KEYS: &[&str] = &[
    "REMOTE_ROOT",
    "REMOTE_SERVICE",
    "REMOTE_OC_BIN",
    "REMOTE_SESSION_PREFIX",
    "EZM_CONFIG",
    "REMOTE_PERLES_BIN",
    "REMOTE_BD_BIN",
    "OPENCODE_SERVER_URL",
    "REMOTE_CODEX_WORKTREE",
    "REMOTE_CODEX_BIN",
    "REMOTE_HOST",
];

pub(super) fn run(server: bool) -> Result<String, AppError> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or_else(|| error("HOME is required"))?);
    let destination = config::config_path()?;
    let parent = destination
        .parent()
        .ok_or_else(|| error("Invalid registry path"))?;
    private_directory(parent)?;
    let _lock = locked_file(&destination.with_extension("lock"))?;
    let mut registry = if destination.exists() {
        config::load()?
    } else {
        Registry::default()
    };
    let base = home.join(".config/remote-agents");
    let mut imported = Vec::new();
    for entry in fs::read_dir(base).map_err(|e| error(e.to_string()))? {
        let directory = entry.map_err(|e| error(e.to_string()))?.path();
        let Some(name) = directory.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        config::identifier(name)?;
        if registry.projects.contains_key(name) {
            continue;
        }
        let profile = directory.join(if server { "server.sh" } else { "client.sh" });
        if !profile.is_file() {
            continue;
        }
        registry
            .projects
            .insert(name.into(), read_project(&home, &profile, server)?);
        imported.push(name.to_owned());
    }
    config::validate(&registry)?;
    if !imported.is_empty() {
        if destination.exists() {
            fs::copy(
                &destination,
                destination.with_extension(format!("{}.bak", std::process::id())),
            )
            .map_err(|e| error(e.to_string()))?;
        }
        atomic_write(
            &destination,
            toml::to_string_pretty(&registry)
                .map_err(|e| error(e.to_string()))?
                .as_bytes(),
        )?;
    }
    Ok(format!(
        "Imported {} projects into {}; existing definitions and source profiles retained",
        imported.len(),
        destination.display()
    ))
}

fn read_project(home: &Path, profile: &Path, server: bool) -> Result<Project, AppError> {
    let fields = read_fields(profile)?;
    let values: BTreeMap<_, _> = KEYS
        .iter()
        .zip(&fields)
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    let get = |key: &str| values.get(key).copied().filter(|v| !v.is_empty());
    let root = PathBuf::from(
        get("REMOTE_ROOT")
            .or_else(|| {
                fields
                    .get(KEYS.len())
                    .map(String::as_str)
                    .filter(|v| !v.is_empty())
            })
            .ok_or_else(|| error("Profile has no root or slots"))?,
    );
    let mut directories = fields[KEYS.len()..]
        .iter()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if server {
        let panes = get("EZM_CONFIG")
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str::<toml::Value>(&s).ok())
            .and_then(|v| v.get("panes").and_then(toml::Value::as_integer))
            .unwrap_or(1);
        let stem = root
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| error("Invalid project root"))?
            .trim_end_matches("-1");
        for n in 1..=panes {
            let path = root.with_file_name(format!("{stem}-{n}"));
            directories.push(if path.is_dir() { path } else { root.clone() });
        }
    }
    if directories.is_empty() {
        return Err(error("No slots in profile"));
    }
    let slots = directories
        .into_iter()
        .map(|directory| {
            let command = if get("REMOTE_CODEX_WORKTREE") == directory.to_str() {
                get("REMOTE_CODEX_BIN").map(config::shell_quote)
            } else {
                None
            };
            Slot {
                directory,
                require_existing: server && command.is_none(),
                command,
            }
        })
        .collect();
    let owner = if server {
        get("REMOTE_SESSION_PREFIX")
            .map(existing_owner)
            .transpose()?
            .flatten()
    } else {
        None
    };
    let credentials = if server {
        Some(credential_reference(home, profile)?)
    } else {
        None
    };
    let perles = get("REMOTE_PERLES_BIN").map(|binary| {
        let script = format!(
            "set -a; source {} || exit; set +a; export PATH=\"$(dirname \"$REMOTE_BD_BIN\"):$PATH\"; exec {} --beads-dir {}",
            config::shell_quote(&profile.display().to_string()),
            config::shell_quote(binary),
            config::shell_quote(&root.join(".beads").display().to_string())
        );
        Auxiliary {
            directory: root.clone(),
            command: format!("bash -c {}", config::shell_quote(&script)),
        }
    });
    Ok(Project {
        root,
        host: if server {
            None
        } else {
            get("REMOTE_HOST").map(str::to_owned)
        },
        remote_binary: None,
        server: if server {
            get("OPENCODE_SERVER_URL").map(str::to_owned)
        } else {
            None
        },
        service: get("REMOTE_SERVICE").map(str::to_owned),
        credentials,
        opencode_binary: get("REMOTE_OC_BIN").map(PathBuf::from),
        perles,
        groups: BTreeMap::from([("main".into(), Group { owner, slots })]),
    })
}
fn existing_owner(prefix: &str) -> Result<Option<String>, AppError> {
    let names = runtime::tmux(&["list-sessions", "-F", "#{session_name}"]).unwrap_or_default();
    let matches = names
        .lines()
        .filter(|n| n.starts_with(prefix) && !n.contains("__"))
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(error(
            "Multiple legacy workspaces match profile; refusing ambiguous import",
        ));
    }
    Ok(matches.first().map(|s| (*s).to_owned()))
}
fn credential_reference(home: &Path, profile: &Path) -> Result<PathBuf, AppError> {
    let source = fs::read_to_string(profile).map_err(|e| error(e.to_string()))?;
    for line in source.lines() {
        if let Some(path) = line.trim().strip_prefix("source ") {
            let path = path
                .trim_matches(['\'', '"'])
                .replace("$HOME", &home.display().to_string());
            let path = PathBuf::from(path);
            if path.is_absolute() && path.is_file() {
                return Ok(path);
            }
        }
    }
    Err(error(
        "No private credential file reference found in server profile",
    ))
}

fn read_fields(profile: &Path) -> Result<Vec<String>, AppError> {
    // Source only the user's existing trusted profiles. Emit an allowlist of
    // non-secret fields, never env wholesale or credential values.
    let script = format!(
        "source \"$1\" || exit; {}; printf '%s\\0' \\\"${{REMOTE_SLOT_DIRS[@]}}\\\"",
        KEYS.iter()
            .map(|k| format!("printf '%s\\0' \\\"${{{k}:-}}\\\""))
            .collect::<Vec<_>>()
            .join("; ")
    );
    let script = script.replace("\\\"", "\"");
    let output = Command::new("bash")
        .args(["-c", &script, "ezm-import"])
        .arg(profile)
        .output()
        .map_err(|_| error("Could not read trusted profile"))?;
    if !output.status.success() {
        return Err(error("Profile could not be loaded"));
    }
    let fields = output
        .stdout
        .split(|b| *b == 0)
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .collect::<Vec<_>>();
    if fields.len() < KEYS.len() {
        return Err(error("Profile export was incomplete"));
    }
    Ok(fields)
}
