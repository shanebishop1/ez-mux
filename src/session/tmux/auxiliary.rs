use super::SessionError;
use super::command::{tmux_output_value, tmux_run};
use super::options::show_session_option;
use super::slot_swap::validate_canonical_slot_registry;
use crate::session::{AuxiliaryViewerAction, AuxiliaryViewerOutcome};
use std::path::{Path, PathBuf};

const AUXILIARY_WINDOW_NAME: &str = "beads-viewer";

pub(super) fn auxiliary_viewer(
    session_name: &str,
    open: bool,
) -> Result<AuxiliaryViewerOutcome, SessionError> {
    let existing = find_window_id_by_name(session_name, AUXILIARY_WINDOW_NAME)?;
    if open {
        if existing.is_none() && discover_executable_in_path("bv").is_none() {
            return Ok(AuxiliaryViewerOutcome {
                session_name: session_name.to_owned(),
                action: AuxiliaryViewerAction::SkippedUnavailable,
                window_name: String::from(AUXILIARY_WINDOW_NAME),
                window_id: None,
            });
        }

        validate_canonical_slot_registry(session_name)?;

        if let Some(window_id) = existing {
            return Ok(AuxiliaryViewerOutcome {
                session_name: session_name.to_owned(),
                action: AuxiliaryViewerAction::Reused,
                window_name: String::from(AUXILIARY_WINDOW_NAME),
                window_id: Some(window_id),
            });
        }

        let bv_executable =
            discover_executable_in_path("bv").ok_or_else(|| SessionError::TmuxCommandFailed {
                command: String::from("auxiliary-viewer discover bv"),
                stderr: String::from("bv executable disappeared during startup reconciliation"),
            })?;

        let cwd = resolve_auxiliary_cwd(session_name)?;
        let command = build_auxiliary_launch_command(&bv_executable);
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

        if let Err(error) =
            tmux_run(&["set-option", "-w", "-t", &window_id, "remain-on-exit", "on"])
        {
            let no_such_window_race = matches!(
                &error,
                SessionError::TmuxCommandFailed { stderr, .. }
                    if stderr.contains("no such window")
            );
            if !no_such_window_race {
                return Err(error);
            }
        }

        return Ok(AuxiliaryViewerOutcome {
            session_name: session_name.to_owned(),
            action: AuxiliaryViewerAction::Created,
            window_name: String::from(AUXILIARY_WINDOW_NAME),
            window_id: Some(window_id),
        });
    }

    validate_canonical_slot_registry(session_name)?;

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

fn build_auxiliary_launch_command(executable_path: &Path) -> String {
    let escaped_path = shell_escape_double_quoted(&executable_path.to_string_lossy());
    format!("sh -lc \"exec \\\"{escaped_path}\\\"\"")
}

fn shell_escape_double_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
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

#[cfg(test)]
mod tests {
    use super::{
        AUXILIARY_WINDOW_NAME, build_auxiliary_launch_command, discover_executable_in_path,
    };

    #[test]
    fn auxiliary_launch_command_uses_resolved_bv_path() {
        let command = build_auxiliary_launch_command(std::path::Path::new("/tmp/tools/bv"));
        assert_eq!(command, "sh -lc \"exec \\\"/tmp/tools/bv\\\"\"");
    }

    #[test]
    fn auxiliary_launch_command_escapes_double_quote_sensitive_characters() {
        let command = build_auxiliary_launch_command(std::path::Path::new(
            "/tmp/tools/space and \"quote\"/$HOME/`cmd`/bv",
        ));
        assert_eq!(
            command,
            "sh -lc \"exec \\\"/tmp/tools/space and \\\"quote\\\"/\\$HOME/\\`cmd\\`/bv\\\"\""
        );
    }

    #[test]
    fn discover_executable_returns_none_for_missing_binary_name() {
        let unlikely = format!("ezm-no-such-tool-{AUXILIARY_WINDOW_NAME}");
        assert!(discover_executable_in_path(&unlikely).is_none());
    }
}
