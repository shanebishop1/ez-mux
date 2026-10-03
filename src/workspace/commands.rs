use super::backend::{credentials, health, service, start};
use super::{
    WorkspaceCommand,
    config::{self, Group, Project, shell_quote},
    runtime,
};
use crate::{app::AppError, opencode::error};
use std::{fmt::Write as _, process::Command};

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

pub(super) fn execute(command: WorkspaceCommand) -> Result<String, AppError> {
    if let WorkspaceCommand::ImportRemoteAgents { server } = command {
        return super::import::run(server);
    }
    let registry = config::load()?;
    let (name, local) = match &command {
        WorkspaceCommand::Open { project, local, .. }
        | WorkspaceCommand::Start { project, local }
        | WorkspaceCommand::Attach { project, local, .. }
        | WorkspaceCommand::New { project, local, .. }
        | WorkspaceCommand::Adopt { project, local, .. }
        | WorkspaceCommand::Doctor { project, local }
        | WorkspaceCommand::Close { project, local, .. }
        | WorkspaceCommand::Stop { project, local } => (Some(project.as_str()), *local),
        WorkspaceCommand::ImportRemoteAgents { .. } => unreachable!(),
        WorkspaceCommand::Status { project, local, .. } => (project.as_deref(), *local),
    };
    let Some(name) = name else {
        if !matches!(command, WorkspaceCommand::Status { all: true, .. }) {
            return Err(error("Specify a project or --all"));
        }
        let mut lines = Vec::new();
        for name in registry.projects.keys() {
            match execute(WorkspaceCommand::Status {
                project: Some(name.clone()),
                all: false,
                local,
            }) {
                Ok(line) => lines.push(line),
                Err(e) => lines.push(format!("{name}: {e}")),
            }
        }
        return Ok(lines.join("\n"));
    };
    let owned_name = name.to_owned();
    let name = owned_name.as_str();
    let project = registry
        .projects
        .get(name)
        .ok_or_else(|| error(format!("Unknown project: {name}")))?;
    if let Some(host) = project.host.as_deref().filter(|_| !local) {
        return remote(host, project, &command);
    }
    dispatch(name, project, command)
}

fn dispatch(name: &str, project: &Project, command: WorkspaceCommand) -> Result<String, AppError> {
    match command {
        WorkspaceCommand::Start { .. } => {
            let _lock = runtime::workspace_lock(name, project)?;
            start(project)?;
            Ok(format!("{name}: backend ready"))
        }
        WorkspaceCommand::Attach { group, slot, .. } => {
            let (group_name, _) = selected_group(project, group.as_deref())?;
            start(project)?;
            agent(name, group_name, slot)
        }
        WorkspaceCommand::ImportRemoteAgents { .. } => unreachable!(),
        WorkspaceCommand::Open {
            project: name,
            group,
            slot,
            no_attach,
            ..
        } => {
            let (group_name, group) = selected_group(project, group.as_deref())?;
            runtime::open(&name, project, group_name, group, slot, !no_attach)?;
            Ok(format!("{name}/{group_name}: ready"))
        }
        WorkspaceCommand::New {
            project: name,
            group,
            slot,
            ..
        } => new_conversation(project, &name, group.as_deref(), slot, None),
        WorkspaceCommand::Adopt {
            project: name,
            group,
            slot,
            session,
            ..
        } => new_conversation(project, &name, group.as_deref(), slot, Some(&session)),
        WorkspaceCommand::Status { .. } => status(name, project, false),
        WorkspaceCommand::Doctor { .. } => status(name, project, true),
        WorkspaceCommand::Close { group, .. } => {
            if let Some(group) = group.as_deref() {
                selected_group(project, Some(group))?;
            }
            runtime::close(name, project, group.as_deref())?;
            Ok(format!(
                "{name}: views closed; backend and conversations retained"
            ))
        }
        WorkspaceCommand::Stop { .. } => {
            let _lock = runtime::close(name, project, None)?;
            service(project, "stop")?;
            Ok(format!(
                "{name}: views closed and configured backend stopped; history retained"
            ))
        }
    }
}

fn remote(host: &str, project: &Project, command: &WorkspaceCommand) -> Result<String, AppError> {
    let binary = project.remote_binary.as_deref().unwrap_or(".local/bin/ezm");
    let (verb, name, group, slot, no_attach) = match command {
        WorkspaceCommand::Open {
            project,
            group,
            slot,
            no_attach,
            ..
        } => ("open", project, group.as_deref(), *slot, *no_attach),
        WorkspaceCommand::Start { project, .. } => ("start", project, None, None, true),
        WorkspaceCommand::Attach {
            project,
            group,
            slot,
            ..
        } => ("attach", project, group.as_deref(), Some(*slot), false),
        WorkspaceCommand::New {
            project,
            group,
            slot,
            ..
        } => ("new", project, group.as_deref(), Some(*slot), false),
        WorkspaceCommand::Adopt {
            project,
            group,
            slot,
            ..
        } => ("adopt", project, group.as_deref(), Some(*slot), true),
        WorkspaceCommand::Status {
            project: Some(project),
            ..
        } => ("status", project, None, None, true),
        WorkspaceCommand::Doctor { project, .. } => ("doctor", project, None, None, true),
        WorkspaceCommand::Close { project, group, .. } => {
            ("close", project, group.as_deref(), None, true)
        }
        WorkspaceCommand::Stop { project, .. } => ("stop", project, None, None, true),
        WorkspaceCommand::Status { .. } | WorkspaceCommand::ImportRemoteAgents { .. } => {
            return Err(error("Missing remote project"));
        }
    };
    let mut args = vec![
        binary.to_owned(),
        verb.into(),
        name.clone(),
        "--local".into(),
    ];
    if let WorkspaceCommand::Adopt { session, .. } = command {
        args.extend(["--session".into(), session.clone()]);
    }
    if let Some(group) = group {
        args.extend(["--group".into(), group.into()]);
    }
    if let Some(slot) = slot {
        args.extend(["--slot".into(), slot.to_string()]);
    }
    if no_attach && verb == "open" {
        args.push("--no-attach".into());
    }
    let mut cmd = Command::new("ssh");
    cmd.args(["-o", "ConnectTimeout=10"]);
    if (verb == "open" && !no_attach) || verb == "attach" {
        cmd.arg("-t");
    }
    let status = cmd
        .arg(host)
        .arg(
            args.iter()
                .map(|s| shell_quote(s))
                .collect::<Vec<_>>()
                .join(" "),
        )
        .status()
        .map_err(|e| error(e.to_string()))?;
    if !status.success() {
        return Err(error("Remote ezm operation failed"));
    }
    Ok(String::new())
}

fn status(name: &str, project: &Project, doctor: bool) -> Result<String, AppError> {
    let api = health(project);
    let mut text = format!(
        "{name}: API {}",
        api.as_ref().map_or_else(ToString::to_string, Clone::clone)
    );
    if let Some(service) = &project.service {
        let out = Command::new("systemctl")
            .args(["--user", "is-active", service])
            .output()
            .map_err(|e| error(e.to_string()))?;
        let _ = write!(
            text,
            "; service {}",
            String::from_utf8_lossy(&out.stdout).trim()
        );
    }
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
    if doctor {
        if !project.root.is_dir()
            || project
                .groups
                .values()
                .flat_map(|g| &g.slots)
                .any(|s| !s.directory.is_dir())
        {
            return Err(error(format!("{text}\nMissing project/slot directory")));
        }
        api.map_err(|_| error(text.clone()))?;
    }
    Ok(text)
}
fn agent_command(
    project: &Project,
    name: &str,
    group: &str,
    slot: u8,
    directory: &str,
) -> Result<Command, AppError> {
    let server = project
        .server
        .as_deref()
        .ok_or_else(|| error("Configure an OpenCode server URL or a slot command"))?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| error(e.to_string()))?);
    command.envs(credentials(project)?);
    if let Some(binary) = &project.opencode_binary {
        command.env("EZM_OPENCODE_BIN", binary);
    }
    command.args([
        "__internal",
        "opencode",
        "--server",
        server,
        "--directory",
        directory,
        "--key",
        &format!("{name}/{group}/{slot}"),
    ]);
    Ok(command)
}
pub(super) fn agent(name: &str, group_name: &str, slot_id: u8) -> Result<String, AppError> {
    let registry = config::load()?;
    let owned_name = name.to_owned();
    let name = owned_name.as_str();
    let project = registry
        .projects
        .get(name)
        .ok_or_else(|| error("Unknown project"))?;
    let (_, group) = selected_group(project, Some(group_name))?;
    let slot = group
        .slots
        .get(usize::from(slot_id).wrapping_sub(1))
        .ok_or_else(|| error("Slot is not configured in this group"))?;
    let mut launch = agent_command(
        project,
        name,
        group_name,
        slot_id,
        &slot.directory.display().to_string(),
    );
    let status = if let Some(command) = &slot.command {
        Command::new("sh")
            .args(["-c", command])
            .current_dir(&slot.directory)
            .status()
    } else {
        let cmd = launch.as_mut().map_err(|e| error(e.to_string()))?;
        if slot.require_existing {
            cmd.arg("--require-existing");
        }
        cmd.status()
    }
    .map_err(|e| error(e.to_string()))?;
    if !status.success() {
        return Err(error(
            "Agent exited unsuccessfully; saved conversation retained",
        ));
    }
    Ok(String::new())
}

fn new_conversation(
    project: &Project,
    name: &str,
    group: Option<&str>,
    slot: u8,
    adopt: Option<&str>,
) -> Result<String, AppError> {
    let (group_name, group) = selected_group(project, group)?;
    let selected = group
        .slots
        .get(usize::from(slot).wrapping_sub(1))
        .ok_or_else(|| error("Slot is not configured"))?;
    if selected.command.is_some() {
        return Err(error("New conversation requires an OpenCode slot"));
    }
    let _lock = runtime::workspace_lock(name, project)?;
    start(project)?;
    let mut cmd = agent_command(
        project,
        name,
        group_name,
        slot,
        &selected.directory.display().to_string(),
    )?;
    if let Some(id) = adopt {
        cmd.args(["--adopt", id]);
    } else {
        cmd.arg("--new");
    }
    let out = cmd
        .arg("--prepare-only")
        .output()
        .map_err(|e| error(e.to_string()))?;
    if !out.status.success() {
        return Err(error(
            "Could not create new conversation; existing view retained",
        ));
    }
    Ok(format!(
        "{name}/{group_name}/{slot}: saved conversation {}; existing terminal unchanged (close/reopen to use it)",
        String::from_utf8_lossy(&out.stdout).trim()
    ))
}
