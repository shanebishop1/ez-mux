use super::SessionError;
use super::command::tmux_run_batch;
use super::options::show_session_option;
use crate::config::{
    EZM_RUNTIME_AGENT_COMMAND_OPTION, EZM_RUNTIME_CONTEXT_VERSION,
    EZM_RUNTIME_CONTEXT_VERSION_OPTION, EZM_RUNTIME_OPENCODE_SERVER_URL_OPTION,
    EZM_RUNTIME_OPENCODE_THEME_PREFIX, EZM_RUNTIME_OPENCODE_THEMES_ENABLED_OPTION,
    EZM_RUNTIME_PERLES_DB_OPTION, EZM_RUNTIME_PERLES_DIR_OPTION, EZM_RUNTIME_REMOTE_PATH_OPTION,
    EZM_RUNTIME_REMOTE_SERVER_URL_OPTION, EZM_RUNTIME_USE_MOSH_OPTION, EZM_RUNTIME_USE_TSSH_OPTION,
    SessionRuntimeContext,
};

/// Persists the non-secret context in session options and commits a marker
/// last. A failed batch therefore remains retryable instead of appearing
/// reconciled on the next launch.
pub(super) fn reconcile_session_runtime_context(
    session_name: &str,
    context: &SessionRuntimeContext,
) -> Result<(), SessionError> {
    if show_session_option(session_name, EZM_RUNTIME_CONTEXT_VERSION_OPTION)?.is_some() {
        return Ok(());
    }
    validate_session_runtime_context(session_name, context)?;
    tmux_run_batch(&session_runtime_context_commands(session_name, context))
}

fn validate_session_runtime_context(
    session_name: &str,
    context: &SessionRuntimeContext,
) -> Result<(), SessionError> {
    if let Some(authority) = context.remote_server_url.as_deref() {
        super::remote_authority::parse_remote_ssh_authority(authority)?;
    }
    if context.shared_server_url.as_deref().is_some_and(|url| {
        crate::config::validate_server_url(url, "session runtime context").is_err()
    }) {
        return Err(SessionError::UnsafeSessionOpenCodeUrl {
            session_name: session_name.to_owned(),
        });
    }
    Ok(())
}

fn session_runtime_context_commands(
    session_name: &str,
    context: &SessionRuntimeContext,
) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    push_optional_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_REMOTE_PATH_OPTION,
        context.remote_path.as_deref(),
    );
    push_optional_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_REMOTE_SERVER_URL_OPTION,
        context.remote_server_url.as_deref(),
    );
    push_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_USE_TSSH_OPTION,
        bool_value(context.use_tssh),
    );
    push_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_USE_MOSH_OPTION,
        bool_value(context.use_mosh),
    );
    push_optional_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_PERLES_DIR_OPTION,
        context.perles_dir.as_deref(),
    );
    push_optional_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_PERLES_DB_OPTION,
        context.perles_db.as_deref(),
    );
    push_optional_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_OPENCODE_SERVER_URL_OPTION,
        context.shared_server_url.as_deref(),
    );
    push_optional_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_AGENT_COMMAND_OPTION,
        context.agent_command.as_deref(),
    );
    push_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_OPENCODE_THEMES_ENABLED_OPTION,
        bool_value(context.opencode_themes_enabled),
    );
    for slot_id in 1_u8..=5 {
        let key = format!("{EZM_RUNTIME_OPENCODE_THEME_PREFIX}{slot_id}");
        push_optional_session_value(
            &mut commands,
            session_name,
            &key,
            context
                .opencode_themes_by_slot
                .get(&slot_id)
                .map(String::as_str),
        );
    }

    push_session_value(
        &mut commands,
        session_name,
        EZM_RUNTIME_CONTEXT_VERSION_OPTION,
        EZM_RUNTIME_CONTEXT_VERSION,
    );
    commands
}

fn push_optional_session_value(
    commands: &mut Vec<Vec<String>>,
    session_name: &str,
    key: &str,
    value: Option<&str>,
) {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => push_session_value(commands, session_name, key, value),
        None => commands.push(vec![
            String::from("set-option"),
            String::from("-u"),
            String::from("-t"),
            session_name.to_owned(),
            key.to_owned(),
        ]),
    }
}

fn push_session_value(commands: &mut Vec<Vec<String>>, session_name: &str, key: &str, value: &str) {
    commands.push(vec![
        String::from("set-option"),
        String::from("-t"),
        session_name.to_owned(),
        key.to_owned(),
        value.to_owned(),
    ]);
}

fn bool_value(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

#[cfg(test)]
mod tests {
    use super::{bool_value, session_runtime_context_commands};
    use crate::config::{EZM_RUNTIME_CONTEXT_VERSION_OPTION, SessionRuntimeContext};
    use std::collections::HashMap;

    #[test]
    fn runtime_boolean_values_are_stable() {
        assert_eq!(bool_value(true), "1");
        assert_eq!(bool_value(false), "0");
    }

    #[test]
    fn persistence_is_session_scoped_and_never_contains_passwords() {
        let context = SessionRuntimeContext {
            remote_path: Some(String::from("/srv/remotes")),
            remote_server_url: Some(String::from("shell.example")),
            use_tssh: true,
            use_mosh: false,
            perles_dir: Some(String::from(".perles")),
            perles_db: Some(String::from("/tmp/perles.db")),
            shared_server_url: Some(String::from("http://opencode.example:4096")),
            agent_command: Some(String::from("exec claude")),
            opencode_themes_enabled: true,
            opencode_themes_by_slot: HashMap::from([(1, String::from("nightowl"))]),
        };
        let commands = session_runtime_context_commands("ezm-project", &context);
        let rendered = format!("{commands:?}");

        assert!(
            commands
                .iter()
                .all(|command| command.first().map(String::as_str) == Some("set-option"))
        );
        assert!(commands.iter().all(|command| !command.iter().any(|value| {
            value == "OPENCODE_SERVER_PASSWORD" || value.contains("super-secret")
        })));
        assert!(!rendered.contains("set-environment"));
        assert!(!rendered.contains("super-secret"));
        assert_eq!(
            commands
                .last()
                .and_then(|command| command.get(3))
                .map(String::as_str),
            Some(EZM_RUNTIME_CONTEXT_VERSION_OPTION)
        );
    }
}
