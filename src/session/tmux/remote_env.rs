use super::SessionError;
use super::command::{format_output_diagnostics, tmux_output, tmux_run};
use super::options::{set_session_option, show_session_option};
use crate::config::{
    EZM_REMOTE_PATH_ENV, EZM_REMOTE_SERVER_URL_ENV, EZM_RUNTIME_AGENT_COMMAND_OPTION,
    EZM_RUNTIME_CONTEXT_VERSION_OPTION, EZM_RUNTIME_OPENCODE_SERVER_URL_OPTION,
    EZM_RUNTIME_OPENCODE_THEME_PREFIX, EZM_RUNTIME_OPENCODE_THEMES_ENABLED_OPTION,
    EZM_RUNTIME_PERLES_DB_OPTION, EZM_RUNTIME_PERLES_DIR_OPTION, EZM_RUNTIME_REMOTE_PATH_OPTION,
    EZM_RUNTIME_REMOTE_SERVER_URL_OPTION, EZM_RUNTIME_USE_MOSH_OPTION, EZM_RUNTIME_USE_TSSH_OPTION,
    EZM_USE_MOSH_ENV, EZM_USE_TSSH_ENV, LEGACY_BEADS_DB_ENV, LEGACY_BEADS_DIR_ENV,
    OPENCODE_SERVER_PASSWORD_ENV, OPENCODE_SERVER_URL_ENV, PERLES_DB_ENV, PERLES_DIR_ENV,
    SessionRuntimeContext,
};
use std::collections::HashMap;

const LEGACY_SESSION_ENVIRONMENT_KEYS: [&str; 10] = [
    EZM_REMOTE_PATH_ENV,
    EZM_REMOTE_SERVER_URL_ENV,
    EZM_USE_TSSH_ENV,
    EZM_USE_MOSH_ENV,
    OPENCODE_SERVER_URL_ENV,
    OPENCODE_SERVER_PASSWORD_ENV,
    PERLES_DIR_ENV,
    PERLES_DB_ENV,
    LEGACY_BEADS_DIR_ENV,
    LEGACY_BEADS_DB_ENV,
];

/// Persists only non-secret project context in session options.
///
/// This deliberately does not touch tmux's global environment. In particular,
/// an empty project context never unsets a value that may belong to another
/// project or to the user. Existing ez-mux global variables are left alone
/// because their ownership cannot be proven.
pub(super) fn reconcile_session_runtime_context(
    session_name: &str,
    context: &SessionRuntimeContext,
) -> Result<(), SessionError> {
    super::runtime_context_persistence::reconcile_session_runtime_context(session_name, context)
}

/// Sets the `OpenCode` password only in one project session's environment.
/// Empty values intentionally mask any inherited global value without changing
/// that global value, so an unconfigured project cannot authenticate with a
/// different project's password.
pub(super) fn reconcile_session_runtime_auth(
    session_name: &str,
    password: Option<&str>,
) -> Result<(), SessionError> {
    super::runtime_auth::reconcile_session_runtime_auth(session_name, password)
}

/// Loads session-scoped context for internal commands. Popup and mode helper
/// sessions delegate to their recorded parent when they have no own marker.
pub(super) fn load_session_runtime_context(
    session_name: &str,
) -> Result<Option<SessionRuntimeContext>, SessionError> {
    load_session_runtime_context_at_depth(session_name, 0)
}

/// Resolves the context owned by `session_name`, migrating only legacy values
/// read from that session's own environment. The caller's process/config state
/// is deliberately not an input to markerless migration.
pub(super) fn resolve_owned_session_runtime_context(
    session_name: &str,
) -> Result<SessionRuntimeContext, SessionError> {
    if let Some(context) = load_session_runtime_context(session_name)? {
        return Ok(context);
    }

    let owner = runtime_context_owner_session(session_name, 0)?;
    if owner != session_name {
        if let Some(context) = load_session_runtime_context(&owner)? {
            return Ok(context);
        }
    }

    let mut legacy_values = load_legacy_session_environment(&owner)?;
    canonicalize_legacy_opencode_url(&owner, &mut legacy_values)?;
    let context = required_legacy_session_context(&owner, &legacy_values)?;
    reconcile_session_runtime_context(&owner, &context)?;
    scrub_legacy_session_environment(&owner, &legacy_values)?;
    Ok(context)
}

fn scrub_legacy_session_environment(
    session_name: &str,
    values: &HashMap<&'static str, String>,
) -> Result<(), SessionError> {
    for key in LEGACY_SESSION_ENVIRONMENT_KEYS {
        if values.contains_key(key) {
            tmux_run(&["set-environment", "-t", session_name, "-u", key])?;
        }
    }
    Ok(())
}

fn canonicalize_legacy_opencode_url(
    session_name: &str,
    values: &mut HashMap<&'static str, String>,
) -> Result<(), SessionError> {
    let Some(value) = values.get(OPENCODE_SERVER_URL_ENV) else {
        return Ok(());
    };
    if crate::config::validate_server_url(value, "legacy session environment").is_ok() {
        return Ok(());
    }
    let Some(canonical) = crate::config::server_url_without_userinfo(value) else {
        return Err(SessionError::UnsafeLegacyOpenCodeUrlMigration {
            session_name: session_name.to_owned(),
        });
    };

    let args = [
        "set-environment",
        "-t",
        session_name,
        OPENCODE_SERVER_URL_ENV,
        canonical.as_str(),
    ];
    tmux_run(&args)?;
    values.insert(OPENCODE_SERVER_URL_ENV, canonical);
    Ok(())
}

fn runtime_context_owner_session(session_name: &str, depth: u8) -> Result<String, SessionError> {
    if depth > 4 {
        return Err(SessionError::UnsafeLegacyRuntimeContextMigration {
            session_name: session_name.to_owned(),
        });
    }
    if show_session_option(session_name, EZM_RUNTIME_CONTEXT_VERSION_OPTION)?.is_some() {
        return Ok(session_name.to_owned());
    }

    let Some(parent) = show_session_option(session_name, "@ezm_popup_origin_session")? else {
        return Ok(session_name.to_owned());
    };
    let parent = parent.trim();
    if parent.is_empty() || parent == session_name {
        return Ok(session_name.to_owned());
    }
    runtime_context_owner_session(parent, depth + 1)
}

fn load_legacy_session_environment(
    session_name: &str,
) -> Result<HashMap<&'static str, String>, SessionError> {
    let mut values = HashMap::new();
    for key in LEGACY_SESSION_ENVIRONMENT_KEYS {
        if let Some(value) = show_session_environment(session_name, key)? {
            values.insert(key, value);
        }
    }
    Ok(values)
}

fn show_session_environment(
    session_name: &str,
    key: &'static str,
) -> Result<Option<String>, SessionError> {
    let output = tmux_output(&["show-environment", "-t", session_name, key])?;
    if output.status.success() {
        let prefix = format!("{key}=");
        return Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned));
    }
    if output.status.code() == Some(1) {
        return Ok(None);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("show-environment -t {session_name} {key}"),
        stderr: format_output_diagnostics(&output),
    })
}

fn legacy_session_context_from_values(
    session_name: &str,
    values: &HashMap<&'static str, String>,
) -> Result<Option<SessionRuntimeContext>, SessionError> {
    if !values
        .keys()
        .any(|key| *key != OPENCODE_SERVER_PASSWORD_ENV)
    {
        return Ok(None);
    }

    let shared_server_url = values.get(OPENCODE_SERVER_URL_ENV).cloned();
    if shared_server_url.as_deref().is_some_and(|url| {
        crate::config::validate_server_url(url, "legacy session environment").is_err()
    }) {
        return Err(SessionError::UnsafeLegacyOpenCodeUrlMigration {
            session_name: session_name.to_owned(),
        });
    }
    let remote_server_url = values.get(EZM_REMOTE_SERVER_URL_ENV).cloned();
    if let Some(authority) = remote_server_url.as_deref() {
        super::remote_authority::parse_remote_ssh_authority(authority)?;
    }

    let default_themes =
        crate::config::resolve_opencode_theme_runtime(&crate::config::FileConfig::default());
    Ok(Some(SessionRuntimeContext {
        remote_path: values.get(EZM_REMOTE_PATH_ENV).cloned(),
        remote_server_url,
        use_tssh: legacy_bool_value(values.get(EZM_USE_TSSH_ENV).map(String::as_str)),
        use_mosh: legacy_bool_value(values.get(EZM_USE_MOSH_ENV).map(String::as_str)),
        perles_dir: values
            .get(PERLES_DIR_ENV)
            .or_else(|| values.get(LEGACY_BEADS_DIR_ENV))
            .cloned(),
        perles_db: values
            .get(PERLES_DB_ENV)
            .or_else(|| values.get(LEGACY_BEADS_DB_ENV))
            .cloned(),
        shared_server_url,
        agent_command: None,
        opencode_themes_enabled: default_themes.enabled,
        opencode_themes_by_slot: default_themes.themes_by_slot,
    }))
}

fn required_legacy_session_context(
    session_name: &str,
    values: &HashMap<&'static str, String>,
) -> Result<SessionRuntimeContext, SessionError> {
    legacy_session_context_from_values(session_name, values)?.ok_or_else(|| {
        SessionError::UnsafeLegacyRuntimeContextMigration {
            session_name: session_name.to_owned(),
        }
    })
}

fn legacy_bool_value(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn load_session_runtime_context_at_depth(
    session_name: &str,
    depth: u8,
) -> Result<Option<SessionRuntimeContext>, SessionError> {
    if depth > 4 {
        return Ok(None);
    }

    if show_session_option(session_name, EZM_RUNTIME_CONTEXT_VERSION_OPTION)?.is_none() {
        let Some(parent) = show_session_option(session_name, "@ezm_popup_origin_session")? else {
            return Ok(None);
        };
        let parent = parent.trim();
        if parent.is_empty() || parent == session_name {
            return Ok(None);
        }
        return load_session_runtime_context_at_depth(parent, depth + 1);
    }

    let mut themes_by_slot = HashMap::new();
    for slot_id in 1_u8..=5 {
        let key = format!("{EZM_RUNTIME_OPENCODE_THEME_PREFIX}{slot_id}");
        if let Some(theme) = show_session_option(session_name, &key)? {
            if !theme.trim().is_empty() {
                themes_by_slot.insert(slot_id, theme.trim().to_owned());
            }
        }
    }

    Ok(Some(SessionRuntimeContext {
        remote_path: optional_session_value(session_name, EZM_RUNTIME_REMOTE_PATH_OPTION)?,
        remote_server_url: optional_session_value(
            session_name,
            EZM_RUNTIME_REMOTE_SERVER_URL_OPTION,
        )?,
        use_tssh: session_bool_value(session_name, EZM_RUNTIME_USE_TSSH_OPTION)?,
        use_mosh: session_bool_value(session_name, EZM_RUNTIME_USE_MOSH_OPTION)?,
        perles_dir: optional_session_value(session_name, EZM_RUNTIME_PERLES_DIR_OPTION)?,
        perles_db: optional_session_value(session_name, EZM_RUNTIME_PERLES_DB_OPTION)?,
        shared_server_url: load_persisted_shared_server_url(session_name)?,
        agent_command: optional_session_value(session_name, EZM_RUNTIME_AGENT_COMMAND_OPTION)?,
        opencode_themes_enabled: session_bool_value(
            session_name,
            EZM_RUNTIME_OPENCODE_THEMES_ENABLED_OPTION,
        )?,
        opencode_themes_by_slot: themes_by_slot,
    }))
}

fn load_persisted_shared_server_url(session_name: &str) -> Result<Option<String>, SessionError> {
    let value = optional_session_value(session_name, EZM_RUNTIME_OPENCODE_SERVER_URL_OPTION)?;
    let Some(value) = value else {
        return Ok(None);
    };
    if crate::config::validate_server_url(&value, "persisted session context").is_ok() {
        return Ok(Some(value));
    }
    if let Some(canonical) = crate::config::server_url_without_userinfo(&value) {
        set_session_option(
            session_name,
            EZM_RUNTIME_OPENCODE_SERVER_URL_OPTION,
            &canonical,
        )?;
        return Ok(Some(canonical));
    }

    Err(SessionError::UnsafeSessionOpenCodeUrl {
        session_name: session_name.to_owned(),
    })
}

fn optional_session_value(session_name: &str, key: &str) -> Result<Option<String>, SessionError> {
    Ok(show_session_option(session_name, key)?.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    }))
}

fn session_bool_value(session_name: &str, key: &str) -> Result<bool, SessionError> {
    Ok(
        show_session_option(session_name, key)?.is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        EZM_REMOTE_PATH_ENV, EZM_REMOTE_SERVER_URL_ENV, OPENCODE_SERVER_URL_ENV,
        legacy_session_context_from_values, required_legacy_session_context,
    };
    use crate::config::EZM_USE_MOSH_ENV;
    use std::collections::HashMap;

    #[test]
    fn markerless_migration_recovers_only_session_owned_legacy_values() {
        let values = HashMap::from([
            (EZM_REMOTE_PATH_ENV, String::from("/srv/project-a")),
            (EZM_REMOTE_SERVER_URL_ENV, String::from("a.example")),
            (EZM_USE_MOSH_ENV, String::from("1")),
            (
                OPENCODE_SERVER_URL_ENV,
                String::from("https://a.example:4096"),
            ),
        ]);

        let recovered = legacy_session_context_from_values("ezm-project-a", &values)
            .expect("owned legacy values should be valid")
            .expect("owned legacy values should be recoverable");

        assert_eq!(recovered.remote_path.as_deref(), Some("/srv/project-a"));
        assert_eq!(recovered.remote_server_url.as_deref(), Some("a.example"));
        assert!(recovered.use_mosh);
        assert_eq!(
            recovered.shared_server_url.as_deref(),
            Some("https://a.example:4096")
        );
        assert!(!format!("{recovered:?}").contains("project-b"));
    }

    #[test]
    fn markerless_migration_requires_reconciliation_without_owned_values() {
        let error = required_legacy_session_context(
            "ezm-project-a",
            &HashMap::<&'static str, String>::new(),
        )
        .expect_err("markerless session without owned values must not import caller state");
        let rendered = error.to_string();

        assert!(rendered.contains("refusing to import the current process/config context"));
        assert!(rendered.contains("ezm kill"));
    }

    #[test]
    fn markerless_migration_rejects_credential_bearing_opencode_url_without_disclosure() {
        let sentinel = "legacy;credential,(sentinel)";
        let values = HashMap::from([(
            OPENCODE_SERVER_URL_ENV,
            format!("https://operator:{sentinel}@a.example:4096"),
        )]);

        let error = legacy_session_context_from_values("ezm-project-a", &values)
            .expect_err("credential-bearing legacy URL must not be persisted");
        let rendered = error.to_string();

        assert!(!rendered.contains(sentinel));
        assert!(rendered.contains("cannot be migrated safely"));
    }
}
