use super::super::SessionError;
use super::super::SlotMode;
use crate::session::{RemoteModeContext, resolve_remote_path};

pub(super) fn launch_command_with_remote_dir_from_mapping(
    mode: SlotMode,
    launch_command: &str,
    cwd: &str,
    remote_context: RemoteModeContext<'_>,
) -> Result<String, SessionError> {
    let resolved = resolve_remote_path(std::path::Path::new(cwd), remote_context.remote_path)?;

    if !resolved.remapped {
        return Ok(launch_command.to_owned());
    }

    if let Some(ssh_command) = ssh_wrapped_launch_command(
        mode,
        &resolved.effective_path.display().to_string(),
        launch_command,
        remote_context.remote_server_url,
    ) {
        return Ok(ssh_command);
    }

    let mut exports = vec![format!(
        "export EZM_REMOTE_DIR='{}'",
        escape_single_quotes(&resolved.effective_path.display().to_string())
    )];
    if let Some(server_url) = remote_context
        .remote_server_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        exports.push(format!(
            "export EZM_REMOTE_SERVER_URL='{}'",
            escape_single_quotes(server_url)
        ));
    }

    Ok(format!("{}; {launch_command}", exports.join("; ")))
}

fn ssh_wrapped_launch_command(
    mode: SlotMode,
    remote_dir: &str,
    launch_command: &str,
    remote_server_url: Option<&str>,
) -> Option<String> {
    if !matches!(mode, SlotMode::Shell | SlotMode::Neovim | SlotMode::Lazygit) {
        return None;
    }

    let server_url = remote_server_url
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let (target, port) = ssh_target_and_port(server_url);
    if target.is_empty() {
        return None;
    }

    let remote_script = format!(
        "cd '{}' && {launch_command}",
        escape_single_quotes(remote_dir)
    );
    let mut ssh_invocation = String::from("ssh -tt");
    if let Some(port) = port {
        ssh_invocation.push_str(&format!(" -p {port}"));
    }
    ssh_invocation.push_str(&format!(" '{}'", escape_single_quotes(&target)));
    ssh_invocation.push_str(&format!(" '{}'", escape_single_quotes(&remote_script)));
    Some(format!(
        "if {ssh_invocation}; then exit 0; fi; ssh_exit_code=$?; printf '%s\\n' \"ez-mux remote ssh launch failed with status $ssh_exit_code\" >&2; exec \"${{SHELL:-/bin/sh}}\" -l"
    ))
}

fn ssh_target_and_port(server_url: &str) -> (String, Option<u16>) {
    let normalized = normalize_ssh_authority(server_url);
    if normalized.is_empty() {
        return (String::new(), None);
    }

    parse_authority_host_and_port(normalized)
}

fn normalize_ssh_authority(server_url: &str) -> &str {
    let trimmed = server_url.trim();
    let without_scheme = trimmed
        .split_once("://")
        .map_or(trimmed, |(_, remainder)| remainder);

    without_scheme.split('/').next().unwrap_or_default().trim()
}

fn parse_authority_host_and_port(authority: &str) -> (String, Option<u16>) {
    if let Some((host, port)) = parse_bracketed_authority(authority) {
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

fn parse_bracketed_authority(authority: &str) -> Option<(String, Option<u16>)> {
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

pub(super) fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}
