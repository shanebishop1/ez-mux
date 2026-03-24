use super::SessionError;
use super::command::{tmux_output, tmux_output_value, tmux_run};
use super::options::{required_session_option, set_session_option};
use super::slot_swap::validate_canonical_slot_registry;
use crate::config::EZM_REMOTE_SERVER_URL_ENV;
use crate::session::{PopupShellAction, PopupShellOutcome, resolve_remote_path};

const POPUP_WIDTH_PCT: u8 = 70;
const POPUP_HEIGHT_PCT: u8 = 70;
const POPUP_PARENT_CLEANUP_HOOK_MARKER: &str = "EZM_POPUP_PARENT_CLEANUP_V2";
const POPUP_PARENT_CLEANUP_LEGACY_INTERNAL_MARKER: &str = "__internal popup-parent-closed";
const EZM_REMOTE_DIR_ENV: &str = "EZM_REMOTE_DIR";

pub(super) fn toggle_popup_shell(
    session_name: &str,
    slot_id: u8,
    client_tty: Option<&str>,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
) -> Result<PopupShellOutcome, SessionError> {
    validate_canonical_slot_registry(session_name)?;
    reconcile_popup_parent_cleanup_hook()?;

    let origin_slot_pane =
        required_session_option(session_name, &format!("@ezm_slot_{slot_id}_pane"))?;
    let cwd = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_cwd"))?;
    let remote_context = resolve_popup_remote_context(&cwd, remote_path, remote_server_url)?;
    let popup_session = popup_session_name(session_name, slot_id);

    if session_exists(&popup_session)? {
        if popup_visible_for_client(client_tty)? {
            close_popup(client_tty)?;
            validate_canonical_slot_registry(session_name)?;
            return Ok(PopupShellOutcome {
                session_name: session_name.to_owned(),
                slot_id,
                action: PopupShellAction::Closed,
                cwd,
                width_pct: POPUP_WIDTH_PCT,
                height_pct: POPUP_HEIGHT_PCT,
            });
        }

        set_session_option(&popup_session, "@ezm_popup_origin_pane", &origin_slot_pane)?;
        set_session_option(&popup_session, "@ezm_popup_cwd", &cwd)?;
        apply_popup_remote_context_environment(&popup_session, remote_context.as_ref())?;
        disable_popup_session_auto_destroy(&popup_session)?;
        show_popup(&origin_slot_pane, &popup_session, &cwd, client_tty)?;

        validate_canonical_slot_registry(session_name)?;
        return Ok(PopupShellOutcome {
            session_name: session_name.to_owned(),
            slot_id,
            action: PopupShellAction::Opened,
            cwd,
            width_pct: POPUP_WIDTH_PCT,
            height_pct: POPUP_HEIGHT_PCT,
        });
    }

    let create_args = popup_new_session_args(&popup_session, &cwd, remote_context.as_ref());
    let create_args_ref = create_args.iter().map(String::as_str).collect::<Vec<_>>();
    tmux_run(&create_args_ref)?;

    persist_popup_defaults(session_name)?;
    set_session_option(&popup_session, "@ezm_popup_origin_session", session_name)?;
    set_session_option(
        &popup_session,
        "@ezm_popup_origin_slot",
        &slot_id.to_string(),
    )?;
    set_session_option(&popup_session, "@ezm_popup_origin_pane", &origin_slot_pane)?;
    set_session_option(&popup_session, "@ezm_popup_cwd", &cwd)?;
    apply_popup_remote_context_environment(&popup_session, remote_context.as_ref())?;
    disable_popup_session_auto_destroy(&popup_session)?;
    show_popup(&origin_slot_pane, &popup_session, &cwd, client_tty)?;

    validate_canonical_slot_registry(session_name)?;
    Ok(PopupShellOutcome {
        session_name: session_name.to_owned(),
        slot_id,
        action: PopupShellAction::Opened,
        cwd,
        width_pct: POPUP_WIDTH_PCT,
        height_pct: POPUP_HEIGHT_PCT,
    })
}

fn popup_session_name(session_name: &str, slot_id: u8) -> String {
    format!("{session_name}__popup_slot_{slot_id}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PopupRemoteContext {
    remote_dir: String,
    remote_server_url: Option<String>,
}

fn resolve_popup_remote_context(
    cwd: &str,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
) -> Result<Option<PopupRemoteContext>, SessionError> {
    let resolved = resolve_remote_path(std::path::Path::new(cwd), remote_path)?;
    if !resolved.remapped {
        return Ok(None);
    }

    Ok(Some(PopupRemoteContext {
        remote_dir: resolved.effective_path.display().to_string(),
        remote_server_url: remote_server_url
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
    }))
}

fn apply_popup_remote_context_environment(
    popup_session: &str,
    remote_context: Option<&PopupRemoteContext>,
) -> Result<(), SessionError> {
    if let Some(context) = remote_context {
        set_session_environment(popup_session, EZM_REMOTE_DIR_ENV, &context.remote_dir)?;

        if let Some(server_url) = context.remote_server_url.as_deref() {
            set_session_environment(popup_session, EZM_REMOTE_SERVER_URL_ENV, server_url)?;
        } else {
            unset_session_environment(popup_session, EZM_REMOTE_SERVER_URL_ENV)?;
        }
    } else {
        unset_session_environment(popup_session, EZM_REMOTE_DIR_ENV)?;
        unset_session_environment(popup_session, EZM_REMOTE_SERVER_URL_ENV)?;
    }

    Ok(())
}

fn set_session_environment(session_name: &str, key: &str, value: &str) -> Result<(), SessionError> {
    tmux_run(&["set-environment", "-t", session_name, key, value])
}

fn unset_session_environment(session_name: &str, key: &str) -> Result<(), SessionError> {
    tmux_run(&["set-environment", "-t", session_name, "-u", key])
}

pub(super) fn reconcile_popup_parent_cleanup_hook() -> Result<(), SessionError> {
    let hooks = tmux_output_value(&["show-hooks", "-g", "session-closed"])?;
    let parent_cleanup_installed = hooks_contain_popup_parent_cleanup(&hooks);
    for hook_name in popup_cleanup_hook_names(&hooks) {
        tmux_run(&["set-hook", "-gu", &hook_name])?;
    }

    if parent_cleanup_installed {
        return Ok(());
    }

    let hook_command = popup_parent_cleanup_hook_command();
    tmux_run(&["set-hook", "-ag", "session-closed", &hook_command])?;

    Ok(())
}

fn popup_cleanup_hook_names(hooks: &str) -> Vec<String> {
    hooks
        .lines()
        .filter(|line| {
            line.contains(POPUP_PARENT_CLEANUP_LEGACY_INTERNAL_MARKER)
                || (line.contains("#{hook_session_name}__popup_slot_")
                    && !line.contains(POPUP_PARENT_CLEANUP_HOOK_MARKER))
        })
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

fn hooks_contain_popup_parent_cleanup(hooks: &str) -> bool {
    hooks.contains(POPUP_PARENT_CLEANUP_HOOK_MARKER)
}

fn popup_parent_cleanup_hook_command() -> String {
    let command = popup_parent_cleanup_script();
    format!("run-shell -b \"{}\"", shell_escape_double_quoted(&command))
}

fn popup_parent_cleanup_script() -> String {
    let mut commands = Vec::with_capacity(6);
    for slot_id in 1_u8..=5 {
        commands.push(format!(
            "tmux has-session -t \"#{{hook_session_name}}__popup_slot_{slot_id}\" 2>/dev/null && tmux kill-session -t \"#{{hook_session_name}}__popup_slot_{slot_id}\" >/dev/null 2>&1"
        ));
    }
    commands.push(format!(": # {POPUP_PARENT_CLEANUP_HOOK_MARKER}"));
    commands.join("; ")
}

fn shell_escape_double_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}

fn show_popup(
    origin_slot_pane: &str,
    popup_session: &str,
    cwd: &str,
    client_tty: Option<&str>,
) -> Result<(), SessionError> {
    let attach_command = popup_attach_command(popup_session);
    let args = popup_display_args(origin_slot_pane, cwd, client_tty, &attach_command);

    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    let result = tmux_run(&args_ref);

    if let Err(SessionError::TmuxCommandFailed { stderr, .. }) = &result {
        if stderr.to_ascii_lowercase().contains("no current client") {
            return Ok(());
        }
    }

    result
}

fn close_popup(client_tty: Option<&str>) -> Result<(), SessionError> {
    let args = popup_close_args(client_tty);
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    let result = tmux_run(&args_ref);

    if let Err(SessionError::TmuxCommandFailed { stderr, .. }) = &result {
        if stderr.to_ascii_lowercase().contains("no current client") {
            return Ok(());
        }
    }

    result
}

fn popup_close_args(client_tty: Option<&str>) -> Vec<String> {
    let mut args = vec![String::from("display-popup")];
    if let Some(client_tty) = client_tty.filter(|tty| !tty.trim().is_empty()) {
        args.push(String::from("-c"));
        args.push(client_tty.to_owned());
    }
    args.push(String::from("-C"));
    args
}

fn popup_visible_for_client(client_tty: Option<&str>) -> Result<bool, SessionError> {
    let args = popup_active_probe_args(client_tty);
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    let output = tmux_output(&args_ref)?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Ok(stdout.trim() == "1");
    }

    let stderr = super::command::format_output_diagnostics(&output);
    if stderr.to_ascii_lowercase().contains("no current client") {
        return Ok(false);
    }

    Err(SessionError::TmuxCommandFailed {
        command: args.join(" "),
        stderr,
    })
}

fn popup_active_probe_args(client_tty: Option<&str>) -> Vec<String> {
    let mut args = vec![String::from("display-message"), String::from("-p")];
    if let Some(client_tty) = client_tty.filter(|tty| !tty.trim().is_empty()) {
        args.push(String::from("-c"));
        args.push(client_tty.to_owned());
    }
    args.push(String::from("#{popup_active}"));
    args
}

fn popup_display_args(
    origin_slot_pane: &str,
    cwd: &str,
    client_tty: Option<&str>,
    attach_command: &str,
) -> Vec<String> {
    let mut args = vec![
        String::from("display-popup"),
        String::from("-t"),
        origin_slot_pane.to_owned(),
        String::from("-w"),
        String::from("70%"),
        String::from("-h"),
        String::from("70%"),
        String::from("-d"),
        cwd.to_owned(),
    ];
    if let Some(client_tty) = client_tty.filter(|tty| !tty.trim().is_empty()) {
        args.push(String::from("-c"));
        args.push(client_tty.to_owned());
    }
    args.push(String::from("-E"));
    args.push(attach_command.to_owned());

    args
}

fn popup_attach_command(popup_session: &str) -> String {
    format!(
        "tmux attach-session -t {}",
        shell_single_quote(popup_session)
    )
}

fn disable_popup_session_auto_destroy(popup_session: &str) -> Result<(), SessionError> {
    let args = popup_persistence_args(popup_session);
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    tmux_run(&args_ref)
}

fn popup_persistence_args(popup_session: &str) -> Vec<String> {
    vec![
        String::from("set-option"),
        String::from("-t"),
        popup_session.to_owned(),
        String::from("destroy-unattached"),
        String::from("off"),
    ]
}

fn popup_new_session_args(
    popup_session: &str,
    cwd: &str,
    remote_context: Option<&PopupRemoteContext>,
) -> Vec<String> {
    let mut args = vec![
        String::from("new-session"),
        String::from("-d"),
        String::from("-s"),
        popup_session.to_owned(),
        String::from("-c"),
        cwd.to_owned(),
    ];

    if let Some(command) = popup_remote_launch_command(remote_context) {
        args.push(command);
    }

    args
}

fn popup_remote_launch_command(remote_context: Option<&PopupRemoteContext>) -> Option<String> {
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

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_escape_single_quoted(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

fn session_exists(session_name: &str) -> Result<bool, SessionError> {
    let output = tmux_output(&["-q", "has-session", "-t", session_name])?;
    if output.status.success() {
        return Ok(true);
    }

    if output.status.code() == Some(1) {
        return Ok(false);
    }

    Err(SessionError::TmuxCommandFailed {
        command: format!("has-session -t {session_name}"),
        stderr: super::command::format_output_diagnostics(&output),
    })
}

fn persist_popup_defaults(session_name: &str) -> Result<(), SessionError> {
    set_session_option(
        session_name,
        "@ezm_popup_width_pct",
        &POPUP_WIDTH_PCT.to_string(),
    )?;
    set_session_option(
        session_name,
        "@ezm_popup_height_pct",
        &POPUP_HEIGHT_PCT.to_string(),
    )?;

    let _ = tmux_output_value(&[
        "set-option",
        "-t",
        session_name,
        "@ezm_popup_geometry",
        "70x70",
    ])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        hooks_contain_popup_parent_cleanup, popup_active_probe_args, popup_attach_command,
        popup_cleanup_hook_names, popup_close_args, popup_display_args, popup_new_session_args,
        popup_parent_cleanup_hook_command, popup_persistence_args, popup_remote_launch_command,
        resolve_popup_remote_context,
    };

    #[test]
    fn popup_attach_command_targets_popup_helper_session() {
        let command = popup_attach_command("ezm-s100__popup_slot_2");
        assert_eq!(command, "tmux attach-session -t 'ezm-s100__popup_slot_2'");
    }

    #[test]
    fn popup_cleanup_hook_names_match_popup_cleanup_entries_only() {
        let hooks = concat!(
            "session-closed[0] run-shell -b \"tmux kill-session -t \\\"#{hook_session_name}__popup_slot_1\\\"\"\n",
            "session-closed[1] display-message keep-me\n",
            "session-closed[2] run-shell -b \"tmux kill-session -t \\\"#{hook_session_name}__popup_slot_5\\\"\"\n"
        );

        assert_eq!(
            popup_cleanup_hook_names(hooks),
            vec![
                String::from("session-closed[0]"),
                String::from("session-closed[2]"),
            ]
        );
    }

    #[test]
    fn popup_parent_cleanup_hook_command_invokes_shell_cleanup_route() {
        let rendered = popup_parent_cleanup_hook_command();
        assert!(rendered.starts_with("run-shell -b \""));
        assert!(
            rendered.contains("tmux has-session -t \\\"#{hook_session_name}__popup_slot_1\\\"")
        );
        assert!(
            rendered.contains("tmux kill-session -t \\\"#{hook_session_name}__popup_slot_5\\\"")
        );
        assert!(rendered.contains("EZM_POPUP_PARENT_CLEANUP_V2"));
        assert!(rendered.ends_with('"'));
        assert!(!rendered.contains("'\"'\"'"));
    }

    #[test]
    fn popup_parent_cleanup_hook_detection_uses_script_marker() {
        let hooks = "session-closed[0] run-shell -b \"tmux has-session -t \\\"#{hook_session_name}__popup_slot_1\\\"; : # EZM_POPUP_PARENT_CLEANUP_V2\"";
        assert!(hooks_contain_popup_parent_cleanup(hooks));
    }

    #[test]
    fn popup_display_args_target_origin_pane_and_client() {
        let args = popup_display_args(
            "%42",
            "/tmp/popup",
            Some("client-7"),
            "tmux attach-session -t 'ezm-s42__popup_slot_2'",
        );

        let rendered = args.join(" ");
        assert!(rendered.contains("display-popup -t %42"));
        assert!(rendered.contains("-c client-7"));
        assert!(rendered.contains("-d /tmp/popup"));
        assert!(rendered.contains("tmux attach-session -t 'ezm-s42__popup_slot_2'"));
    }

    #[test]
    fn popup_close_args_target_client_when_present() {
        let args = popup_close_args(Some("/dev/pts/88"));
        assert_eq!(
            args,
            vec![
                String::from("display-popup"),
                String::from("-c"),
                String::from("/dev/pts/88"),
                String::from("-C"),
            ]
        );
    }

    #[test]
    fn popup_close_args_omit_client_when_unset() {
        let args = popup_close_args(None);
        assert_eq!(
            args,
            vec![String::from("display-popup"), String::from("-C")]
        );
    }

    #[test]
    fn popup_active_probe_args_include_popup_active_format() {
        let args = popup_active_probe_args(Some("/dev/pts/3"));
        assert_eq!(
            args,
            vec![
                String::from("display-message"),
                String::from("-p"),
                String::from("-c"),
                String::from("/dev/pts/3"),
                String::from("#{popup_active}")
            ]
        );
    }

    #[test]
    fn popup_helper_sessions_disable_destroy_unattached_for_reopen_toggle() {
        let args = popup_persistence_args("ezm-s100__popup_slot_4");
        assert_eq!(
            args,
            vec![
                String::from("set-option"),
                String::from("-t"),
                String::from("ezm-s100__popup_slot_4"),
                String::from("destroy-unattached"),
                String::from("off"),
            ]
        );
    }

    #[test]
    fn popup_new_session_uses_default_shell_without_lc_wrapper() {
        let args = popup_new_session_args("ezm-s100__popup_slot_4", "/tmp/popup-cwd", None);
        let rendered = args.join(" ");

        assert!(rendered.starts_with("new-session -d -s ezm-s100__popup_slot_4 -c /tmp/popup-cwd"));
        assert!(!rendered.contains("sh -lc"));
    }

    #[test]
    fn popup_new_session_uses_remote_ssh_shell_when_remote_routing_is_active() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("alpha");
        let nested = repo_root.join("feature");
        std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
        std::fs::create_dir_all(&nested).expect("create nested");

        let remote_context = resolve_popup_remote_context(
            &nested.display().to_string(),
            Some("/srv/remotes"),
            Some("https://shell.remote.example:7443"),
        )
        .expect("context should resolve");

        let args = popup_new_session_args(
            "ezm-s100__popup_slot_4",
            "/tmp/popup-cwd",
            remote_context.as_ref(),
        );
        let rendered = args.join(" ");

        assert!(rendered.contains("new-session -d -s ezm-s100__popup_slot_4 -c /tmp/popup-cwd"));
        assert!(rendered.contains("sh -lc"));
        assert!(rendered.contains("ssh -tt -p 7443"));
        assert!(rendered.contains("shell.remote.example"));
        assert!(rendered.contains("cd '"));
    }

    #[test]
    fn popup_remote_launch_command_returns_none_without_server_url() {
        let context = super::PopupRemoteContext {
            remote_dir: String::from("/srv/remotes/alpha"),
            remote_server_url: None,
        };

        let command = popup_remote_launch_command(Some(&context));
        assert!(command.is_none());
    }

    #[test]
    fn popup_cleanup_hook_names_ignore_non_popup_cleanup_hooks() {
        let hooks = concat!(
            "session-closed\n",
            "session-closed[0] display-message keep-me\n",
            "pane-died[0] run-shell -b \"echo other\"\n"
        );

        assert!(popup_cleanup_hook_names(hooks).is_empty());
    }

    #[test]
    fn popup_cleanup_hook_names_skip_current_parent_cleanup_hook_entries() {
        let hooks = "session-closed[0] run-shell -b \"tmux has-session -t \\\"#{hook_session_name}__popup_slot_1\\\"; : # EZM_POPUP_PARENT_CLEANUP_V2\"";
        assert!(popup_cleanup_hook_names(hooks).is_empty());
    }

    #[test]
    fn popup_cleanup_hook_names_include_legacy_internal_cleanup_entries() {
        let hooks = "session-closed[2] run-shell -b \"/tmp/ezm __internal popup-parent-closed --session \\\"#{hook_session_name}\\\"\"";
        assert_eq!(
            popup_cleanup_hook_names(hooks),
            vec![String::from("session-closed[2]")]
        );
    }

    #[test]
    fn popup_remote_context_is_none_when_remote_remap_is_inactive() {
        let context =
            resolve_popup_remote_context("/tmp/local", None, None).expect("context should resolve");
        assert!(context.is_none());
    }

    #[test]
    fn popup_remote_context_resolves_when_remote_path_is_active() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("alpha");
        let nested = repo_root.join("feature");
        std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
        std::fs::create_dir_all(&nested).expect("create nested");

        let context =
            resolve_popup_remote_context(&nested.display().to_string(), Some("/srv/remotes"), None)
                .expect("context should resolve")
                .expect("context should be present");

        assert_eq!(
            context.remote_dir,
            String::from("/srv/remotes/alpha/feature")
        );
        assert_eq!(context.remote_server_url, None);
    }

    #[test]
    fn popup_remote_context_includes_optional_server_url_when_configured() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo_root = temp.path().join("alpha");
        let nested = repo_root.join("feature");
        std::fs::create_dir_all(repo_root.join(".git")).expect("create .git");
        std::fs::create_dir_all(&nested).expect("create nested");

        let context = resolve_popup_remote_context(
            &nested.display().to_string(),
            Some("/srv/remotes"),
            Some(" https://shell.remote.example:7443 "),
        )
        .expect("context should resolve")
        .expect("context should be present");

        assert_eq!(
            context.remote_dir,
            String::from("/srv/remotes/alpha/feature")
        );
        assert_eq!(
            context.remote_server_url,
            Some(String::from("https://shell.remote.example:7443"))
        );
    }
}
