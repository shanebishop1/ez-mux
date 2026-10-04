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
    #[serde(default)]
    pub root: PathBuf,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub remote_binary: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub credentials: Option<PathBuf>,
    #[serde(default)]
    pub opencode_binary: Option<PathBuf>,
    #[serde(default)]
    pub perles: Option<Auxiliary>,
    #[serde(default)]
    pub groups: BTreeMap<String, Group>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Auxiliary {
    pub directory: PathBuf,
    pub command: String,
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub panes: Option<u8>,
    #[serde(skip)]
    pub discovered: bool,
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
        if let Some(host) = &project.host {
            crate::session::validate_remote_ssh_authority(host)?;
            continue;
        }
        if !project.root.is_absolute() {
            return Err(error("Project roots must be absolute"));
        }
        if let Some(host) = &project.host {
            crate::session::validate_remote_ssh_authority(host)?;
        }
        if let Some(url) = &project.server {
            crate::config::validate_server_url(url, "project registry")?;
        }
        for (name, group) in &project.groups {
            identifier(name)?;
            if let Some(owner) = &group.owner {
                identifier(owner)?;
                if !owners.insert(owner) {
                    return Err(error("An owner session cannot belong to multiple groups"));
                }
            }
            if name == "perles"
                || group.slots.len() > 5
                || group.panes.is_some_and(|p| !(1..=5).contains(&p))
            {
                return Err(error("Groups need 1..5 slots; perles is reserved"));
            }
            if !group.slots.is_empty()
                && group
                    .panes
                    .is_some_and(|p| usize::from(p) > group.slots.len())
            {
                return Err(error(
                    "Explicit groups cannot request more panes than configured slots",
                ));
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

pub(super) fn settings(project: &Project) -> Result<crate::config::RuntimeContext, AppError> {
    let env = crate::config::ProcessEnv;
    let loaded = crate::config::load_config_with_current_dir(
        &env,
        crate::config::OperatingSystem::current(),
        Some(&project.root),
    )?;
    Ok(crate::config::resolve_runtime_context(
        &env,
        &loaded.values,
    )?)
}

pub(super) fn resolve(name: &str, definition: &Project) -> Result<Project, AppError> {
    let mut project = definition.clone();
    if !project.root.is_absolute() {
        return Err(error("Execution host needs an absolute project root"));
    }
    let loaded = crate::config::load_config_with_current_dir(
        &crate::config::ProcessEnv,
        crate::config::OperatingSystem::current(),
        Some(&project.root),
    )?;
    let default_panes = crate::config::resolve_pane_count(None, &loaded.values)?.value;
    if project.groups.is_empty() {
        project.groups.insert("main".into(), Group::default());
    }
    // Discover only on creation. Once open, the owner retains its assignments.
    let snapshot = project.clone();
    for (group_name, group) in &mut project.groups {
        if group.slots.is_empty() {
            group.discovered = true;
            group.panes = Some(group.panes.unwrap_or(default_panes));
            let (_, owner) = super::runtime::identities(name, &snapshot, group_name);
            let saved = if super::runtime::exists(&owner) {
                super::runtime::option(&owner, "@ezm_explicit_worktrees")?
            } else {
                String::new()
            };
            let paths: Vec<PathBuf> = if saved.is_empty() {
                crate::session::discover_workspace_worktrees(&project.root)?
            } else {
                serde_json::from_str(&saved)
                    .map_err(|_| error("Invalid live worktree assignments"))?
            };
            group.slots = paths
                .into_iter()
                .take(5)
                .map(|directory| Slot {
                    directory,
                    command: None,
                    require_existing: false,
                })
                .collect();
        }
    }
    Ok(project)
}

impl Group {
    pub(super) fn pane_count(&self) -> usize {
        self.panes.map_or(self.slots.len(), usize::from)
    }
}
