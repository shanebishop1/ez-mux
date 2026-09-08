use crate::config::{self, RuntimeContext, SessionRuntimeContext};

use super::SessionError;

/// Resolves a context for an internal command. Session metadata wins over the
/// caller's process environment/configuration, and helper sessions delegate to
/// their recorded parent. Markerless sessions migrate only positively owned
/// values from their own legacy session environment.
///
/// # Errors
/// Returns an error when tmux cannot inspect or initialize the session context.
pub fn resolve_session_runtime_context(
    session_name: &str,
    current: &RuntimeContext,
) -> Result<RuntimeContext, SessionError> {
    let scoped = super::tmux::resolve_owned_session_runtime_context(session_name)?;

    Ok(runtime_context_with_session_scope(current, scoped))
}

fn runtime_context_with_session_scope(
    current: &RuntimeContext,
    scoped: SessionRuntimeContext,
) -> RuntimeContext {
    let remote_server_matches =
        current.remote.shared_server.url.value.as_deref() == scoped.shared_server_url.as_deref();
    RuntimeContext {
        remote: config::RemoteRuntimeResolution {
            remote_path: session_value(scoped.remote_path),
            remote_server_url: session_value(scoped.remote_server_url),
            use_tssh: session_value(scoped.use_tssh),
            use_mosh: session_value(scoped.use_mosh),
            shared_server: config::SharedServerRuntimeResolution {
                url: session_value(scoped.shared_server_url),
                // A secret is never loaded from tmux. Reuse the caller's
                // secret only when it targets the same persisted server.
                password: if remote_server_matches {
                    current.remote.shared_server.password.clone()
                } else {
                    config::ResolvedValue {
                        value: None,
                        source: config::ValueSource::Default,
                    }
                },
            },
        },
        auxiliary: config::AuxiliaryRuntimeResolution {
            perles_dir: session_value(scoped.perles_dir),
            perles_db: session_value(scoped.perles_db),
        },
        agent_command: scoped.agent_command,
        opencode_theme: config::OpencodeThemeRuntimeResolution {
            enabled: scoped.opencode_themes_enabled,
            themes_by_slot: scoped.opencode_themes_by_slot,
        },
    }
}

fn session_value<T>(value: T) -> config::ResolvedValue<T> {
    config::ResolvedValue {
        value,
        source: config::ValueSource::Session,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{SessionRuntimeContext, runtime_context_with_session_scope};

    #[test]
    fn persisted_project_context_wins_without_reusing_another_server_password() {
        let mut current = crate::config::RuntimeContext::default();
        current.remote.remote_path.value = Some(String::from("/srv/project-b"));
        current.remote.remote_server_url.value = Some(String::from("b.example"));
        current.remote.shared_server.url.value = Some(String::from("http://b.example:4096"));
        current.remote.shared_server.password.value = Some(String::from("not-reused"));

        let scoped = SessionRuntimeContext {
            remote_path: Some(String::from("/srv/project-a")),
            remote_server_url: Some(String::from("a.example")),
            use_tssh: false,
            use_mosh: true,
            perles_dir: Some(String::from(".perles-a")),
            perles_db: Some(String::from("/tmp/perles-a.db")),
            shared_server_url: Some(String::from("http://a.example:4096")),
            agent_command: Some(String::from("exec agent-a")),
            opencode_themes_enabled: true,
            opencode_themes_by_slot: HashMap::new(),
        };

        let resolved = runtime_context_with_session_scope(&current, scoped);

        assert_eq!(
            resolved.remote.remote_path.value.as_deref(),
            Some("/srv/project-a")
        );
        assert_eq!(
            resolved.remote.remote_server_url.value.as_deref(),
            Some("a.example")
        );
        assert!(resolved.remote.use_mosh.value);
        assert_eq!(resolved.agent_command.as_deref(), Some("exec agent-a"));
        assert!(resolved.remote.shared_server.password.value.is_none());
    }
}
