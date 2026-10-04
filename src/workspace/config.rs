use crate::{app::AppError, opencode::error};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub projects: BTreeMap<String, Project>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub root: PathBuf,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub remote_binary: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub credentials: Option<PathBuf>,
    #[serde(default)]
    pub opencode_binary: Option<PathBuf>,
    #[serde(default)]
    pub perles: Option<Auxiliary>,
    pub groups: BTreeMap<String, Group>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Auxiliary {
    pub directory: PathBuf,
    pub command: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    #[serde(default)]
    pub owner: Option<String>,
    pub slots: Vec<Slot>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    #[serde(default)]
    pub require_existing: bool,
    pub directory: PathBuf,
    #[serde(default)]
    pub command: Option<String>,
}

pub fn config_path() -> Result<PathBuf, AppError> {
    if let Some(path) = std::env::var_os("EZM_PROJECTS_CONFIG") {
        return Ok(path.into());
    }
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| error("HOME or XDG_CONFIG_HOME is required"))?;
    Ok(root.join("ez-mux/projects.toml"))
}

pub fn load() -> Result<Registry, AppError> {
    let path = config_path()?;
    let source = std::fs::read_to_string(&path).map_err(|e| {
        error(format!(
            "Cannot read project registry {}: {e}",
            path.display()
        ))
    })?;
    let registry: Registry = toml::from_str(&source)
        .map_err(|_| error("Invalid project registry TOML; see docs/workspaces.md"))?;
    validate(&registry)?;
    Ok(registry)
}

pub fn validate(registry: &Registry) -> Result<(), AppError> {
    let mut owners = std::collections::BTreeSet::new();
    for (name, project) in &registry.projects {
        identifier(name)?;
        if !project.root.is_absolute() {
            return Err(error("Project roots must be absolute"));
        }
        if let Some(host) = &project.host {
            crate::session::validate_remote_ssh_authority(host)?;
        }
        if let Some(url) = &project.server {
            crate::config::validate_server_url(url, "project registry")?;
        }
        if project.groups.is_empty() {
            return Err(error("Each project needs at least one group"));
        }
        for (name, group) in &project.groups {
            identifier(name)?;
            if let Some(owner) = &group.owner {
                identifier(owner)?;
                if !owners.insert(owner) {
                    return Err(error("An owner session cannot belong to multiple groups"));
                }
            }
            if name == "perles" || !(1..=5).contains(&group.slots.len()) {
                return Err(error("Groups need 1..5 slots; perles is reserved"));
            }
            let mut paths = std::collections::BTreeSet::new();
            for slot in &group.slots {
                if !slot.directory.is_absolute() || !paths.insert(&slot.directory) {
                    return Err(error(
                        "Each group slot needs a distinct absolute worktree path",
                    ));
                }
            }
        }
    }
    Ok(())
}
pub fn identifier(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > 64
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(error(
            "Names must use letters, digits, hyphens or underscores (max 64)",
        ));
    }
    Ok(())
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
