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
    args.windows(2)
        .filter(|window| window[0] == OPENCODE_SERVER_PASSWORD_ENV)
        .map(|window| window[1].clone())
        .filter(|value| !value.is_empty())
        .collect()
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
    let mut rendered = redact_embedded_authorities(value);
    rendered = redact_named_secret_values(&rendered);
    for secret in secret_values.iter().filter(|secret| !secret.is_empty()) {
        rendered = rendered.replace(secret, REDACTED_SECRET_VALUE);
    }
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
mod tests {
    use super::{
        REDACTED_SECRET_VALUE, format_output_diagnostics_with_args,
        legacy_window_zero_session_target, parse_primary_window_target, render_startup_trace,
        tmux_batch_command_for_diagnostics, tmux_command_for_diagnostics,
    };

    #[test]
    fn parse_primary_window_target_prefers_active_window_id() {
        let output = "0|@77\n1|@92\n";
        assert_eq!(
            parse_primary_window_target(output),
            Some(String::from("@92"))
        );
    }

    #[test]
    fn parse_primary_window_target_falls_back_to_first_window_id() {
        let output = "0|@77\n0|@92\n";
        assert_eq!(
            parse_primary_window_target(output),
            Some(String::from("@77"))
        );
    }

    #[test]
    fn legacy_window_zero_session_target_detects_list_panes_zero_window_failure() {
        let args = ["list-panes", "-t", "ezm-demo:0", "-F", "#{pane_id}"];
        let stderr = "can't find window: 0";
        assert_eq!(
            legacy_window_zero_session_target(&args, stderr),
            Some((2, "ezm-demo"))
        );
    }

    #[test]
    fn legacy_window_zero_session_target_ignores_non_matching_failures() {
        let args = ["list-panes", "-t", "ezm-demo:2", "-F", "#{pane_id}"];
        let stderr = "can't find window: 2";
        assert_eq!(legacy_window_zero_session_target(&args, stderr), None);
    }

    #[test]
    fn tmux_command_for_diagnostics_redacts_password_set_environment_value_for_global_sync() {
        let rendered = tmux_command_for_diagnostics(&[
            "set-environment",
            "-g",
            "OPENCODE_SERVER_PASSWORD",
            "super-secret",
        ]);

        assert!(!rendered.contains("super-secret"));
        assert!(rendered.contains(REDACTED_SECRET_VALUE));
    }

    #[test]
    fn tmux_command_for_diagnostics_redacts_password_set_environment_value_for_targeted_sync() {
        let rendered = tmux_command_for_diagnostics(&[
            "set-environment",
            "-t",
            "ezm-s42",
            "OPENCODE_SERVER_PASSWORD",
            "another-secret",
        ]);

        assert!(!rendered.contains("another-secret"));
        assert!(rendered.contains(REDACTED_SECRET_VALUE));
    }

    #[test]
    fn tmux_command_for_diagnostics_does_not_inject_redaction_for_unset_without_value() {
        let rendered =
            tmux_command_for_diagnostics(&["set-environment", "-gu", "OPENCODE_SERVER_PASSWORD"]);

        assert_eq!(rendered, "set-environment -gu OPENCODE_SERVER_PASSWORD");
    }

    #[test]
    fn tmux_command_for_diagnostics_keeps_non_secret_environment_values_visible() {
        let rendered = tmux_command_for_diagnostics(&[
            "set-environment",
            "-g",
            "EZM_REMOTE_PATH",
            "/srv/remotes",
        ]);

        assert_eq!(rendered, "set-environment -g EZM_REMOTE_PATH /srv/remotes");
    }

    #[test]
    fn tmux_batch_command_for_diagnostics_redacts_password_values() {
        let commands = vec![
            vec![
                String::from("set-environment"),
                String::from("-g"),
                String::from("OPENCODE_SERVER_PASSWORD"),
                String::from("super-secret"),
            ],
            vec![
                String::from("set-option"),
                String::from("-t"),
                String::from("demo"),
                String::from("@foo"),
                String::from("bar"),
            ],
        ];

        let rendered = tmux_batch_command_for_diagnostics(&commands);
        assert!(!rendered.contains("super-secret"));
        assert!(rendered.contains(REDACTED_SECRET_VALUE));
        assert!(rendered.contains("set-option -t demo @foo bar"));
    }

    #[test]
    fn command_diagnostics_redact_url_userinfo_and_password_streams() {
        let secret = "unique-b2-command-secret";
        let output = std::process::Command::new("sh")
            .args([
                "-c",
                &format!(
                    "printf 'url=https://operator:{secret}@remote.example:7443/path\\n'; printf 'OPENCODE_SERVER_PASSWORD={secret}\\n' >&2"
                ),
            ])
            .output()
            .expect("shell should emit fixture diagnostics");
        let args = ["set-environment", "-g", "OPENCODE_SERVER_PASSWORD", secret];

        let rendered = format_output_diagnostics_with_args(&output, &args);

        assert!(!rendered.contains(secret));
        assert!(rendered.contains("operator:<redacted>@remote.example:7443/path"));
        assert!(rendered.contains("OPENCODE_SERVER_PASSWORD=<redacted>"));
    }

    #[test]
    fn command_diagnostics_keep_executable_data_separate_from_redacted_rendering() {
        let secret = "unique-b2-executable-secret";
        let executable = vec![
            String::from("set-environment"),
            String::from("-g"),
            String::from("OPENCODE_SERVER_PASSWORD"),
            secret.to_owned(),
        ];
        let rendered = super::tmux_batch_command_for_diagnostics(std::slice::from_ref(&executable));

        assert_eq!(executable[3], secret);
        assert!(!rendered.contains(secret));
        assert!(rendered.contains(REDACTED_SECRET_VALUE));
    }

    #[test]
    fn startup_trace_renders_only_the_redacted_command_boundary() {
        let secret = "unique-b2-startup-trace-secret";
        let command = super::tmux_command_for_diagnostics(&[
            "display-popup",
            &format!("https://operator:{secret}@remote.example:7443/path"),
        ]);
        let trace = render_startup_trace(&command, "1", std::time::Duration::from_millis(4));

        assert!(!trace.contains(secret));
        assert!(trace.contains("operator:<redacted>@remote.example:7443/path"));
    }

    #[test]
    fn startup_trace_redacts_url_userinfo_containing_sub_delimiters() {
        for punctuation in [';', ',', '(', ')'] {
            let sentinel = format!("trace{punctuation}credential");
            let command = super::tmux_command_for_diagnostics(&[
                "display-popup",
                &format!("https://operator:{sentinel}@remote.example:7443/path"),
            ]);
            let trace = render_startup_trace(&command, "1", std::time::Duration::from_millis(4));

            assert!(!trace.contains(&sentinel));
            assert!(trace.contains("operator:<redacted>@remote.example:7443/path"));
        }
    }

    #[test]
    fn output_diagnostics_redact_named_passwords_with_sub_delimiters() {
        for punctuation in [';', ',', '(', ')'] {
            let sentinel = format!("named{punctuation}credential");
            let output = std::process::Command::new("sh")
                .args([
                    "-c",
                    &format!("printf '%s\\n' 'OPENCODE_SERVER_PASSWORD={sentinel}' >&2"),
                ])
                .output()
                .expect("shell should emit fixture diagnostics");

            let rendered = super::format_output_diagnostics(&output);

            assert!(!rendered.contains(&sentinel));
            assert!(rendered.contains("OPENCODE_SERVER_PASSWORD=<redacted>"));
        }
    }
}
