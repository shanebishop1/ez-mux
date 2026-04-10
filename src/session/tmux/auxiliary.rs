use super::SessionError;
use super::command::{tmux_output_value, tmux_run};
use super::options::show_session_option;
use super::remote_authority::parse_remote_ssh_authority;
use super::remote_transport::{build_remote_invocation, remote_transport_label};
use super::slot_swap::validate_canonical_slot_registry;
use crate::config::{EZM_REMOTE_PATH_ENV, EZM_REMOTE_SERVER_URL_ENV};
use crate::session::resolve_remote_path;
use crate::session::{AuxiliaryViewerAction, AuxiliaryViewerOutcome};
use std::path::{Path, PathBuf};

const AUXILIARY_WINDOW_NAME: &str = "perles";
const PERLES_DIR_ENV: &str = "PERLES_DIR";
const PERLES_DB_ENV: &str = "PERLES_DB";
const LEGACY_BEADS_DIR_ENV: &str = "BEADS_DIR";
const LEGACY_BEADS_DB_ENV: &str = "BEADS_DB";

pub(super) fn auxiliary_viewer(
    session_name: &str,
    open: bool,
    use_tssh: bool,
    use_mosh: bool,
) -> Result<AuxiliaryViewerOutcome, SessionError> {
    let existing = find_window_id_by_name(session_name, AUXILIARY_WINDOW_NAME)?;
    if open {
        return open_auxiliary_viewer(session_name, existing, use_tssh, use_mosh);
    }

    close_auxiliary_viewer(session_name, existing)
}

fn open_auxiliary_viewer(
    session_name: &str,
    existing: Option<String>,
    use_tssh: bool,
    use_mosh: bool,
) -> Result<AuxiliaryViewerOutcome, SessionError> {
    if let Some(window_id) = existing {
        return Ok(AuxiliaryViewerOutcome {
            session_name: session_name.to_owned(),
            action: AuxiliaryViewerAction::Reused,
            window_name: String::from(AUXILIARY_WINDOW_NAME),
            window_id: Some(window_id),
        });
    }

    let cwd = resolve_auxiliary_cwd(session_name)?;
    let remote_path = std::env::var(EZM_REMOTE_PATH_ENV).ok();
    let remote_server_url = std::env::var(EZM_REMOTE_SERVER_URL_ENV).ok();
    let remote_launch = resolve_auxiliary_remote_launch(
        &cwd,
        remote_path.as_deref(),
        remote_server_url.as_deref(),
        use_tssh,
        use_mosh,
    )?;

    let local_perles_executable = if remote_launch.is_none() {
        discover_executable_in_path("perles")
    } else {
        None
    };

    if existing.is_none() && remote_launch.is_none() && local_perles_executable.is_none() {
        return Ok(AuxiliaryViewerOutcome {
            session_name: session_name.to_owned(),
            action: AuxiliaryViewerAction::SkippedUnavailable,
            window_name: String::from(AUXILIARY_WINDOW_NAME),
            window_id: None,
        });
    }

    if should_validate_registry_for_auxiliary(true) {
        validate_canonical_slot_registry(session_name)?;
    }

    let command = build_auxiliary_launch_command(remote_launch.as_ref(), local_perles_executable)?;
    let window_id = tmux_output_value(&[
        "new-window",
        "-d",
        "-t",
        &format!("{session_name}:"),
        "-n",
        AUXILIARY_WINDOW_NAME,
        "-c",
        &cwd,
        "-P",
        "-F",
        "#{window_id}",
        &command,
    ])?
    .trim()
    .to_owned();

    set_auxiliary_window_remain_on_exit(&window_id)?;

    Ok(AuxiliaryViewerOutcome {
        session_name: session_name.to_owned(),
        action: AuxiliaryViewerAction::Created,
        window_name: String::from(AUXILIARY_WINDOW_NAME),
        window_id: Some(window_id),
    })
}

fn close_auxiliary_viewer(
    session_name: &str,
    existing: Option<String>,
) -> Result<AuxiliaryViewerOutcome, SessionError> {
    if should_validate_registry_for_auxiliary(false) {
        validate_canonical_slot_registry(session_name)?;
    }

    if let Some(window_id) = existing {
        tmux_run(&["kill-window", "-t", &window_id])?;
        return Ok(AuxiliaryViewerOutcome {
            session_name: session_name.to_owned(),
            action: AuxiliaryViewerAction::Closed,
            window_name: String::from(AUXILIARY_WINDOW_NAME),
            window_id: Some(window_id),
        });
    }

    Ok(AuxiliaryViewerOutcome {
        session_name: session_name.to_owned(),
        action: AuxiliaryViewerAction::Closed,
        window_name: String::from(AUXILIARY_WINDOW_NAME),
        window_id: None,
    })
}

fn build_auxiliary_launch_command(
    remote_launch: Option<&AuxiliaryRemoteLaunch>,
    local_perles_executable: Option<PathBuf>,
) -> Result<String, SessionError> {
    if let Some(remote_launch) = remote_launch {
        return build_auxiliary_command_for_remote(remote_launch);
    }

    let perles_dir = std::env::var(PERLES_DIR_ENV)
        .ok()
        .or_else(|| std::env::var(LEGACY_BEADS_DIR_ENV).ok());
    let perles_db = std::env::var(PERLES_DB_ENV)
        .ok()
        .or_else(|| std::env::var(LEGACY_BEADS_DB_ENV).ok());
    let perles_executable =
        local_perles_executable.ok_or_else(|| SessionError::TmuxCommandFailed {
            command: String::from("auxiliary-viewer discover perles"),
            stderr: String::from("perles executable disappeared during startup reconciliation"),
        })?;
    Ok(build_auxiliary_local_launch_command(
        &perles_executable,
        perles_dir.as_deref(),
        perles_db.as_deref(),
    ))
}

fn build_auxiliary_command_for_remote(
    remote_launch: &AuxiliaryRemoteLaunch,
) -> Result<String, SessionError> {
    build_auxiliary_remote_launch_command(
        &remote_launch.remote_dir,
        &remote_launch.remote_server_url,
        remote_launch.use_tssh,
        remote_launch.use_mosh,
    )
}

fn set_auxiliary_window_remain_on_exit(window_id: &str) -> Result<(), SessionError> {
    if let Err(error) = tmux_run(&["set-option", "-w", "-t", window_id, "remain-on-exit", "on"]) {
        let no_such_window_race = matches!(
            &error,
            SessionError::TmuxCommandFailed { stderr, .. }
                if stderr.contains("no such window")
        );
        if !no_such_window_race {
            return Err(error);
        }
    }

    Ok(())
}

fn should_validate_registry_for_auxiliary(open: bool) -> bool {
    !open
}

fn resolve_auxiliary_cwd(session_name: &str) -> Result<String, SessionError> {
    if let Some(worktree) = show_session_option(session_name, "@ezm_slot_1_worktree")? {
        if !worktree.trim().is_empty() {
            return Ok(worktree);
        }
    }

    tmux_output_value(&[
        "display-message",
        "-p",
        "-t",
        session_name,
        "#{pane_current_path}",
    ])
    .map(|path| path.trim().to_owned())
}

fn find_window_id_by_name(
    session_name: &str,
    window_name: &str,
) -> Result<Option<String>, SessionError> {
    let output = tmux_output_value(&[
        "list-windows",
        "-t",
        session_name,
        "-F",
        "#{window_id}|#{window_name}",
    ])?;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split('|');
        let id = parts.next().unwrap_or_default().trim();
        let name = parts.next().unwrap_or_default().trim();
        if name == window_name {
            return Ok(Some(id.to_owned()));
        }
    }

    Ok(None)
}

fn build_auxiliary_local_launch_command(
    executable_path: &Path,
    perles_dir: Option<&str>,
    perles_db: Option<&str>,
) -> String {
    let escaped_path = escape_single_quotes(&executable_path.to_string_lossy());
    let mut segments = render_auxiliary_env_exports(perles_dir, perles_db);
    segments.push(format!("'{escaped_path}'"));
    segments.push(String::from("exit_code=$?"));
    segments.push(String::from(
        "if [ \"$exit_code\" -ne 0 ]; then printf '%s\\n' \"ez-mux auxiliary viewer perles exited with status $exit_code\" >&2; fi",
    ));
    segments.push(String::from("exec \"${SHELL:-/bin/sh}\" -l"));
    segments.join("; ")
}

fn build_auxiliary_remote_launch_command(
    remote_dir: &str,
    remote_server_url: &str,
    use_tssh: bool,
    use_mosh: bool,
) -> Result<String, SessionError> {
    let authority = parse_remote_ssh_authority(remote_server_url)?;

    let remote_script = render_auxiliary_remote_script(remote_dir);
    let remote_invocation = build_remote_invocation(&authority, &remote_script, use_tssh, use_mosh);
    let transport = remote_transport_label(use_tssh, use_mosh);
    Ok(format!(
        "if {remote_invocation}; then :; else remote_exit_code=$?; printf '%s\\n' \"ez-mux remote {transport} launch failed with status $remote_exit_code\" >&2; fi; exec \"${{SHELL:-/bin/sh}}\" -l"
    ))
}

fn render_auxiliary_remote_script(remote_dir: &str) -> String {
    let mut segments = Vec::new();
    segments.push(format!("cd '{}'", escape_single_quotes(remote_dir)));
    segments.push(String::from(
        "\"${SHELL:-/bin/sh}\" -lic 'if command -v perles >/dev/null 2>&1; then perles; exit_code=$?; if [ \"$exit_code\" -ne 0 ]; then printf \"%s\\n\" \"ez-mux auxiliary viewer perles exited with status $exit_code\" >&2; fi; else printf \"%s\\n\" \"ez-mux auxiliary viewer command not found: perles\" >&2; fi'",
    ));
    segments.push(String::from("exec \"${SHELL:-/bin/sh}\" -l"));
    segments.join("; ")
}

fn render_auxiliary_env_exports(perles_dir: Option<&str>, perles_db: Option<&str>) -> Vec<String> {
    let mut rendered = Vec::new();
    for (key, value) in [(PERLES_DIR_ENV, perles_dir), (PERLES_DB_ENV, perles_db)] {
        let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
            continue;
        };
        rendered.push(format!("export {key}='{}'", escape_single_quotes(value)));
    }
    rendered
}

fn resolve_auxiliary_remote_launch(
    cwd: &str,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    use_tssh: bool,
    use_mosh: bool,
) -> Result<Option<AuxiliaryRemoteLaunch>, SessionError> {
    let server_url = remote_server_url
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(server_url) = server_url else {
        return Ok(None);
    };

    let resolved = resolve_remote_path(Path::new(cwd), remote_path)?;
    if !resolved.remapped {
        return Ok(None);
    }

    Ok(Some(AuxiliaryRemoteLaunch {
        remote_dir: resolved.effective_path.display().to_string(),
        remote_server_url: server_url.to_owned(),
        use_tssh,
        use_mosh,
    }))
}

fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

fn discover_executable_in_path(command_name: &str) -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH")?;
    let candidate_names = executable_candidate_names(command_name);
    for path_dir in std::env::split_paths(&path_env) {
        for candidate_name in &candidate_names {
            let candidate_path = path_dir.join(candidate_name);
            if is_executable_file(&candidate_path) {
                return Some(candidate_path);
            }
        }
    }

    None
}

fn executable_candidate_names(command_name: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let mut names = vec![command_name.to_owned()];
        for extension in ["exe", "cmd", "bat"] {
            names.push(format!("{command_name}.{extension}"));
        }
        names
    }

    #[cfg(not(windows))]
    {
        vec![command_name.to_owned()]
    }
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        if let Ok(metadata) = std::fs::metadata(path) {
            return metadata.permissions().mode() & 0o111 != 0;
        }
        false
    }

    #[cfg(not(unix))]
    {
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuxiliaryRemoteLaunch {
    remote_dir: String,
    remote_server_url: String,
    use_tssh: bool,
    use_mosh: bool,
}

#[cfg(test)]
mod tests;
