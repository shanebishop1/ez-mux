use std::process::{Command, Output};

use super::SessionError;

pub(super) fn tmux_output(args: &[&str]) -> Result<Output, SessionError> {
    Command::new("tmux")
        .args(args)
        .output()
        .map_err(|source| SessionError::TmuxSpawnFailed {
            command: args.join(" "),
            source,
        })
}

pub(super) fn tmux_run(args: &[&str]) -> Result<(), SessionError> {
    let output = tmux_output(args)?;
    if output.status.success() {
        return Ok(());
    }

    Err(SessionError::TmuxCommandFailed {
        command: args.join(" "),
        stderr: format_output_diagnostics(&output),
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
        command: args.join(" "),
        stderr: format_output_diagnostics(&output),
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
        command: retry_args.join(" "),
        stderr: format_output_diagnostics(&retry_output),
    })
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

pub(super) fn format_output_diagnostics(output: &Output) -> String {
    let status = output
        .status
        .code()
        .map_or_else(|| String::from("signal"), |code| code.to_string());
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();

    format!("status={status}; stdout={stdout:?}; stderr={stderr:?}")
}

#[cfg(test)]
mod tests {
    use super::{legacy_window_zero_session_target, parse_primary_window_target};

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
}
