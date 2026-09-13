use std::process::{Command, Output};
use std::sync::OnceLock;
use std::time::Instant;

use super::SessionError;
use crate::config::OPENCODE_SERVER_PASSWORD_ENV;

const REDACTED_SECRET_VALUE: &str = "<redacted>";
const STARTUP_TRACE_TMUX_ENV: &str = "EZM_STARTUP_TRACE_TMUX";

pub(super) fn tmux_run_batch(commands: &[Vec<String>]) -> Result<(), SessionError> {
    if commands.is_empty() {
        return Ok(());
    }

    let mut flattened_args = Vec::new();
    let mut first = true;
    for command in commands {
        if command.is_empty() {
            continue;
        }
        if !first {
            flattened_args.push(String::from(";"));
        }
        first = false;
        flattened_args.extend(command.iter().cloned());
    }

    if flattened_args.is_empty() {
        return Ok(());
    }

    let diagnostics = tmux_batch_command_for_diagnostics(commands);
    let secret_values = secret_values_from_batch(commands);
    let started_at = Instant::now();
    let flat_refs = flattened_args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let output = Command::new("tmux")
        .args(&flat_refs)
        .output()
        .map_err(|source| SessionError::TmuxSpawnFailed {
            command: diagnostics.clone(),
            source,
        })?;
    trace_tmux_command(&diagnostics, &output, started_at.elapsed());

    if output.status.success() {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: diagnostics,
        stderr: format_output_diagnostics_with_secrets(&output, &secret_values),
    })
}

pub(super) fn tmux_output(args: &[&str]) -> Result<Output, SessionError> {
    let diagnostics = tmux_command_for_diagnostics(args);
    let started_at = Instant::now();
    let output = Command::new("tmux").args(args).output().map_err(|source| {
        SessionError::TmuxSpawnFailed {
            command: diagnostics.clone(),
            source,
        }
    })?;
    trace_tmux_command(&diagnostics, &output, started_at.elapsed());
    Ok(output)
}

pub(super) fn tmux_run(args: &[&str]) -> Result<(), SessionError> {
    let output = tmux_output(args)?;
    if output.status.success() {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: tmux_command_for_diagnostics(args),
        stderr: format_output_diagnostics_with_args(&output, args),
    })
}

pub(super) fn tmux_output_value(args: &[&str]) -> Result<String, SessionError> {
    let output = tmux_output(args)?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    if let Some(retried_stdout) = retry_legacy_window_zero_list_panes(args, &output)? {
        return Ok(retried_stdout);
    }

    Err(SessionError::TmuxCommandFailed {
        command: tmux_command_for_diagnostics(args),
        stderr: format_output_diagnostics_with_args(&output, args),
    })
}

pub(super) fn tmux_primary_window_target(session_name: &str) -> Result<String, SessionError> {
    let command = format!("list-windows -t {session_name} -F #{{window_active}}|#{{window_id}}");
    let output = tmux_output_value(&[
        "list-windows",
        "-t",
        session_name,
        "-F",
        "#{window_active}|#{window_id}",
    ])?;
    parse_primary_window_target(&output).ok_or_else(|| SessionError::TmuxCommandFailed {
        command,
        stderr: String::from("tmux returned no window id for session"),
    })
}

fn parse_primary_window_target(output: &str) -> Option<String> {
    let mut fallback = None;
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.splitn(2, '|');
        let active = parts.next().unwrap_or_default().trim();
        let window_id = parts.next().unwrap_or_default().trim();
        if window_id.is_empty() {
            continue;
        }
        if fallback.is_none() {
            fallback = Some(window_id.to_owned());
        }
        if active == "1" {
            return Some(window_id.to_owned());
        }
    }

    fallback
}

fn retry_legacy_window_zero_list_panes(
    args: &[&str],
    output: &Output,
) -> Result<Option<String>, SessionError> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let Some((target_index, session_name)) = legacy_window_zero_session_target(args, &stderr)
    else {
        return Ok(None);
    };

    let primary_target = tmux_primary_window_target(session_name)?;
    let mut owned_args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    owned_args[target_index] = primary_target;
    let retry_args = owned_args.iter().map(String::as_str).collect::<Vec<_>>();
    let retry_output = tmux_output(&retry_args)?;
    if retry_output.status.success() {
        return Ok(Some(
            String::from_utf8_lossy(&retry_output.stdout).into_owned(),
        ));
    }

    Err(SessionError::TmuxCommandFailed {
        command: tmux_command_for_diagnostics(&retry_args),
        stderr: format_output_diagnostics_with_args(&retry_output, &retry_args),
    })
}

fn tmux_command_for_diagnostics(args: &[&str]) -> String {
    tmux_command_for_diagnostics_owned(args.iter().map(|arg| (*arg).to_owned()).collect())
}

fn tmux_command_for_diagnostics_owned(mut args: Vec<String>) -> String {
    redact_set_environment_secret_value(&mut args, OPENCODE_SERVER_PASSWORD_ENV);
    redact_new_session_environment_secret_value(&mut args, OPENCODE_SERVER_PASSWORD_ENV);
    redact_diagnostic_text(&args.join(" "), &[])
}

fn tmux_batch_command_for_diagnostics(commands: &[Vec<String>]) -> String {
    commands
        .iter()
        .filter(|command| !command.is_empty())
        .map(|command| tmux_command_for_diagnostics_owned(command.clone()))
        .collect::<Vec<_>>()
        .join(" \\; ")
}

fn secret_values_from_batch(commands: &[Vec<String>]) -> Vec<String> {
    commands
        .iter()
        .flat_map(|command| secret_values_from_owned_args(command))
        .collect()
}

fn secret_values_from_args(args: &[&str]) -> Vec<String> {
    let owned = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    secret_values_from_owned_args(&owned)
}

fn secret_values_from_owned_args(args: &[String]) -> Vec<String> {
    let mut values = args
        .windows(2)
        .filter(|window| window[0] == OPENCODE_SERVER_PASSWORD_ENV)
        .map(|window| window[1].clone())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    let prefix = format!("{OPENCODE_SERVER_PASSWORD_ENV}=");
    values.extend(
        args.windows(2)
            .filter(|window| window[0] == "-e")
            .filter_map(|window| window[1].strip_prefix(&prefix))
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    );
    values
}

fn redact_set_environment_secret_value(args: &mut [String], secret_key: &str) {
    if args.first().map(String::as_str) != Some("set-environment") {
        return;
    }

    let Some(key_index) = args.iter().position(|arg| arg == secret_key) else {
        return;
    };

    let Some(value) = args.get_mut(key_index + 1) else {
        return;
    };

    *value = String::from(REDACTED_SECRET_VALUE);
}

fn redact_new_session_environment_secret_value(args: &mut [String], secret_key: &str) {
    let prefix = format!("{secret_key}=");
    for index in 0..args.len().saturating_sub(1) {
        if args[index] != "-e" {
            continue;
        }
        let Some(value) = args[index + 1].strip_prefix(&prefix) else {
            continue;
        };
        if value.is_empty() {
            continue;
        }
        args[index + 1] = format!("{prefix}{REDACTED_SECRET_VALUE}");
    }
}

fn legacy_window_zero_session_target<'a>(
    args: &[&'a str],
    stderr: &str,
) -> Option<(usize, &'a str)> {
    if args.first().copied() != Some("list-panes") {
        return None;
    }
    if !stderr.to_ascii_lowercase().contains("can't find window: 0") {
        return None;
    }

    let target_flag_index = args.iter().position(|arg| *arg == "-t")?;
    let target_index = target_flag_index + 1;
    let target = *args.get(target_index)?;
    let session = target.strip_suffix(":0")?;
    if session.is_empty() {
        return None;
    }

    Some((target_index, session))
}

fn trace_tmux_command(command: &str, output: &Output, elapsed: std::time::Duration) {
    if !startup_trace_tmux_enabled() {
        return;
    }

    let status_code = output
        .status
        .code()
        .map_or_else(|| String::from("signal"), |code| code.to_string());
    eprintln!("{}", render_startup_trace(command, &status_code, elapsed));
}

fn render_startup_trace(command: &str, status_code: &str, elapsed: std::time::Duration) -> String {
    format!(
        "startup-trace tmux delta_ms={:.2} status={} cmd=tmux {}",
        elapsed.as_secs_f64() * 1000.0,
        status_code,
        command
    )
}

fn startup_trace_tmux_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(STARTUP_TRACE_TMUX_ENV).is_ok_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
    })
}

pub(super) fn format_output_diagnostics(output: &Output) -> String {
    format_output_diagnostics_with_secrets(output, &[])
}

fn format_output_diagnostics_with_args(output: &Output, args: &[&str]) -> String {
    let secret_values = secret_values_from_args(args);
    format_output_diagnostics_with_secrets(output, &secret_values)
}

fn format_output_diagnostics_with_secrets(output: &Output, secret_values: &[String]) -> String {
    let status = output
        .status
        .code()
        .map_or_else(|| String::from("signal"), |code| code.to_string());
    let stdout = redact_diagnostic_text(
        String::from_utf8_lossy(&output.stdout).trim(),
        secret_values,
    );
    let stderr = redact_diagnostic_text(
        String::from_utf8_lossy(&output.stderr).trim(),
        secret_values,
    );

    format!("status={status}; stdout={stdout:?}; stderr={stderr:?}")
}

fn redact_diagnostic_text(value: &str, secret_values: &[String]) -> String {
    let mut rendered = value.to_owned();
    for secret in secret_values.iter().filter(|secret| !secret.is_empty()) {
        rendered = rendered.replace(secret, REDACTED_SECRET_VALUE);
    }
    rendered = redact_embedded_authorities(&rendered);
    rendered = redact_named_secret_values(&rendered);
    rendered
}

fn redact_embedded_authorities(value: &str) -> String {
    let mut rendered = String::with_capacity(value.len());
    let mut cursor = 0;

    while let Some(relative_scheme_end) = value[cursor..].find("://") {
        let scheme_end = cursor + relative_scheme_end;
        let scheme_start = value[..scheme_end]
            .char_indices()
            .rev()
            .take_while(|(_, ch)| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
            .last()
            .map_or(scheme_end, |(index, _)| index);
        if scheme_start == scheme_end {
            break;
        }

        let end = value[scheme_start..]
            .char_indices()
            // URL userinfo permits sub-delimiters such as `;`, `,`, `(`, and
            // `)`. Keep them inside the candidate so a credential cannot end
            // the scanner before the authority's `@` delimiter.
            .find(|(_, ch)| ch.is_whitespace() || matches!(ch, '\'' | '"' | '`'))
            .map_or(value.len(), |(index, _)| scheme_start + index);

        rendered.push_str(&value[cursor..scheme_start]);
        rendered.push_str(&super::remote_authority::redact_remote_authority_value(
            &value[scheme_start..end],
        ));
        cursor = end;
    }

    rendered.push_str(&value[cursor..]);
    rendered
}

fn redact_named_secret_values(value: &str) -> String {
    let marker = format!("{OPENCODE_SERVER_PASSWORD_ENV}=");
    let mut rendered = String::with_capacity(value.len());
    let mut cursor = 0;

    while let Some(relative_start) = value[cursor..].find(&marker) {
        let start = cursor + relative_start;
        let value_start = start + marker.len();
        let value_end = value[value_start..]
            .char_indices()
            // Credentials may legitimately contain URL/sub-delimiter
            // punctuation. Stop only at quoting or whitespace boundaries.
            .find(|(_, ch)| ch.is_whitespace() || matches!(ch, '\'' | '"' | '`'))
            .map_or(value.len(), |(index, _)| value_start + index);
        rendered.push_str(&value[cursor..value_start]);
        rendered.push_str(REDACTED_SECRET_VALUE);
        cursor = value_end;
    }

    rendered.push_str(&value[cursor..]);
    rendered
}

#[cfg(test)]
mod tests;
