use std::collections::HashMap;

use crate::config::SessionRuntimeContext;

use super::super::launch_context::repair_launch_context_from_session_context;

#[test]
fn repair_launch_context_uses_only_the_owning_sessions_runtime_values() {
    let project_b = SessionRuntimeContext {
        remote_path: Some(String::from("/srv/project-b")),
        remote_server_url: Some(String::from("b.example")),
        use_tssh: true,
        use_mosh: false,
        perles_dir: None,
        perles_db: None,
        shared_server_url: Some(String::from("https://b.example:4096")),
        agent_command: Some(String::from("agent-b")),
        opencode_themes_enabled: true,
        opencode_themes_by_slot: HashMap::from([(1, String::from("theme-b"))]),
    };
    let project_a = SessionRuntimeContext {
        remote_path: Some(String::from("/srv/project-a")),
        remote_server_url: Some(String::from("a.example")),
        use_tssh: false,
        use_mosh: true,
        perles_dir: Some(String::from("/project-a/perles")),
        perles_db: None,
        shared_server_url: Some(String::from("https://a.example:4096")),
        agent_command: Some(String::from("agent-a")),
        opencode_themes_enabled: true,
        opencode_themes_by_slot: HashMap::from([(1, String::from("theme-a"))]),
    };

    let unrelated_process_context = repair_launch_context_from_session_context(project_b);
    let resolved = repair_launch_context_from_session_context(project_a);

    assert_eq!(resolved.remote_path.as_deref(), Some("/srv/project-a"));
    assert_eq!(resolved.remote_server_url.as_deref(), Some("a.example"));
    assert!(resolved.use_mosh);
    assert_eq!(resolved.agent_command.as_deref(), Some("agent-a"));
    assert_eq!(
        resolved
            .shared_server
            .as_ref()
            .map(|server| server.url.as_str()),
        Some("https://a.example:4096")
    );
    assert_eq!(resolved.opencode_themes.theme_for_slot(1), Some("theme-a"));
    assert_ne!(resolved.remote_path, unrelated_process_context.remote_path);
    assert!(!format!("{resolved:?}").contains("project-b"));
}
