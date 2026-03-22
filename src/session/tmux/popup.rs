use super::SessionError;
use super::command::{tmux_output, tmux_output_value, tmux_run};
use super::options::{required_session_option, set_session_option};
use super::slot_swap::validate_canonical_slot_registry;
use super::style::refresh_active_border_for_slot;
use crate::session::{PopupShellAction, PopupShellOutcome};

const POPUP_WIDTH_PCT: u8 = 70;
const POPUP_HEIGHT_PCT: u8 = 70;

pub(super) fn toggle_popup_shell(
    session_name: &str,
    slot_id: u8,
    client_tty: Option<&str>,
) -> Result<PopupShellOutcome, SessionError> {
    validate_canonical_slot_registry(session_name)?;
    ensure_popup_cleanup_hook()?;

    let origin_slot_pane =
        required_session_option(session_name, &format!("@ezm_slot_{slot_id}_pane"))?;
    let cwd = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_cwd"))?;
    let popup_session = popup_session_name(session_name, slot_id);

    if session_exists(&popup_session)? {
        tmux_run(&["kill-session", "-t", &popup_session])?;
        let _ = refresh_active_border_for_slot(session_name, slot_id);
        let _ = tmux_run(&["select-pane", "-t", &origin_slot_pane]);
        persist_popup_defaults(session_name)?;
        return Ok(PopupShellOutcome {
            session_name: session_name.to_owned(),
            slot_id,
            action: PopupShellAction::Closed,
            cwd,
            width_pct: POPUP_WIDTH_PCT,
            height_pct: POPUP_HEIGHT_PCT,
        });
    }

    let create_args = popup_new_session_args(&popup_session, &cwd);
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
    show_popup(&origin_slot_pane, &popup_session, &cwd, client_tty)?;
    enable_popup_session_auto_destroy(&popup_session)?;

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

fn ensure_popup_cleanup_hook() -> Result<(), SessionError> {
    let hook_script = popup_cleanup_hook_command();
    let hook_command = format!("run-shell -b {}", shell_single_quote(&hook_script));
    tmux_run(&["set-hook", "-g", "session-closed", &hook_command])
}

fn popup_cleanup_hook_command() -> String {
    (1_u8..=5)
        .map(|slot_id| {
            format!(
                "tmux kill-session -t \"#{{hook_session_name}}__popup_slot_{slot_id}\" >/dev/null 2>&1"
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
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

fn enable_popup_session_auto_destroy(popup_session: &str) -> Result<(), SessionError> {
    let args = popup_destroy_unattached_args(popup_session);
    let args_ref = args.iter().map(String::as_str).collect::<Vec<_>>();
    tmux_run(&args_ref)
}

fn popup_destroy_unattached_args(popup_session: &str) -> Vec<String> {
    vec![
        String::from("set-option"),
        String::from("-t"),
        popup_session.to_owned(),
        String::from("destroy-unattached"),
        String::from("on"),
    ]
}

fn popup_new_session_args(popup_session: &str, cwd: &str) -> Vec<String> {
    vec![
        String::from("new-session"),
        String::from("-d"),
        String::from("-s"),
        popup_session.to_owned(),
        String::from("-c"),
        cwd.to_owned(),
    ]
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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
        popup_attach_command, popup_cleanup_hook_command, popup_destroy_unattached_args,
        popup_display_args, popup_new_session_args,
    };

    #[test]
    fn popup_attach_command_targets_popup_helper_session() {
        let command = popup_attach_command("ezm-s100__popup_slot_2");
        assert_eq!(command, "tmux attach-session -t 'ezm-s100__popup_slot_2'");
    }

    #[test]
    fn popup_cleanup_hook_command_targets_all_popup_helper_slots() {
        let command = popup_cleanup_hook_command();

        assert!(command.contains("#{hook_session_name}__popup_slot_1"));
        assert!(command.contains("#{hook_session_name}__popup_slot_2"));
        assert!(command.contains("#{hook_session_name}__popup_slot_3"));
        assert!(command.contains("#{hook_session_name}__popup_slot_4"));
        assert!(command.contains("#{hook_session_name}__popup_slot_5"));
        assert!(command.contains(">/dev/null 2>&1"));
        assert!(!command.contains("-t '#{hook_session_name}"));
        assert!(command.contains("-t \"#{hook_session_name}__popup_slot_1\""));
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
    fn popup_helper_sessions_enable_destroy_unattached() {
        let args = popup_destroy_unattached_args("ezm-s100__popup_slot_4");
        assert_eq!(
            args,
            vec![
                String::from("set-option"),
                String::from("-t"),
                String::from("ezm-s100__popup_slot_4"),
                String::from("destroy-unattached"),
                String::from("on"),
            ]
        );
    }

    #[test]
    fn popup_new_session_uses_default_shell_without_lc_wrapper() {
        let args = popup_new_session_args("ezm-s100__popup_slot_4", "/tmp/popup-cwd");
        let rendered = args.join(" ");

        assert!(rendered.starts_with("new-session -d -s ezm-s100__popup_slot_4 -c /tmp/popup-cwd"));
        assert!(!rendered.contains("sh -lc"));
    }
}
