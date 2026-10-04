use super::{
    WorkspaceCommand,
    config::{self, Group, Project},
    runtime,
    state::error,
};
use crate::app::AppError;
use std::{fmt::Write as _, path::Path, process::Command};
fn selected_group<'a>(
    project: &'a Project,
    name: Option<&str>,
) -> Result<(&'a str, &'a Group), AppError> {
    let name = name
        .or_else(|| project.groups.contains_key("main").then_some("main"))
        .or_else(|| project.groups.keys().next().map(String::as_str))
        .ok_or_else(|| error("No groups configured"))?;
    project
        .groups
        .get_key_value(name)
        .map(|(k, v)| (k.as_str(), v))
        .ok_or_else(|| error("Unknown group"))
}

pub(super) fn execute(file: &Path, command: WorkspaceCommand) -> Result<String, AppError> {
    let project = config::load(file)?;
    let name = &project.name;
    match command {
        WorkspaceCommand::Open {
            group,
            slot,
            no_attach,
        } => {
            let (group_name, group) = selected_group(&project, group.as_deref())?;
            runtime::open(name, &project, group_name, group, slot, !no_attach)?;
            Ok(format!("{name}/{group_name}: ready"))
        }
        WorkspaceCommand::Status => status(name, &project),
        WorkspaceCommand::Close { group } => {
            if let Some(group) = group.as_deref() {
                selected_group(&project, Some(group))?;
            }
            let _lock = runtime::close(name, &project, group.as_deref())?;
            Ok(format!("{name}: terminal views closed"))
        }
    }
}

pub(super) fn agent(file: &Path, group_name: &str, slot_id: u8) -> Result<String, AppError> {
    let project = config::load(file)?;
    let (_, group) = selected_group(&project, Some(group_name))?;
    let slot = group
        .slots
        .get(usize::from(slot_id).wrapping_sub(1))
        .ok_or_else(|| error("Slot is not configured in this group"))?;
    let mut settings = config::settings(&project)?;
    if let Some(command) = slot.command.as_ref().or(group.command.as_ref()) {
        settings.agent_command = Some(command.clone());
    }
    let command = crate::app::workspace_agent_launch(&slot.directory, slot_id, &settings)?;
    let status = Command::new("sh")
        .args(["-c", &command])
        .env("EZM_SLOT", slot_id.to_string())
        .current_dir(&slot.directory)
        .status()
        .map_err(|e| error(e.to_string()))?;
    if !status.success() {
        return Err(error("Slot command exited unsuccessfully"));
    }
    Ok(String::new())
}

fn status(name: &str, project: &Project) -> Result<String, AppError> {
    let mut text = name.to_owned();
    for (group_name, group) in &project.groups {
        let (_, owner) = runtime::identities(name, project, group_name);
        let running = runtime::exists(&owner);
        let _ = write!(
            text,
            "\n  {group_name}: {}",
            if running { "open" } else { "closed" }
        );
        for (index, slot) in group.slots.iter().enumerate() {
            let pane = if running {
                runtime::option(&owner, &format!("@ezm_slot_{}_pane", index + 1))?
            } else {
                String::new()
            };
            let process = if pane.is_empty() {
                "no view".into()
            } else {
                runtime::tmux(&[
                    "display-message",
                    "-p",
                    "-t",
                    &pane,
                    "#{pane_current_command} (dead=#{pane_dead})",
                ])?
            };
            let _ = write!(
                text,
                "\n    {}: {} — {process}",
                index + 1,
                slot.directory.display()
            );
        }
    }
    Ok(text)
}
