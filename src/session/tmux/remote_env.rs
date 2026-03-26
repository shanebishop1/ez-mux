use super::command::tmux_run_batch;
use super::SessionError;
use crate::config::{
    EnvProvider, ProcessEnv, EZM_REMOTE_PATH_ENV, EZM_REMOTE_SERVER_URL_ENV,
    OPENCODE_SERVER_PASSWORD_ENV, OPENCODE_SERVER_URL_ENV,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RuntimeEnvVar {
    pub(super) key: &'static str,
    pub(super) value: Option<String>,
}

pub(super) fn resolve_runtime_env_vars(env: &impl EnvProvider) -> Vec<RuntimeEnvVar> {
    runtime_env_keys()
        .iter()
        .map(|&key| RuntimeEnvVar {
            key,
            value: env
                .get_var(key)
                .map(|candidate| candidate.trim().to_owned())
                .filter(|candidate| !candidate.is_empty()),
        })
        .collect()
}

pub(super) fn sync_runtime_env_into_tmux_server() -> Result<(), SessionError> {
    let env = ProcessEnv;
    sync_runtime_env_into_tmux_server_with(&env)
}

pub(super) fn sync_runtime_env_into_tmux_server_with(
    env: &impl EnvProvider,
) -> Result<(), SessionError> {
    tmux_run_batch(&runtime_env_sync_commands(env))
}

fn runtime_env_sync_commands(env: &impl EnvProvider) -> Vec<Vec<String>> {
    resolve_runtime_env_vars(env)
        .into_iter()
        .map(|variable| match variable.value {
            Some(value) => vec![
                String::from("set-environment"),
                String::from("-g"),
                String::from(variable.key),
                value,
            ],
            None => vec![
                String::from("set-environment"),
                String::from("-gu"),
                String::from(variable.key),
            ],
        })
        .collect()
}

fn runtime_env_keys() -> [&'static str; 4] {
    [
        EZM_REMOTE_PATH_ENV,
        EZM_REMOTE_SERVER_URL_ENV,
        OPENCODE_SERVER_URL_ENV,
        OPENCODE_SERVER_PASSWORD_ENV,
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{resolve_runtime_env_vars, runtime_env_sync_commands};
    use crate::config::{
        EZM_REMOTE_PATH_ENV, EZM_REMOTE_SERVER_URL_ENV, OPENCODE_SERVER_PASSWORD_ENV,
        OPENCODE_SERVER_URL_ENV,
    };

    #[test]
    fn resolve_runtime_env_vars_reads_remote_and_shared_server_contract_keys() {
        let mut env = HashMap::new();
        env.insert(
            String::from(EZM_REMOTE_PATH_ENV),
            String::from("/srv/remotes"),
        );
        env.insert(
            String::from(EZM_REMOTE_SERVER_URL_ENV),
            String::from("devbox-ez-1"),
        );
        env.insert(
            String::from(OPENCODE_SERVER_URL_ENV),
            String::from("http://devbox-ez-1:4096"),
        );
        env.insert(
            String::from(OPENCODE_SERVER_PASSWORD_ENV),
            String::from("weinthisyuh78"),
        );

        let resolved = resolve_runtime_env_vars(&env);

        assert_eq!(resolved.len(), 4);
        assert_eq!(resolved[0].key, EZM_REMOTE_PATH_ENV);
        assert_eq!(resolved[0].value.as_deref(), Some("/srv/remotes"));
        assert_eq!(resolved[1].key, EZM_REMOTE_SERVER_URL_ENV);
        assert_eq!(resolved[1].value.as_deref(), Some("devbox-ez-1"));
        assert_eq!(resolved[2].key, OPENCODE_SERVER_URL_ENV);
        assert_eq!(
            resolved[2].value.as_deref(),
            Some("http://devbox-ez-1:4096")
        );
        assert_eq!(resolved[3].key, OPENCODE_SERVER_PASSWORD_ENV);
        assert_eq!(resolved[3].value.as_deref(), Some("weinthisyuh78"));
    }

    #[test]
    fn resolve_runtime_env_vars_trims_values_and_treats_blank_as_unset() {
        let mut env = HashMap::new();
        env.insert(
            String::from(EZM_REMOTE_PATH_ENV),
            String::from("  /srv/remotes  "),
        );
        env.insert(String::from(EZM_REMOTE_SERVER_URL_ENV), String::from("   "));

        let resolved = resolve_runtime_env_vars(&env);

        assert_eq!(resolved[0].value.as_deref(), Some("/srv/remotes"));
        assert_eq!(resolved[1].value, None);
        assert_eq!(resolved[2].value, None);
        assert_eq!(resolved[3].value, None);
    }

    #[test]
    fn runtime_env_sync_commands_emit_set_and_unset_in_contract_key_order() {
        let mut env = HashMap::new();
        env.insert(
            String::from(EZM_REMOTE_PATH_ENV),
            String::from("/srv/remotes"),
        );
        env.insert(
            String::from(EZM_REMOTE_SERVER_URL_ENV),
            String::from("https://shell.remote.example:7443"),
        );
        env.insert(
            String::from(OPENCODE_SERVER_URL_ENV),
            String::from("http://devbox-ez-1:4096"),
        );
        env.insert(
            String::from(OPENCODE_SERVER_PASSWORD_ENV),
            String::from("super-secret"),
        );

        let commands = runtime_env_sync_commands(&env);

        assert_eq!(
            commands,
            vec![
                vec![
                    String::from("set-environment"),
                    String::from("-g"),
                    String::from(EZM_REMOTE_PATH_ENV),
                    String::from("/srv/remotes"),
                ],
                vec![
                    String::from("set-environment"),
                    String::from("-g"),
                    String::from(EZM_REMOTE_SERVER_URL_ENV),
                    String::from("https://shell.remote.example:7443"),
                ],
                vec![
                    String::from("set-environment"),
                    String::from("-g"),
                    String::from(OPENCODE_SERVER_URL_ENV),
                    String::from("http://devbox-ez-1:4096"),
                ],
                vec![
                    String::from("set-environment"),
                    String::from("-g"),
                    String::from(OPENCODE_SERVER_PASSWORD_ENV),
                    String::from("super-secret"),
                ],
            ]
        );
    }

    #[test]
    fn runtime_env_sync_commands_treat_blank_values_as_unset_commands() {
        let mut env = HashMap::new();
        env.insert(String::from(EZM_REMOTE_PATH_ENV), String::from("   "));
        env.insert(
            String::from(EZM_REMOTE_SERVER_URL_ENV),
            String::from("  https://shell.remote.example:7443  "),
        );

        let commands = runtime_env_sync_commands(&env);

        assert_eq!(
            commands,
            vec![
                vec![
                    String::from("set-environment"),
                    String::from("-gu"),
                    String::from(EZM_REMOTE_PATH_ENV),
                ],
                vec![
                    String::from("set-environment"),
                    String::from("-g"),
                    String::from(EZM_REMOTE_SERVER_URL_ENV),
                    String::from("https://shell.remote.example:7443"),
                ],
                vec![
                    String::from("set-environment"),
                    String::from("-gu"),
                    String::from(OPENCODE_SERVER_URL_ENV),
                ],
                vec![
                    String::from("set-environment"),
                    String::from("-gu"),
                    String::from(OPENCODE_SERVER_PASSWORD_ENV),
                ],
            ]
        );
    }
}
