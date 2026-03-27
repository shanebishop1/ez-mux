use super::super::SessionError;
use super::super::SlotMode;
use super::super::remote_authority::parse_remote_ssh_authority;
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
    )? {
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
) -> Result<Option<String>, SessionError> {
    if !matches!(mode, SlotMode::Shell | SlotMode::Neovim | SlotMode::Lazygit) {
        return Ok(None);
    }

    let server_url = remote_server_url
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(server_url) = server_url else {
        return Ok(None);
    };

    let authority = parse_remote_ssh_authority(server_url)?;
    let remote_script = format!(
        "cd '{}' && {launch_command}",
        escape_single_quotes(remote_dir)
    );

    let mut ssh_invocation = String::from("ssh -tt");
    if let Some(port) = authority.port {
        ssh_invocation.push_str(&format!(" -p {port}"));
    }
    ssh_invocation.push_str(&format!(" '{}'", escape_single_quotes(&authority.target)));
    ssh_invocation.push_str(&format!(" '{}'", escape_single_quotes(&remote_script)));

    Ok(Some(format!(
        "if {ssh_invocation}; then exit 0; fi; ssh_exit_code=$?; printf '%s\\n' \"ez-mux remote ssh launch failed with status $ssh_exit_code\" >&2; exec \"${{SHELL:-/bin/sh}}\" -l"
    )))
}

pub(super) fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}
