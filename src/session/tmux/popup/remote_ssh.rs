use super::context::PopupRemoteContext;

pub(super) fn popup_remote_launch_command(
    remote_context: Option<&PopupRemoteContext>,
) -> Option<String> {
    let context = remote_context?;
    let server_url = context
        .remote_server_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;

    let (target, port) = popup_ssh_target_and_port(server_url);
    if target.is_empty() {
        return None;
    }

    let remote_script = format!(
        "cd '{}' && exec \"${{SHELL:-/bin/sh}}\" -l",
        shell_escape_single_quoted(&context.remote_dir)
    );

    let mut ssh_invocation = String::from("ssh -tt");
    if let Some(port) = port {
        ssh_invocation.push_str(&format!(" -p {port}"));
    }
    ssh_invocation.push_str(&format!(" '{}'", shell_escape_single_quoted(&target)));
    ssh_invocation.push_str(&format!(
        " '{}'",
        shell_escape_single_quoted(&remote_script)
    ));

    Some(format!(
        "sh -lc '{}'",
        shell_escape_single_quoted(&format!(
            "if {ssh_invocation}; then exit 0; fi; ssh_exit_code=$?; printf '%s\\n' \"ez-mux remote ssh launch failed with status $ssh_exit_code\" >&2; exec \"${{SHELL:-/bin/sh}}\" -l"
        ))
    ))
}

fn popup_ssh_target_and_port(server_url: &str) -> (String, Option<u16>) {
    let normalized = popup_normalize_ssh_authority(server_url);
    if normalized.is_empty() {
        return (String::new(), None);
    }

    popup_parse_authority_host_and_port(normalized)
}

fn popup_normalize_ssh_authority(server_url: &str) -> &str {
    let trimmed = server_url.trim();
    let without_scheme = trimmed
        .split_once("://")
        .map_or(trimmed, |(_, remainder)| remainder);

    without_scheme.split('/').next().unwrap_or_default().trim()
}

fn popup_parse_authority_host_and_port(authority: &str) -> (String, Option<u16>) {
    if let Some((host, port)) = popup_parse_bracketed_authority(authority) {
        return (host, port);
    }

    if let Some((host, port)) = authority.rsplit_once(':') {
        let parsed_port = port.parse::<u16>().ok();
        if !host.contains(':') && parsed_port.is_some() {
            return (host.to_owned(), parsed_port);
        }
    }

    (authority.to_owned(), None)
}

fn popup_parse_bracketed_authority(authority: &str) -> Option<(String, Option<u16>)> {
    if !authority.starts_with('[') {
        return None;
    }

    let closing = authority.find(']')?;
    let host = authority[..=closing].to_owned();
    let remainder = authority[(closing + 1)..].trim();
    if remainder.is_empty() {
        return Some((host, None));
    }

    let port = remainder
        .strip_prefix(':')
        .and_then(|candidate| candidate.parse::<u16>().ok());
    Some((host, port))
}

pub(super) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(super) fn shell_escape_single_quoted(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}
