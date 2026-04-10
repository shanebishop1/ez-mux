use super::super::SessionError;
use super::super::remote_authority::parse_remote_ssh_authority;
use super::super::remote_transport::{build_remote_invocation, remote_transport_label};
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

    let remote_invocation = build_remote_invocation(
        &authority,
        &remote_script,
        context.use_tssh,
        context.use_mosh,
    );
    let transport = remote_transport_label(context.use_tssh, context.use_mosh);

    Ok(Some(format!(
        "sh -lc '{}'",
        shell_escape_single_quoted(&format!(
            "if {remote_invocation}; then exit 0; fi; remote_exit_code=$?; printf '%s\\n' \"ez-mux remote {transport} launch failed with status $remote_exit_code\" >&2; exec \"${{SHELL:-/bin/sh}}\" -l"
        ))
    )))
}

pub(super) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

pub(super) fn shell_escape_single_quoted(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}
