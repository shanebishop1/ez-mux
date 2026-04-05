use super::super::SessionError;
use super::super::command::tmux_run;
use crate::config::EZM_REMOTE_SERVER_URL_ENV;
use crate::session::resolve_remote_path;

const EZM_REMOTE_DIR_ENV: &str = "EZM_REMOTE_DIR";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PopupRemoteContext {
    pub(super) remote_dir: String,
    pub(super) remote_server_url: Option<String>,
    pub(super) use_mosh: bool,
}

pub(super) fn resolve_popup_remote_context(
    cwd: &str,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    use_mosh: bool,
) -> Result<Option<PopupRemoteContext>, SessionError> {
    let resolved = resolve_remote_path(std::path::Path::new(cwd), remote_path)?;
    if !resolved.remapped {
        return Ok(None);
    }

    Ok(Some(PopupRemoteContext {
        remote_dir: resolved.effective_path.display().to_string(),
        remote_server_url: remote_server_url
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        use_mosh,
    }))
}

pub(super) fn apply_popup_remote_context_environment(
    popup_session: &str,
    remote_context: Option<&PopupRemoteContext>,
) -> Result<(), SessionError> {
    if let Some(context) = remote_context {
        set_session_environment(popup_session, EZM_REMOTE_DIR_ENV, &context.remote_dir)?;

        if let Some(server_url) = context.remote_server_url.as_deref() {
            set_session_environment(popup_session, EZM_REMOTE_SERVER_URL_ENV, server_url)?;
        } else {
            unset_session_environment(popup_session, EZM_REMOTE_SERVER_URL_ENV)?;
        }
    } else {
        unset_session_environment(popup_session, EZM_REMOTE_DIR_ENV)?;
        unset_session_environment(popup_session, EZM_REMOTE_SERVER_URL_ENV)?;
    }

    Ok(())
}

fn set_session_environment(session_name: &str, key: &str, value: &str) -> Result<(), SessionError> {
    tmux_run(&["set-environment", "-t", session_name, key, value])
}

fn unset_session_environment(session_name: &str, key: &str) -> Result<(), SessionError> {
    tmux_run(&["set-environment", "-t", session_name, "-u", key])
}
