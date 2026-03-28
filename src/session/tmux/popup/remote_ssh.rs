use super::super::SessionError;
use super::super::remote_authority::parse_remote_ssh_authority;
use super::context::PopupRemoteContext;

pub(super) fn popup_remote_launch_command(
    remote_context: Option<&PopupRemoteContext>,
) -> Result<Option<String>, SessionError> {
    let Some(context) = remote_context else {
        return Ok(None);
    };
    let server_url = context
        .remote_server_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(server_url) = server_url else {
        return Ok(None);
    };

    let authority = parse_remote_ssh_authority(server_url)?;
    let remote_script = format!(
        "cd '{}' && exec \"${{SHELL:-/bin/sh}}\" -l",
        shell_escape_single_quoted(&context.remote_dir)
    );

    let mut ssh_invocation = String::from("ssh -tt");
    if let Some(port) = authority.port {
        ssh_invocation.push_str(" -p ");
        ssh_invocation.push_str(&port.to_string());
    }
    ssh_invocation.push_str(" '");
    ssh_invocation.push_str(&shell_escape_single_quoted(&authority.target));
    ssh_invocation.push('\'');
    ssh_invocation.push_str(" '");
    ssh_invocation.push_str(&shell_escape_single_quoted(&remote_script));
    ssh_invocation.push('\'');

    Ok(Some(format!(
        "sh -lc '{}'",
        shell_escape_single_quoted(&format!(
            "if {ssh_invocation}; then exit 0; fi; ssh_exit_code=$?; printf '%s\\n' \"ez-mux remote ssh launch failed with status $ssh_exit_code\" >&2; exec \"${{SHELL:-/bin/sh}}\" -l"
        ))
    )))
}

pub(super) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(super) fn shell_escape_single_quoted(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}
