use super::remote_launch::escape_single_quotes;

pub(super) fn with_opencode_tui_config_env(
    command: String,
    _slot_id: u8,
    opencode_theme: Option<&str>,
) -> String {
    let Some(theme) = opencode_theme
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return command;
    };
    // v2 merges this appearance override over the user's cli.json. Never replace
    // the server configuration directory just to color one terminal client.
    let mut config = std::env::var("OPENCODE_CLI_CONFIG_CONTENT")
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    if !config
        .get("theme")
        .is_some_and(serde_json::Value::is_object)
    {
        config["theme"] = serde_json::json!({});
    }
    config["theme"]["name"] = theme.into();
    let content = config.to_string();
    format!(
        "export OPENCODE_CLI_CONFIG_CONTENT='{}'; {command}",
        escape_single_quotes(&content)
    )
}
