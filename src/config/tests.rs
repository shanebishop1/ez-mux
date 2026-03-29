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
        std::path::PathBuf::from("/tmp/xdg/ez-mux/ez-mux.toml")
    );
}

#[test]
fn linux_falls_back_to_home_config() {
    let mut env = HashMap::new();
    env.insert(String::from("HOME"), String::from("/tmp/home"));

    let path = resolve_config_path(&env, OperatingSystem::Linux).expect("path should resolve");
    assert_eq!(
        path,
        std::path::PathBuf::from("/tmp/home/.config/ez-mux/ez-mux.toml")
    );
}

#[test]
fn macos_uses_application_support() {
    let mut env = HashMap::new();
    env.insert(String::from("HOME"), String::from("/Users/tester"));

    let path = resolve_config_path(&env, OperatingSystem::MacOs).expect("path should resolve");
    assert_eq!(
        path,
        std::path::PathBuf::from("/Users/tester/Library/Application Support/ez-mux/ez-mux.toml")
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
        std::path::PathBuf::from("/tmp/home/.config/ez-mux/ez-mux.toml")
    );
}

#[test]
fn load_config_prefers_local_ez_mux_toml_when_present() {
    let dir = tempdir().expect("tempdir");
    let local_config = dir.path().join("ez-mux.toml");
    fs::write(&local_config, "ezm_remote_path = '/local/remotes'\n").expect("write local config");

    let env = HashMap::<String, String>::new();
    let loaded =
        super::load::load_config_with_current_dir(&env, OperatingSystem::Linux, Some(dir.path()))
            .expect("load should succeed");

    assert_eq!(loaded.path, local_config);
    assert_eq!(
        loaded.values.ezm_remote_path.as_deref(),
        Some("/local/remotes")
    );
}

#[test]
fn load_config_parses_panes_setting() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    fs::write(&path, "panes = 4\n").expect("write config");

    let mut env = HashMap::new();
    env.insert(String::from(EZM_CONFIG_ENV), path.display().to_string());

    let loaded = load_config(&env, OperatingSystem::Linux).expect("load should succeed");
    assert_eq!(loaded.values.panes, Some(4));
}

#[test]
fn ezm_config_override_wins_over_local_ez_mux_toml() {
    let dir = tempdir().expect("tempdir");
    let local_config = dir.path().join("ez-mux.toml");
    fs::write(&local_config, "ezm_remote_path = '/local/remotes'\n").expect("write local config");

    let explicit_config = dir.path().join("custom.toml");
    fs::write(&explicit_config, "ezm_remote_path = '/explicit/remotes'\n")
        .expect("write explicit config");

    let mut env = HashMap::new();
    env.insert(
        String::from(EZM_CONFIG_ENV),
        explicit_config.display().to_string(),
    );

    let loaded =
        super::load::load_config_with_current_dir(&env, OperatingSystem::Linux, Some(dir.path()))
            .expect("load should succeed");

    assert_eq!(loaded.path, explicit_config);
    assert_eq!(
        loaded.values.ezm_remote_path.as_deref(),
        Some("/explicit/remotes")
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
    fs::write(&path, "ezm_remote_path = [").expect("write");

    let mut env = HashMap::new();
    env.insert(String::from(EZM_CONFIG_ENV), path.display().to_string());

    let error = load_config(&env, OperatingSystem::Linux).expect_err("load should fail");
    assert!(matches!(error, ConfigError::InvalidToml { .. }));
}

#[test]
fn remote_runtime_prefers_env_over_file_values() {
    let mut env = HashMap::new();
    env.insert(
        String::from(EZM_REMOTE_PATH_ENV),
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
        ezm_remote_path: Some(String::from("/file/remotes")),
        ezm_remote_server_url: None,
        opencode_server_url: Some(String::from("https://file.example:4096")),
        opencode_server_password: Some(String::from("file-secret")),
        ..FileConfig::default()
    };

    let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_path,
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
fn remote_runtime_prefers_ezm_env_remote_path_when_present() {
    let mut env = HashMap::new();
    env.insert(
        String::from(EZM_REMOTE_PATH_ENV),
        String::from("/ezm/env-remotes"),
    );

    let file = FileConfig {
        ezm_remote_path: Some(String::from("/ezm/file-remotes")),
        ezm_remote_server_url: None,
        opencode_server_url: None,
        opencode_server_password: None,
        ..FileConfig::default()
    };

    let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_path,
        ResolvedValue {
            value: Some(String::from("/ezm/env-remotes")),
            source: ValueSource::Env,
        }
    );
}

#[test]
fn remote_runtime_uses_ezm_file_remote_path_when_env_missing() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    fs::write(&path, "ezm_remote_path = '/ezm/file-remotes'\n").expect("write config");

    let mut env = HashMap::new();
    env.insert(String::from(EZM_CONFIG_ENV), path.display().to_string());

    let loaded = load_config(&env, OperatingSystem::Linux).expect("load should succeed");
    let resolved = resolve_remote_runtime(&env, &loaded.values).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_path,
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
        ezm_remote_path: Some(String::from("/file/remotes")),
        ezm_remote_server_url: None,
        opencode_server_url: Some(String::from("https://shared.example:7443")),
        opencode_server_password: Some(String::from("file-secret")),
        ..FileConfig::default()
    };

    let resolved = resolve_remote_runtime(&env, &file).expect("runtime should resolve");

    assert_eq!(
        resolved.remote_path,
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
fn remote_shared_server_env_constants_are_contract_stable() {
    assert_eq!(EZM_REMOTE_PATH_ENV, "EZM_REMOTE_PATH");
    assert_eq!(EZM_REMOTE_SERVER_URL_ENV, "EZM_REMOTE_SERVER_URL");
    assert_eq!(OPENCODE_SERVER_URL_ENV, "OPENCODE_SERVER_URL");
    assert_eq!(OPENCODE_SERVER_PASSWORD_ENV, "OPENCODE_SERVER_PASSWORD");
}

#[test]
fn opencode_theme_runtime_defaults_to_slot_palette_mapping() {
    let runtime = resolve_opencode_theme_runtime(&FileConfig::default());

    assert!(runtime.enabled);
    assert_eq!(runtime.theme_for_slot(1), Some("nightowl"));
    assert_eq!(runtime.theme_for_slot(2), Some("orng"));
    assert_eq!(runtime.theme_for_slot(3), Some("osaka-jade"));
    assert_eq!(runtime.theme_for_slot(4), Some("catppuccin"));
    assert_eq!(runtime.theme_for_slot(5), Some("monokai"));
}

#[test]
fn opencode_theme_runtime_accepts_per_slot_overrides() {
    let file = FileConfig {
        opencode_slot_themes: Some(HashMap::from([
            (String::from("2"), String::from("dracula")),
            (String::from("4"), String::from("nord")),
        ])),
        ..FileConfig::default()
    };

    let runtime = resolve_opencode_theme_runtime(&file);

    assert_eq!(runtime.theme_for_slot(1), Some("nightowl"));
    assert_eq!(runtime.theme_for_slot(2), Some("dracula"));
    assert_eq!(runtime.theme_for_slot(3), Some("osaka-jade"));
    assert_eq!(runtime.theme_for_slot(4), Some("nord"));
    assert_eq!(runtime.theme_for_slot(5), Some("monokai"));
}

#[test]
fn opencode_theme_runtime_can_be_disabled_globally() {
    let file = FileConfig {
        opencode_slot_themes_enabled: Some(false),
        ..FileConfig::default()
    };

    let runtime = resolve_opencode_theme_runtime(&file);

    assert!(!runtime.enabled);
    assert_eq!(runtime.theme_for_slot(1), None);
    assert_eq!(runtime.theme_for_slot(3), None);
    assert_eq!(runtime.theme_for_slot(5), None);
}

#[test]
fn load_config_parses_opencode_slot_theme_settings() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "opencode_slot_themes_enabled = true\n[opencode_slot_themes]\n\"2\" = \"dracula\"\n\"4\" = \"catppuccin\"\n",
    )
    .expect("write config");

    let mut env = HashMap::new();
    env.insert(String::from(EZM_CONFIG_ENV), path.display().to_string());

    let loaded = load_config(&env, OperatingSystem::Linux).expect("load should succeed");
    let runtime = resolve_opencode_theme_runtime(&loaded.values);

    assert!(runtime.enabled);
    assert_eq!(runtime.theme_for_slot(2), Some("dracula"));
    assert_eq!(runtime.theme_for_slot(4), Some("catppuccin"));
}

#[test]
fn resolve_agent_command_uses_trimmed_config_value() {
    let file = FileConfig {
        agent_command: Some(String::from(
            "  exec claude || exec \"${SHELL:-/bin/sh}\" -l  ",
        )),
        ..FileConfig::default()
    };

    let command = resolve_agent_command(&file);

    assert_eq!(
        command.as_deref(),
        Some("exec claude || exec \"${SHELL:-/bin/sh}\" -l")
    );
}

#[test]
fn resolve_agent_command_treats_empty_value_as_unset() {
    let file = FileConfig {
        agent_command: Some(String::from("   ")),
        ..FileConfig::default()
    };

    let command = resolve_agent_command(&file);

    assert!(command.is_none());
}

#[test]
fn pane_count_defaults_to_five_when_unset() {
    let resolved = resolve_pane_count(None, &FileConfig::default()).expect("pane count resolution");

    assert_eq!(
        resolved,
        ResolvedValue {
            value: 5,
            source: ValueSource::Default,
        }
    );
}

#[test]
fn pane_count_uses_config_when_cli_missing() {
    let file = FileConfig {
        panes: Some(3),
        ..FileConfig::default()
    };

    let resolved = resolve_pane_count(None, &file).expect("pane count resolution");

    assert_eq!(
        resolved,
        ResolvedValue {
            value: 3,
            source: ValueSource::File,
        }
    );
}

#[test]
fn pane_count_prefers_cli_over_config() {
    let file = FileConfig {
        panes: Some(2),
        ..FileConfig::default()
    };

    let resolved = resolve_pane_count(Some(4), &file).expect("pane count resolution");

    assert_eq!(
        resolved,
        ResolvedValue {
            value: 4,
            source: ValueSource::Cli,
        }
    );
}

#[test]
fn pane_count_rejects_out_of_range_file_value() {
    let file = FileConfig {
        panes: Some(9),
        ..FileConfig::default()
    };

    let error = resolve_pane_count(None, &file).expect_err("invalid pane count should fail");
    assert!(matches!(
        error,
        ConfigError::InvalidPaneCount {
            origin: "config panes",
            value: 9,
        }
    ));
}
