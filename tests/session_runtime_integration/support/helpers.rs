use std::path::Path;

use ez_mux::session::RemoteTransportFlags;
use ez_mux::session::TmuxClient;
use ez_mux::session::ensure_project_session_with_remote_path;
use ez_mux::session::resolve_session_identity;

pub(crate) fn ensure_local_project_session(
    project_dir: &Path,
    tmux: &impl TmuxClient,
) -> Result<ez_mux::session::SessionLaunchOutcome, ez_mux::session::SessionError> {
    ensure_project_session_with_remote_path(
        project_dir,
        None,
        None,
        RemoteTransportFlags::default(),
        5,
        tmux,
    )
}

pub(crate) fn error_session_name(project_dir: &Path) -> String {
    resolve_session_identity(project_dir)
        .expect("resolve session identity")
        .session_name
}

pub(crate) fn test_runtime_context(
    remote_path: &str,
    server: &str,
) -> ez_mux::config::RuntimeContext {
    let mut context = ez_mux::config::RuntimeContext::default();
    context.remote.remote_path.value = Some(remote_path.to_owned());
    context.remote.remote_server_url.value = Some(server.to_owned());
    context.remote.shared_server.url.value = Some(format!("http://{server}:4096"));
    let password_suffix = server.chars().next().unwrap();
    context.remote.shared_server.password.value = Some(format!("password-{password_suffix}"));
    context
}
