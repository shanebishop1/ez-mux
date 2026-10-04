use crate::{app::AppError, workspace::state::error};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub name: String,
    #[serde(skip)]
    pub manifest: PathBuf,
    #[serde(default)]
    pub root: PathBuf,
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
    pub command: Option<String>,
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
    pub directory: PathBuf,
    #[serde(default)]
    pub command: Option<String>,
}

pub fn load(file: &std::path::Path) -> Result<Project, AppError> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| error(format!("Cannot read workspace manifest: {e}")))?;
    let mut project: Project =
        serde_json::from_str(&source).map_err(|_| error("Invalid workspace manifest JSON"))?;
    identifier(&project.name)?;
    if !project.root.is_absolute() {
        return Err(error("Workspace root must be absolute"));
    }
    project.manifest = file.canonicalize().map_err(|e| error(e.to_string()))?;
    for (name, group) in &project.groups {
        identifier(name)?;
        if name == "perles"
            || group.slots.len() > 5
            || group.panes.is_some_and(|n| !(1..=5).contains(&n))
        {
            return Err(error("Groups need 1..5 slots; perles is reserved"));
        }
        for slot in &group.slots {
            if !slot.directory.is_absolute() {
                return Err(error("Each group slot needs an absolute worktree path"));
            }
        }
    }
    resolve(&project.name, &project)
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
            let (_, owner) = super::runtime::identities(name, &snapshot, group_name)?;
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
