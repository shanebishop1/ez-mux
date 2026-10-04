use super::config::{Group, Project, shell_quote};
use crate::{
    app::AppError,
    session::{ProcessTmuxClient, TmuxClient},
    workspace::state::{error, stable_key},
};
use std::process::Command;

pub(super) fn tmux(args: &[&str]) -> Result<String, AppError> {
    let out = Command::new("tmux")
        .args(args)
        .output()
        .map_err(|e| error(e.to_string()))?;
    if !out.status.success() {
        return Err(error(format!(
            "tmux {} failed: {}",
            args.first().unwrap_or(&"operation"),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}
pub(super) fn exists(name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", &format!("={name}")])
        .output()
        .is_ok_and(|o| o.status.success())
}
pub(super) fn identities(name: &str, project: &Project, group: &str) -> (String, String) {
    let key = stable_key(&format!("{name}\0{}", project.root.display()));
    (
        format!("ezm-project-{name}-{key}"),
        project
            .groups
            .get(group)
            .and_then(|g| g.owner.clone())
            .unwrap_or_else(|| format!("ezm-group-{key}-{group}")),
    )
}
pub(super) fn option(target: &str, key: &str) -> Result<String, AppError> {
    tmux(&["show-options", "-qv", "-t", target, key])
}
pub(super) fn set(target: &str, key: &str, value: &str) -> Result<(), AppError> {
    tmux(&["set-option", "-t", target, key, value]).map(|_| ())
}

pub(super) fn workspace_lock(name: &str, project: &Project) -> Result<std::fs::File, AppError> {
    let (parent, _) = identities(name, project, "");
    let state = super::state::state_root()?.join("workspaces");
    super::state::private_directory(&state)?;
    super::state::locked_file(&state.join(format!("{parent}.lock")))
}

pub(super) fn open(
    name: &str,
    project: &Project,
    group_name: &str,
    group: &Group,
    slot: Option<u8>,
    attach: bool,
) -> Result<(), AppError> {
    let (parent, owner) = identities(name, project, group_name);
    let lock = workspace_lock(name, project)?;
    if exists(&parent) && option(&parent, "@ezm_workspace_project")? != name {
        return Err(error("Project session identity conflict"));
    }
    if exists(&owner) {
        validate_owner(name, group_name, group, &owner)?;
    }
    if !project.root.is_dir() || group.slots.iter().any(|s| !s.directory.is_dir()) {
        return Err(error(
            "Project or slot directory is missing on execution host",
        ));
    }
    if let Some(slot) = slot {
        if slot == 0 || usize::from(slot) > group.pane_count() {
            return Err(error("Slot is not configured in this group"));
        }
    }
    ensure_owner(name, project, group_name, group, &owner)?;
    let window = option(&owner, "@ezm_canonical_window_id")?;
    if window.is_empty() {
        return Err(error(
            "Group has no canonical window; inspect or repair before reopening",
        ));
    }
    tmux(&[
        "set-window-option",
        "-t",
        &window,
        "@ezm_group_owner",
        &owner,
    ])?;
    tmux(&[
        "set-window-option",
        "-t",
        &window,
        "@ezm_slot_1_pane",
        &option(&owner, "@ezm_slot_1_pane")?,
    ])?;
    tmux(&[
        "set-window-option",
        "-t",
        &window,
        "automatic-rename",
        "off",
    ])?;
    tmux(&["set-window-option", "-t", &window, "allow-rename", "off"])?;
    tmux(&["rename-window", "-t", &window, group_name])?;
    let created_parent = !exists(&parent);
    if created_parent {
        tmux(&[
            "new-session",
            "-d",
            "-s",
            &parent,
            "-n",
            "starting",
            "-c",
            &project.root.display().to_string(),
        ])?;
        set(&parent, "@ezm_workspace_project", name)?;
    }
    let linked = tmux(&["list-windows", "-t", &parent, "-F", "#{window_id}"])?;
    if !linked.lines().any(|id| id == window) {
        tmux(&[
            "link-window",
            "-s",
            &format!("{owner}:{window}"),
            "-t",
            &format!("{parent}:"),
            "-a",
        ])?;
    }
    if created_parent {
        tmux(&["kill-window", "-t", &format!("{parent}:starting")])?;
    }
    ensure_perles(project, &parent, &owner)?;
    layout(&["workspace-bindings"])?;
    tmux(&["select-window", "-t", &format!("{parent}:{window}")])?;
    if let Some(slot) = slot {
        layout(&["focus", "--session", &owner, "--slot", &slot.to_string()])?;
    }
    drop(lock);
    if attach {
        ProcessTmuxClient.attach_session(&parent)?;
    }
    Ok(())
}

fn ensure_perles(project: &Project, parent: &str, owner: &str) -> Result<(), AppError> {
    if let Some(aux) = &project.perles {
        let windows = tmux(&["list-windows", "-t", parent, "-F", "#{window_name}"])?;
        if !windows.lines().any(|w| w == "perles") {
            let existing = tmux(&[
                "list-windows",
                "-t",
                owner,
                "-F",
                "#{window_id}|#{window_name}",
            ])?;
            if let Some((id, _)) = existing
                .lines()
                .filter_map(|line| line.split_once('|'))
                .find(|(_, name)| *name == "perles")
            {
                tmux(&[
                    "link-window",
                    "-s",
                    &format!("{owner}:{id}"),
                    "-t",
                    &format!("{parent}:"),
                ])?;
            } else {
                tmux(&[
                    "new-window",
                    "-d",
                    "-t",
                    &format!("{parent}:"),
                    "-n",
                    "perles",
                    "-c",
                    &aux.directory.display().to_string(),
                    &aux.command,
                ])?;
            }
        }
    }
    Ok(())
}

pub(super) fn close(
    name: &str,
    project: &Project,
    group: Option<&str>,
) -> Result<std::fs::File, AppError> {
    let (parent, _) = identities(name, project, "");
    let lock = workspace_lock(name, project)?;
    if exists(&parent) && option(&parent, "@ezm_workspace_project")? != name {
        return Err(error("Project session identity conflict"));
    }
    // Validate the entire close scope before the first mutation.
    for (group_name, definition) in project
        .groups
        .iter()
        .filter(|(g, _)| group.is_none_or(|wanted| g.as_str() == wanted))
    {
        let (_, owner) = identities(name, project, group_name);
        if exists(&owner) {
            validate_owner(name, group_name, definition, &owner)?;
        }
    }
    for group_name in project
        .groups
        .keys()
        .filter(|g| group.is_none_or(|wanted| g.as_str() == wanted))
    {
        let (_, owner) = identities(name, project, group_name);
        if exists(&owner) {
            let window = option(&owner, "@ezm_canonical_window_id")?;
            // Unlink only our presentation window before destroying its owner.
            if exists(&parent) {
                let _ = tmux(&["unlink-window", "-k", "-t", &format!("{parent}:{window}")]);
            }
            layout(&["teardown", "--session", &owner])?;
        }
    }
    if group.is_none() && exists(&parent) {
        tmux(&["kill-session", "-t", &parent])?;
    }
    Ok(lock)
}

fn ensure_owner(
    name: &str,
    project: &Project,
    group_name: &str,
    group: &Group,
    owner: &str,
) -> Result<(), AppError> {
    let executable = std::env::current_exe().map_err(|e| error(e.to_string()))?;
    let registry = &project.manifest;
    if exists(owner) {
        validate_owner(name, group_name, group, owner)?;
        let saved = option(owner, "@ezm_explicit_worktrees")?;
        let expected =
            serde_json::to_string(&group.slots.iter().map(|s| &s.directory).collect::<Vec<_>>())
                .map_err(|e| error(e.to_string()))?;
        if saved.is_empty() && group.owner.as_deref() == Some(owner) {
            for (index, slot) in group.slots.iter().enumerate() {
                if option(owner, &format!("@ezm_slot_{}_worktree", index + 1))?
                    != slot.directory.display().to_string()
                {
                    return Err(error(
                        "Legacy group worktree does not match configured slot; refusing adoption",
                    ));
                }
            }
            set(owner, "@ezm_explicit_worktrees", &expected)?;
            set(owner, "@ezm_workspace_project", name)?;
            set(owner, "@ezm_workspace_group", group_name)?;
            let launch = agent_launch(registry, &executable, name, group_name);
            set(owner, "@ezm_runtime_agent_command", &launch)?;
        } else if saved != expected {
            return Err(error(
                "Live group worktrees differ from config; close that group explicitly before changing its slots",
            ));
        }
    } else {
        ProcessTmuxClient.create_detached_session(owner, &project.root)?;
        let result = (|| {
            set(owner, "@ezm_workspace_project", name)?;
            set(owner, "@ezm_workspace_group", group_name)?;
            set(
                owner,
                "@ezm_explicit_worktrees",
                &serde_json::to_string(
                    &group.slots.iter().map(|s| &s.directory).collect::<Vec<_>>(),
                )
                .map_err(|e| error(e.to_string()))?,
            )?;
            // Each helper resolves its own trusted registry and credentials, never
            // the tmux server's global environment or a different project's PATH.
            let launch = agent_launch(registry, &executable, name, group_name);
            let managed =
                group.command.is_some() || group.slots.iter().any(|s| s.command.is_some());
            let mut args = vec![
                "group-layout".to_owned(),
                "--session".into(),
                owner.into(),
                "--directory".into(),
                project.root.display().to_string(),
                "--panes".into(),
                group.pane_count().to_string(),
            ];
            if managed {
                args.extend(["--agent-command".into(), launch]);
            }
            layout_in(
                &args.iter().map(String::as_str).collect::<Vec<_>>(),
                Some(&project.root),
            )?;
            Ok::<(), AppError>(())
        })();
        if let Err(e) = result {
            let _ = layout(&["teardown", "--session", owner]);
            return Err(e);
        }
    }

    if group.command.is_some() || group.slots.iter().any(|s| s.command.is_some()) {
        set(
            owner,
            "@ezm_runtime_agent_command",
            &agent_launch(registry, &executable, name, group_name),
        )?;
    }
    Ok(())
}

fn validate_owner(
    name: &str,
    group_name: &str,
    group: &Group,
    owner: &str,
) -> Result<(), AppError> {
    let project_marker = option(owner, "@ezm_workspace_project")?;
    let group_marker = option(owner, "@ezm_workspace_group")?;
    if !project_marker.is_empty() || !group_marker.is_empty() {
        if project_marker != name || group_marker != group_name {
            return Err(error("Group owner belongs to a different project/group"));
        }
    } else if group.owner.as_deref() != Some(owner) {
        return Err(error("Unowned group session; refusing mutation"));
    }
    for (index, slot) in group.slots.iter().enumerate() {
        if option(owner, &format!("@ezm_slot_{}_worktree", index + 1))?
            != slot.directory.display().to_string()
        {
            return Err(error(
                "Owner worktrees differ from group definition; refusing mutation",
            ));
        }
    }
    // Discovery preserves ezm's ordinary empty visible slots, owned by this group.
    if group.discovered && project_marker == name && group_marker == group_name {
        return Ok(());
    }
    // Extra visible slots would be work outside this group's claimed scope.
    let live_panes = tmux(&["list-panes", "-a", "-F", "#{pane_id}"])?;
    for index in group.slots.len() + 1..=5 {
        if option(owner, &format!("@ezm_slot_{index}_suspended"))? != "1" {
            return Err(error(
                "Owner has additional live slots; refusing partial adoption or teardown",
            ));
        }
        if exists(&format!("{owner}__popup_slot_{index}"))
            || exists(&format!("{owner}__mode_slot_{index}"))
        {
            return Err(error(
                "Extra slot owns a helper session; refusing partial adoption or teardown",
            ));
        }
        for suffix in [
            "pane",
            "restore_pane",
            "backing_agent_pane",
            "backing_shell_pane",
            "backing_neovim_pane",
            "backing_lazygit_pane",
        ] {
            let pane = option(owner, &format!("@ezm_slot_{index}_{suffix}"))?;
            if !pane.is_empty() && live_panes.lines().any(|p| p == pane) {
                return Err(error(
                    "Extra slot retains a live pane; refusing partial adoption or teardown",
                ));
            }
        }
    }
    Ok(())
}

fn agent_launch(
    registry: &std::path::Path,
    executable: &std::path::Path,
    _name: &str,
    group: &str,
) -> String {
    format!(
        "{} __internal workspace-agent --file {} --group {} --slot {{slot}}; exec \"${{SHELL:-/bin/sh}}\" -l",
        shell_quote(&executable.display().to_string()),
        shell_quote(&registry.display().to_string()),
        shell_quote(group)
    )
}

fn layout(args: &[&str]) -> Result<(), AppError> {
    layout_in(args, None)
}
fn layout_in(args: &[&str], directory: Option<&std::path::Path>) -> Result<(), AppError> {
    let binary = std::env::current_exe().map_err(|e| error(e.to_string()))?;
    let mut command = Command::new(&binary);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let output = command
        .env("EZM_BIN", &binary)
        .arg("__internal")
        .args(args)
        .output()
        .map_err(|e| error(e.to_string()))?;
    if !output.status.success() {
        return Err(error(format!(
            "ezm layout operation failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
