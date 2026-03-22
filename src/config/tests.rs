use std::collections::HashMap;
use std::fs;

use tempfile::tempdir;

use super::*;

#[test]
fn linux_uses_xdg_config_home() {
    let mut env = HashMap::new();
    env.insert(String::from("XDG_CONFIG_HOME"), String::from("/tmp/xdg"));
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let path = resolve_config_path(&env, OperatingSystem::Linux).expect("path should resolve");
    assert_eq!(
        path,
        std::path::PathBuf::from("/tmp/xdg/ez-mux/config.toml")
    );
}

#[test]
fn linux_falls_back_to_home_config() {
    let mut env = HashMap::new();
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let path = resolve_config_path(&env, OperatingSystem::Linux).expect("path should resolve");
    assert_eq!(
        path,
        std::path::PathBuf::from("/tmp/home/.config/ez-mux/config.toml")
    );
}

#[test]
fn macos_uses_application_support() {
    let mut env = HashMap::new();
    env.insert(String::from("HOME"), String::from("/Users/tester"));

    let path = resolve_config_path(&env, OperatingSystem::MacOs).expect("path should resolve");
    assert_eq!(
        path,
        std::path::PathBuf::from("/Users/tester/Library/Application Support/ez-mux/config.toml")
    );
}

#[test]
fn ezm_config_overrides_default_path() {
    let mut env = HashMap::new();
    env.insert(
        String::from(EZM_CONFIG_ENV),
        String::from("/custom/path.toml"),
    );
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let path = resolve_config_path(&env, OperatingSystem::Linux).expect("path should resolve");
    assert_eq!(path, std::path::PathBuf::from("/custom/path.toml"));
}

#[test]
fn whitespace_only_env_values_are_treated_as_unset() {
    let mut env = HashMap::new();
    env.insert(String::from(EZM_CONFIG_ENV), String::from("   \t"));
    env.insert(String::from("XDG_CONFIG_HOME"), String::from("   "));
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let path = resolve_config_path(&env, OperatingSystem::Linux).expect("path should resolve");
    assert_eq!(
        path,
        std::path::PathBuf::from("/tmp/home/.config/ez-mux/config.toml")
    );
}

#[test]
fn unsupported_platform_returns_typed_error() {
    let env = HashMap::<String, String>::new();

    let error = resolve_config_path(&env, OperatingSystem::Unsupported).expect_err("must fail");
    assert!(matches!(error, ConfigError::UnsupportedPlatform { .. }));
}

#[test]
fn missing_config_file_is_non_fatal() {
    let dir = tempdir().expect("tempdir");
    let mut env = HashMap::new();
    env.insert(
        String::from(EZM_CONFIG_ENV),
        dir.path().join("missing.toml").display().to_string(),
    );

    let loaded = load_config(&env, OperatingSystem::Linux).expect("load should succeed");
    assert_eq!(loaded.values, FileConfig::default());
}

#[test]
fn invalid_toml_is_fatal() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    fs::write(&path, "operator = [").expect("write");

    let mut env = HashMap::new();
    env.insert(String::from(EZM_CONFIG_ENV), path.display().to_string());

    let error = load_config(&env, OperatingSystem::Linux).expect_err("load should fail");
    assert!(matches!(error, ConfigError::InvalidToml { .. }));
}

#[test]
fn precedence_is_cli_then_env_then_file_then_default() {
    let resolved = resolve_operator(
        Some(String::from("cli")),
        Some(String::from("env")),
        Some(String::from("file")),
    );
    assert_eq!(resolved.value, Some(String::from("cli")));
    assert_eq!(resolved.source, ValueSource::Cli);

    let resolved = resolve_operator(None, Some(String::from("env")), Some(String::from("file")));
    assert_eq!(resolved.value, Some(String::from("env")));
    assert_eq!(resolved.source, ValueSource::Env);

    let resolved = resolve_operator(None, None, Some(String::from("file")));
    assert_eq!(resolved.value, Some(String::from("file")));
    assert_eq!(resolved.source, ValueSource::File);

    let resolved = resolve_operator(None, None, None);
    assert_eq!(resolved.value, None);
    assert_eq!(resolved.source, ValueSource::Default);
}

#[test]
fn remote_runtime_prefers_env_over_file_values() {
    let mut env = HashMap::new();
    env.insert(
        String::from(EZM_REMOTE_DIR_PREFIX_ENV),
        String::from("/env/remotes"),
    );
    env.insert(
        String::from(OPENCODE_SERVER_URL_ENV),
        String::from("https://env.example:4242"),
    );
    env.insert(
        String::from(OPENCODE_SERVER_PASSWORD_ENV),
        String::from("env-secret"),
    );

    let file = FileConfig {
        operator: None,
        ezm_remote_dir_prefix: Some(String::from("/file/remotes")),
        ezm_remote_server_url: None,
        opencode_server_url: Some(String::from("https://file.example:4096")),
        opencode_server_password: Some(String::from("file-secret")),
    };

    let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_dir_prefix,
        ResolvedValue {
            value: Some(String::from("/env/remotes")),
            source: ValueSource::Env,
        }
    );
    assert_eq!(
        resolved.shared_server.url,
        ResolvedValue {
            value: Some(String::from("https://env.example:4242")),
            source: ValueSource::Env,
        }
    );
    assert_eq!(
        resolved.shared_server.password,
        ResolvedValue {
            value: Some(String::from("env-secret")),
            source: ValueSource::Env,
        }
    );
}

#[test]
fn remote_runtime_prefers_ezm_env_remote_prefix_when_present() {
    let mut env = HashMap::new();
    env.insert(
        String::from(EZM_REMOTE_DIR_PREFIX_ENV),
        String::from("/ezm/env-remotes"),
    );

    let file = FileConfig {
        operator: None,
        ezm_remote_dir_prefix: Some(String::from("/ezm/file-remotes")),
        ezm_remote_server_url: None,
        opencode_server_url: None,
        opencode_server_password: None,
    };

    let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_dir_prefix,
        ResolvedValue {
            value: Some(String::from("/ezm/env-remotes")),
            source: ValueSource::Env,
        }
    );
}

#[test]
fn remote_runtime_uses_ezm_file_remote_prefix_when_env_missing() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    fs::write(&path, "ezm_remote_dir_prefix = '/ezm/file-remotes'\n").expect("write config");

    let mut env = HashMap::new();
    env.insert(String::from(EZM_CONFIG_ENV), path.display().to_string());

    let loaded = load_config(&env, OperatingSystem::Linux).expect("load should succeed");
    let resolved = resolve_remote_runtime(&env, &loaded.values).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_dir_prefix,
        ResolvedValue {
            value: Some(String::from("/ezm/file-remotes")),
            source: ValueSource::File,
        }
    );
}

#[test]
fn remote_runtime_prefers_ezm_remote_server_url_env_over_file() {
    let mut env = HashMap::new();
    env.insert(
        String::from(EZM_REMOTE_SERVER_URL_ENV),
        String::from("https://shell.env.example:7443"),
    );

    let file = FileConfig {
        ezm_remote_server_url: Some(String::from("https://shell.file.example:8443")),
        ..FileConfig::default()
    };

    let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_server_url,
        ResolvedValue {
            value: Some(String::from("https://shell.env.example:7443")),
            source: ValueSource::Env,
        }
    );
}

#[test]
fn remote_runtime_uses_ezm_remote_server_url_file_when_env_missing() {
    let env = HashMap::<String, String>::new();
    let file = FileConfig {
        ezm_remote_server_url: Some(String::from("https://shell.file.example:8443")),
        opencode_server_url: Some(String::from("https://shared.attach.example:4096")),
        ..FileConfig::default()
    };

    let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_server_url,
        ResolvedValue {
            value: Some(String::from("https://shell.file.example:8443")),
            source: ValueSource::File,
        }
    );
    assert_eq!(
        resolved.shared_server.url,
        ResolvedValue {
            value: Some(String::from("https://shared.attach.example:4096")),
            source: ValueSource::File,
        }
    );
}

#[test]
fn remote_runtime_does_not_reuse_opencode_server_url_as_shell_remote_server_url() {
    let mut env = HashMap::new();
    env.insert(
        String::from(OPENCODE_SERVER_URL_ENV),
        String::from("https://shared.attach.example:4096"),
    );

    let resolved =
        resolve_remote_runtime(&env, &FileConfig::default()).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_server_url,
        ResolvedValue {
            value: None,
            source: ValueSource::Default,
        }
    );
    assert_eq!(
        resolved.shared_server.url,
        ResolvedValue {
            value: Some(String::from("https://shared.attach.example:4096")),
            source: ValueSource::Env,
        }
    );
}

#[test]
fn remote_runtime_uses_config_when_env_is_missing() {
    let env = HashMap::<String, String>::new();
    let file = FileConfig {
        operator: None,
        ezm_remote_dir_prefix: Some(String::from("/file/remotes")),
        ezm_remote_server_url: None,
        opencode_server_url: Some(String::from("https://shared.example:7443")),
        opencode_server_password: Some(String::from("file-secret")),
    };

    let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_dir_prefix,
        ResolvedValue {
            value: Some(String::from("/file/remotes")),
            source: ValueSource::File,
        }
    );
    assert_eq!(
        resolved.shared_server.url,
        ResolvedValue {
            value: Some(String::from("https://shared.example:7443")),
            source: ValueSource::File,
        }
    );
    assert_eq!(
        resolved.shared_server.password,
        ResolvedValue {
            value: Some(String::from("file-secret")),
            source: ValueSource::File,
        }
    );
}

#[test]
fn remote_runtime_defaults_shared_server_url_to_none_when_unset() {
    let env = HashMap::<String, String>::new();
    let file = FileConfig::default();

    let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

    assert_eq!(
        resolved.shared_server.url,
        ResolvedValue {
            value: None,
            source: ValueSource::Default,
        }
    );
}

#[test]
fn invalid_server_url_fails_fast() {
    let mut env = HashMap::new();
    env.insert(
        String::from(OPENCODE_SERVER_URL_ENV),
        String::from("localhost:4096"),
    );

    let error = resolve_remote_runtime(&env, &FileConfig::default())
        .expect_err("invalid url should fail fast");
    assert!(matches!(
        error,
        ConfigError::InvalidOpenCodeServerUrl {
            origin: "env OPENCODE_SERVER_URL"
        }
    ));
}

#[test]
fn operator_env_constant_is_contract_stable() {
    assert_eq!(OPERATOR_ENV, "OPERATOR");
}

#[test]
fn remote_shared_server_env_constants_are_contract_stable() {
    assert_eq!(EZM_REMOTE_DIR_PREFIX_ENV, "EZM_REMOTE_DIR_PREFIX");
    assert_eq!(EZM_REMOTE_SERVER_URL_ENV, "EZM_REMOTE_SERVER_URL");
    assert_eq!(OPENCODE_SERVER_URL_ENV, "OPENCODE_SERVER_URL");
    assert_eq!(OPENCODE_SERVER_PASSWORD_ENV, "OPENCODE_SERVER_PASSWORD");
}
