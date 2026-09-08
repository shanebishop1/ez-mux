/// Normalizes a configured executable hint without interpreting it as shell
/// syntax. The caller remains responsible for quoting the resulting token for
/// its command boundary.
pub(crate) fn normalize_shell_binary_hint(value: &str) -> Option<String> {
    let mut normalized = value.trim();

    loop {
        let previous = normalized;
        normalized = strip_quote_like_prefix(normalized);
        normalized = strip_quote_like_suffix(normalized);
        normalized = normalized.trim();
        if normalized == previous {
            break;
        }
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_owned())
    }
}

pub(crate) fn binary_hint_looks_like_single_executable(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '\'' | '"' | '\0'))
}

fn strip_quote_like_prefix(value: &str) -> &str {
    if let Some(stripped) = value.strip_prefix("\\\"") {
        return stripped;
    }

    if let Some(stripped) = value.strip_prefix("\\'") {
        return stripped;
    }

    if let Some(stripped) = value.strip_prefix('"') {
        return stripped;
    }

    if let Some(stripped) = value.strip_prefix('\'') {
        return stripped;
    }

    value
}

fn strip_quote_like_suffix(value: &str) -> &str {
    if let Some(stripped) = value.strip_suffix("\\\"") {
        return stripped;
    }

    if let Some(stripped) = value.strip_suffix("\\'") {
        return stripped;
    }

    if let Some(stripped) = value.strip_suffix('"') {
        return stripped;
    }

    if let Some(stripped) = value.strip_suffix('\'') {
        return stripped;
    }

    value
}

#[cfg(test)]
mod tests {
    use super::{binary_hint_looks_like_single_executable, normalize_shell_binary_hint};

    #[test]
    fn normalizes_quote_wrapped_and_unbalanced_hints() {
        for (input, expected) in [
            ("'/tmp/ezm'", Some("/tmp/ezm")),
            ("\"/tmp/ezm\"", Some("/tmp/ezm")),
            ("'/tmp/ezm", Some("/tmp/ezm")),
            ("/tmp/ezm'", Some("/tmp/ezm")),
            ("\\\"/tmp/ezm\\\"", Some("/tmp/ezm")),
            ("", None),
            ("   ", None),
        ] {
            assert_eq!(normalize_shell_binary_hint(input).as_deref(), expected);
        }
    }

    #[test]
    fn accepts_only_one_executable_token() {
        assert!(binary_hint_looks_like_single_executable("/tmp/ezm"));
        assert!(!binary_hint_looks_like_single_executable(
            "/tmp/ezm __internal focus"
        ));
        assert!(!binary_hint_looks_like_single_executable("/tmp/ezm\0"));
    }
}
