use super::SessionError;
use super::command::tmux_run;
use super::options::{required_session_option, set_session_option};
use super::slot_swap::validate_canonical_slot_registry;
use crate::session::{PopupShellAction, PopupShellOutcome};

mod context;
mod display;
mod hooks;
mod remote_ssh;
mod session;

#[cfg(test)]
mod tests;

const POPUP_WIDTH_PCT: u8 = 70;
const POPUP_HEIGHT_PCT: u8 = 70;

pub(super) fn toggle_popup_shell(
    session_name: &str,
    slot_id: u8,
    client_tty: Option<&str>,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    remote_transport: crate::session::RemoteTransportFlags,
) -> Result<PopupShellOutcome, SessionError> {
    validate_canonical_slot_registry(session_name)?;
    reconcile_popup_parent_cleanup_hook()?;

    let origin_slot_pane =
        required_session_option(session_name, &format!("@ezm_slot_{slot_id}_pane"))?;
    let cwd = required_session_option(session_name, &format!("@ezm_slot_{slot_id}_cwd"))?;
    let remote_context = context::resolve_popup_remote_context(
        &cwd,
        remote_path,
        remote_server_url,
        remote_transport.use_tssh,
        remote_transport.use_mosh,
    )?;
    let popup_session = session::popup_session_name(session_name, slot_id);

    if session::session_exists(&popup_session)? {
        if display::popup_visible_for_client(client_tty)? {
            display::close_popup(client_tty)?;
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
        context::apply_popup_remote_context_environment(&popup_session, remote_context.as_ref())?;
        session::disable_popup_session_auto_destroy(&popup_session)?;
        display::show_popup(&origin_slot_pane, &popup_session, &cwd, client_tty)?;

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

    let create_args =
        session::popup_new_session_args(&popup_session, &cwd, remote_context.as_ref())?;
    let create_args_ref = create_args.iter().map(String::as_str).collect::<Vec<_>>();
    tmux_run(&create_args_ref)?;

    session::persist_popup_defaults(session_name)?;
    set_session_option(&popup_session, "@ezm_popup_origin_session", session_name)?;
    set_session_option(
        &popup_session,
        "@ezm_popup_origin_slot",
        &slot_id.to_string(),
    )?;
    set_session_option(&popup_session, "@ezm_popup_origin_pane", &origin_slot_pane)?;
    set_session_option(&popup_session, "@ezm_popup_cwd", &cwd)?;
    context::apply_popup_remote_context_environment(&popup_session, remote_context.as_ref())?;
    session::disable_popup_session_auto_destroy(&popup_session)?;
    display::show_popup(&origin_slot_pane, &popup_session, &cwd, client_tty)?;

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

pub(super) fn reconcile_popup_parent_cleanup_hook() -> Result<(), SessionError> {
    hooks::reconcile_popup_parent_cleanup_hook()
}

pub(super) fn popup_parent_cleanup_hook_install_command() -> Vec<String> {
    hooks::popup_parent_cleanup_hook_install_command()
}
