use crate::support::foundation_harness::FoundationHarness;

#[derive(Debug)]
pub(crate) struct KeyBindingCheck {
    pub(crate) pass: bool,
    pub(crate) detail: String,
}

#[derive(Debug, Eq, PartialEq)]
struct ParsedKeyBinding {
    table: String,
    key: String,
    command: String,
}

pub(crate) fn extract_stdout_field(stdout: &str, key: &str) -> Option<String> {
    let marker = format!("{key}=");
    let start = stdout.find(&marker)? + marker.len();
    let tail = &stdout[start..];
    let end = tail.find(';').unwrap_or(tail.len());
    Some(tail[..end].trim().trim_end_matches('.').to_owned())
}

pub(crate) fn send_prefix_keybind(
    harness: &FoundationHarness,
    session_name: &str,
    key: &str,
) -> Result<(), String> {
    let target = format!("{session_name}:0");

    if harness
        .tmux_capture(&["send-keys", "-K", "-t", &target, "C-b", key])
        .is_ok()
    {
        return Ok(());
    }

    harness
        .tmux_capture(&["send-keys", "-t", &target, "C-b", key])
        .map(|_| ())
}

pub(crate) fn check_key_binding(
    harness: &FoundationHarness,
    table: &str,
    key: &str,
    command_markers: &[&str],
) -> KeyBindingCheck {
    let version = harness
        .tmux_version()
        .unwrap_or_else(|error| format!("unknown ({error})"));
    let table_output = match harness.tmux_capture(&["list-keys", "-T", table]) {
        Ok(output) => output,
        Err(error) => {
            return KeyBindingCheck {
                pass: false,
                detail: format!(
                    "key binding lookup failed: table={table:?}; key={key:?}; tmux_version={version}; captured table output=<unavailable>; error={error}"
                ),
            };
        }
    };

    let Some(binding) = find_key_binding(&table_output, table, key) else {
        return KeyBindingCheck {
            pass: false,
            detail: format!(
                "key binding missing: table={table:?}; key={key:?}; tmux_version={version}; captured table output:\n{table_output}"
            ),
        };
    };

    let missing_markers = command_markers
        .iter()
        .copied()
        .filter(|marker| !binding.command.contains(marker))
        .collect::<Vec<_>>();
    if missing_markers.is_empty() {
        KeyBindingCheck {
            pass: true,
            detail: format!(
                "key binding matched: table={table:?}; key={key:?}; tmux_version={version}; command={}",
                binding.command
            ),
        }
    } else {
        KeyBindingCheck {
            pass: false,
            detail: format!(
                "key binding command mismatch: table={table:?}; key={key:?}; tmux_version={version}; missing_markers={missing_markers:?}; parsed_command={}; captured table output:\n{table_output}",
                binding.command
            ),
        }
    }
}

fn find_key_binding(output: &str, table: &str, key: &str) -> Option<ParsedKeyBinding> {
    output
        .lines()
        .filter_map(parse_key_binding_line)
        .find(|binding| binding.table == table && binding.key == key)
}

fn parse_key_binding_line(line: &str) -> Option<ParsedKeyBinding> {
    let tokens = tmux_list_keys_tokens(line);
    if !matches!(
        tokens.first().map(String::as_str),
        Some("bind-key" | "bind")
    ) {
        return None;
    }

    let table_index = tokens.iter().position(|token| token == "-T")?;
    let table = tokens.get(table_index + 1)?.clone();
    let key = tokens.get(table_index + 2)?.clone();
    let command = tokens
        .get(table_index + 3..)
        .map(|tokens| tokens.join(" "))
        .unwrap_or_default();
    Some(ParsedKeyBinding {
        table,
        key,
        command,
    })
}

fn tmux_list_keys_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quote = None;
    let mut escaped = false;

    for character in line.chars() {
        if escaped {
            token.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if character == active_quote {
                quote = None;
            } else {
                token.push(character);
            }
        } else if character == '\'' || character == '"' {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !token.is_empty() {
                tokens.push(std::mem::take(&mut token));
            }
        } else {
            token.push(character);
        }
    }
    if escaped {
        token.push('\\');
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::{ParsedKeyBinding, find_key_binding};

    #[test]
    fn key_binding_parser_rejects_missing_binding() {
        let output = "bind-key -T prefix f switch-client -T ezm-focus\n";

        assert!(find_key_binding(output, "prefix", "P").is_none());
    }

    #[test]
    fn key_binding_parser_matches_exact_key_not_similarly_named_key() {
        let output = concat!(
            "bind-key -T prefix P-old run-shell old-route\n",
            "bind-key -T prefix P run-shell -b 'ezm __internal popup'\n",
        );

        let binding = find_key_binding(output, "prefix", "P");
        assert_eq!(
            binding,
            Some(ParsedKeyBinding {
                table: String::from("prefix"),
                key: String::from("P"),
                command: String::from("run-shell -b ezm __internal popup"),
            })
        );
    }

    #[test]
    fn key_binding_parser_handles_observed_supported_version_output() {
        let output = concat!(
            "bind-key    -T prefix M-3 if-shell -F '#{@ezm_slot_1_pane}' \\; ",
            "run-shell -b 'ezm __internal preset --preset three-pane'\n",
        );

        let binding = find_key_binding(output, "prefix", "M-3").expect("binding should parse");
        assert!(binding.command.contains("__internal preset"));
        assert!(binding.command.contains("three-pane"));
    }
}
