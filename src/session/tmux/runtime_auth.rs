use super::SessionError;
use super::command::{format_output_diagnostics, tmux_output, tmux_run, tmux_stdout};
use crate::config::OPENCODE_SERVER_PASSWORD_ENV;

/// Reconciles the credential at the session environment boundary.
///
/// The generated command always targets one session. Empty credentials are
/// intentionally written as an empty value so tmux does not fall back to a
/// password inherited from global state.
pub(super) fn reconcile_session_runtime_auth(
    session_name: &str,
    password: Option<&str>,
) -> Result<(), SessionError> {
    let password = password
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let args = session_runtime_auth_args(session_name, password);
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    tmux_run(&args_ref)
}

/// Reads the credential explicitly owned by one tmux session.
///
/// `show-environment -t` does not fall back to tmux global state, which is
/// important here: an unset owner must remain distinguishable from a global
/// credential so callers can install an explicit empty mask elsewhere.
pub(super) fn read_session_runtime_auth(
    session_name: &str,
) -> Result<Option<String>, SessionError> {
    let output = tmux_output(&[
        "show-environment",
        "-t",
        session_name,
        OPENCODE_SERVER_PASSWORD_ENV,
    ])?;
    if output.status.success() {
        let prefix = format!("{OPENCODE_SERVER_PASSWORD_ENV}=");
        let stdout = tmux_stdout(&output);
        return Ok(stdout
            .strip_prefix(&prefix)
            .map(|value| value.strip_suffix('\n').unwrap_or(value).to_owned()));
    }

    if output.status.code() == Some(1)
        && String::from_utf8_lossy(&output.stderr)
            .to_ascii_lowercase()
            .contains("unknown variable")
    {
        return Ok(None);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("show-environment -t {session_name} {OPENCODE_SERVER_PASSWORD_ENV}"),
        stderr: format_output_diagnostics(&output),
    })
}

fn session_runtime_auth_args(session_name: &str, password: &str) -> Vec<String> {
    vec![
        String::from("set-environment"),
        String::from("-t"),
        session_name.to_owned(),
        String::from(OPENCODE_SERVER_PASSWORD_ENV),
        password.to_owned(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{OPENCODE_SERVER_PASSWORD_ENV, session_runtime_auth_args};

    #[test]
    fn runtime_auth_is_targeted_to_one_session_and_empty_without_password() {
        let first = session_runtime_auth_args("ezm-project-a", "password-a");
        let second = session_runtime_auth_args("ezm-project-b", "");

        assert_eq!(first[0], "set-environment");
        assert_eq!(first[1], "-t");
        assert_eq!(first[2], "ezm-project-a");
        assert_eq!(first[3], OPENCODE_SERVER_PASSWORD_ENV);
        assert_eq!(first[4], "password-a");
        assert!(!first.contains(&String::from("-g")));
        assert_eq!(second[2], "ezm-project-b");
        assert_eq!(second[4], "");
        assert!(!second.contains(&String::from("password-a")));
    }

    #[test]
    fn same_url_projects_keep_password_updates_session_scoped() {
        let first = session_runtime_auth_args("ezm-project-a", "credential-a");
        let second = session_runtime_auth_args("ezm-project-b", "credential-b");

        assert_eq!(first[2], "ezm-project-a");
        assert_eq!(second[2], "ezm-project-b");
        assert_eq!(first[4], "credential-a");
        assert_eq!(second[4], "credential-b");
        assert!(!first.contains(&String::from("-g")));
        assert!(!second.contains(&String::from("-g")));
    }
}
