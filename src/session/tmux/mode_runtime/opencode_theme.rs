use super::remote_launch::escape_single_quotes;

pub(super) fn with_opencode_tui_config_env(
    command: String,
    slot_id: u8,
    opencode_theme: Option<&str>,
) -> String {
    let Some(theme) = opencode_theme
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return command;
    };

    match ensure_opencode_tui_config_directory(slot_id, theme) {
        Ok(directory) => {
            let directory_value = directory.display().to_string();
            let tui_config_path = directory.join("tui.json").display().to_string();
            format!(
                "export OPENCODE_CONFIG_DIR='{}'; export OPENCODE_TUI_CONFIG='{}'; export OPENCODE_TEST_MANAGED_CONFIG_DIR='{}'; {command}",
                escape_single_quotes(&directory_value),
                escape_single_quotes(&tui_config_path),
                escape_single_quotes(&directory_value)
            )
        }
        Err(source) => {
            eprintln!("warning: failed writing opencode tui config for slot {slot_id}: {source}");
            command
        }
    }
}

fn ensure_opencode_tui_config_directory(
    slot_id: u8,
    theme: &str,
) -> Result<std::path::PathBuf, std::io::Error> {
    let directory = opencode_tui_root_directory()
        .join("opencode-tui")
        .join(format!("slot-{slot_id}"));
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("tui.json");
    std::fs::write(path, render_opencode_tui_config(theme))?;
    Ok(directory)
}

fn opencode_tui_root_directory() -> std::path::PathBuf {
    let user_scope = resolve_temp_directory_scope();
    std::env::temp_dir().join(format!("ez-mux-{user_scope}"))
}

fn resolve_temp_directory_scope() -> String {
    ["UID", "USER", "LOGNAME"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|value| normalize_scope_value(&value))
        .find(|value| !value.is_empty())
        .unwrap_or_else(|| format!("pid-{}", std::process::id()))
}

fn normalize_scope_value(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn render_opencode_tui_config(theme: &str) -> String {
    format!(
        "{{\n  \"$schema\": \"https://opencode.ai/tui.json\",\n  \"theme\": \"{}\"\n}}\n",
        escape_json_string(theme)
    )
}

fn escape_json_string(value: &str) -> String {
    use std::fmt::Write;

    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{0008}' => escaped.push_str("\\b"),
            '\u{000c}' => escaped.push_str("\\f"),
            c if c.is_control() => {
                let _ = write!(escaped, "\\u{:04x}", u32::from(c));
            }
            c => escaped.push(c),
        }
    }
    escaped
}
