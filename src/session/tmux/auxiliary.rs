use super::SessionError;
use super::command::{tmux_output_value, tmux_run};
use super::options::show_session_option;
use super::slot_swap::validate_canonical_slot_registry;
use crate::session::{AuxiliaryViewerAction, AuxiliaryViewerOutcome};

const AUXILIARY_WINDOW_NAME: &str = "beads-viewer";

pub(super) fn auxiliary_viewer(
    session_name: &str,
    open: bool,
) -> Result<AuxiliaryViewerOutcome, SessionError> {
    validate_canonical_slot_registry(session_name)?;

    let existing = find_window_id_by_name(session_name, AUXILIARY_WINDOW_NAME)?;
    if open {
        if let Some(window_id) = existing {
            return Ok(AuxiliaryViewerOutcome {
                session_name: session_name.to_owned(),
                action: AuxiliaryViewerAction::Reused,
                window_name: String::from(AUXILIARY_WINDOW_NAME),
                window_id: Some(window_id),
            });
        }

        let cwd = resolve_auxiliary_cwd(session_name)?;
        let command = String::from(
            "sh -lc 'if command -v bv >/dev/null 2>&1; then exec bv; else printf \"bv not available; auxiliary viewer stub\\n\"; fi'",
        );
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
