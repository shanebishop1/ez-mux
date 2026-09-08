use std::path::PathBuf;

use crate::config::{RuntimeContext, SessionRuntimeContext};

use super::super::TmuxClient;
use super::{
    SessionAction, SessionError, SessionIdentity, SessionLaunchOutcome, StartupTrace,
    TeardownOwnership, shared_server_password, should_open_auxiliary_synchronously,
    spawn_auxiliary_viewer_open,
};

pub(super) struct BootstrapRequest<'a> {
    pub(super) identity: SessionIdentity,
    pub(super) session_context: &'a SessionRuntimeContext,
    pub(super) runtime_context: &'a RuntimeContext,
    pub(super) remote_project_dir: PathBuf,
    pub(super) remote_routing_active: bool,
    pub(super) action: SessionAction,
    pub(super) created_session: bool,
    pub(super) pane_count: u8,
    pub(super) no_worktrees: bool,
    pub(super) ownership: &'a mut TeardownOwnership,
}

pub(super) fn run(
    request: BootstrapRequest<'_>,
    tmux: &impl TmuxClient,
    trace: &mut StartupTrace,
) -> Result<SessionLaunchOutcome, SessionError> {
    let BootstrapRequest {
        identity,
        session_context,
        runtime_context,
        remote_project_dir,
        remote_routing_active,
        action,
        created_session,
        pane_count,
        no_worktrees,
        ownership,
    } = request;

    if created_session {
        for helper_session in TeardownOwnership::bootstrap_helper_sessions(&identity.session_name) {
            if !tmux.session_exists(&helper_session)? {
                ownership.add_helper_session(helper_session);
            }
        }
        tmux.reconcile_session_runtime_auth(
            &identity.session_name,
            shared_server_password(runtime_context),
        )?;
        tmux.reconcile_session_runtime_context(&identity.session_name, session_context)?;
        tmux.bootstrap_default_layout(
            &identity.session_name,
            &identity.project_dir,
            pane_count,
            no_worktrees,
        )?;
        trace.mark("tmux-bootstrap-default-layout");
    } else {
        if shared_server_password(runtime_context).is_some()
            && runtime_context.remote.shared_server.url.value.as_deref()
                == session_context.shared_server_url.as_deref()
        {
            tmux.reconcile_session_runtime_auth(
                &identity.session_name,
                shared_server_password(runtime_context),
            )?;
        }
        tmux.reconcile_session_runtime_context(&identity.session_name, session_context)?;
        tmux.validate_session_invariants(&identity.session_name)?;
        trace.mark("tmux-validate-invariants");
    }

    if should_open_auxiliary_synchronously() {
        tmux.auxiliary_viewer_with_runtime_context(&identity.session_name, true, session_context)?;
        trace.mark("tmux-auxiliary-viewer-sync-non-interactive");
    } else if let Err(source) = spawn_auxiliary_viewer_open(&identity.session_name) {
        eprintln!(
            "warning: failed scheduling auxiliary viewer open in background; falling back to synchronous open: {source}"
        );
        tmux.auxiliary_viewer_with_runtime_context(&identity.session_name, true, session_context)?;
        trace.mark("tmux-auxiliary-viewer-sync-fallback");
    } else {
        trace.mark("tmux-auxiliary-viewer-scheduled");
    }
    trace.emit_pre_attach_summary(&identity.session_name, action.label());
    tmux.attach_session(&identity.session_name)?;

    Ok(SessionLaunchOutcome {
        identity,
        remote_project_dir,
        remote_routing_active,
        action,
    })
}

pub(super) fn rollback_created_session(
    tmux: &impl TmuxClient,
    session_name: &str,
    ownership: &TeardownOwnership,
    bootstrap_error: SessionError,
) -> SessionError {
    match tmux.teardown_owned_session(session_name, ownership) {
        Ok(_) => bootstrap_error,
        Err(cleanup_error) => SessionError::TmuxCommandFailed {
            command: format!("rollback newly created session {session_name}"),
            stderr: format!("bootstrap failed: {bootstrap_error}; cleanup failed: {cleanup_error}"),
        },
    }
}
