use std::io::IsTerminal;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

mod bootstrap;

use super::SessionError;
use super::SessionIdentity;
use super::TeardownOwnership;
use super::TmuxClient;
use super::binary_hint::{binary_hint_looks_like_single_executable, normalize_shell_binary_hint};
use super::resolve_remote_path;
use super::resolve_session_identity;
use crate::config::{self, RuntimeContext};
use crate::config::{EZM_BIN_ENV, EZM_REMOTE_PATH_ENV, EZM_REMOTE_SERVER_URL_ENV};
use crate::config::{EZM_USE_MOSH_ENV, EZM_USE_TSSH_ENV};

pub const DEFAULT_STARTUP_PANE_COUNT: u8 = 5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RemoteTransportFlags {
    pub use_tssh: bool,
    pub use_mosh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    Create,
    Attach,
}

impl SessionAction {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Attach => "attach",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionLaunchOutcome {
    pub identity: SessionIdentity,
    pub remote_project_dir: std::path::PathBuf,
    pub remote_routing_active: bool,
    pub action: SessionAction,
}

/// Ensures a session exists for the current working directory.
///
/// # Errors
/// Returns an error when reading the current directory fails, when session
/// identity resolution fails, or when tmux operations fail.
pub fn ensure_current_project_session(
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    let project_dir = std::env::current_dir().map_err(SessionError::CurrentDir)?;
    ensure_project_session(&project_dir, tmux)
}

/// Ensures a session exists for the provided project directory.
///
/// # Errors
/// Returns an error when session identity resolution fails or any tmux
/// operation needed to create, validate, bootstrap, or attach fails.
pub fn ensure_project_session(
    project_dir: &Path,
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    // This exported convenience API predates the resolved runtime boundary.
    // Keep its shipped contract: callers that use it directly get the remote
    // routing values from the process environment. The CLI uses
    // `ensure_project_session_with_runtime_context` after resolving config,
    // so this compatibility boundary does not create a second startup path.
    let remote_path = std::env::var(EZM_REMOTE_PATH_ENV).ok();
    let remote_server_url = std::env::var(EZM_REMOTE_SERVER_URL_ENV).ok();
    let remote_transport = RemoteTransportFlags {
        use_tssh: std::env::var(EZM_USE_TSSH_ENV)
            .ok()
            .is_some_and(|value| parse_enabled_value(&value)),
        use_mosh: std::env::var(EZM_USE_MOSH_ENV)
            .ok()
            .is_some_and(|value| parse_enabled_value(&value)),
    };
    let runtime_context = runtime_context_for_remote_values(
        remote_path.as_deref(),
        remote_server_url.as_deref(),
        remote_transport,
    );

    ensure_project_session_with_runtime_context(
        project_dir,
        &runtime_context,
        DEFAULT_STARTUP_PANE_COUNT,
        false,
        tmux,
    )
}

/// Ensures a session exists for the provided project directory using an
/// explicit remote remap prefix when supplied.
///
/// # Errors
/// Returns an error when session identity resolution fails or any tmux
/// operation needed to create, validate, bootstrap, or attach fails.
pub fn ensure_project_session_with_remote_path(
    project_dir: &Path,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    remote_transport: RemoteTransportFlags,
    pane_count: u8,
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    ensure_project_session_with_remote_path_and_options(
        project_dir,
        remote_path,
        remote_server_url,
        remote_transport,
        pane_count,
        false,
        tmux,
    )
}

/// Ensures a session exists for the provided project directory using explicit
/// startup options.
///
/// # Errors
/// Returns an error when session identity resolution fails or any tmux
/// operation needed to create, validate, bootstrap, or attach fails.
pub fn ensure_project_session_with_remote_path_and_options(
    project_dir: &Path,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    remote_transport: RemoteTransportFlags,
    pane_count: u8,
    no_worktrees: bool,
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    let runtime_context =
        runtime_context_for_remote_values(remote_path, remote_server_url, remote_transport);
    ensure_project_session_with_runtime_context(
        project_dir,
        &runtime_context,
        pane_count,
        no_worktrees,
        tmux,
    )
}

fn runtime_context_for_remote_values(
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    remote_transport: RemoteTransportFlags,
) -> RuntimeContext {
    RuntimeContext {
        remote: config::RemoteRuntimeResolution {
            remote_path: config::ResolvedValue {
                value: remote_path
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned),
                source: config::ValueSource::Env,
            },
            remote_server_url: config::ResolvedValue {
                value: remote_server_url
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned),
                source: config::ValueSource::Env,
            },
            use_tssh: config::ResolvedValue {
                value: remote_transport.use_tssh,
                source: config::ValueSource::Env,
            },
            use_mosh: config::ResolvedValue {
                value: remote_transport.use_mosh,
                source: config::ValueSource::Env,
            },
            shared_server: config::SharedServerRuntimeResolution {
                url: config::ResolvedValue {
                    value: None,
                    source: config::ValueSource::Default,
                },
                password: config::ResolvedValue {
                    value: None,
                    source: config::ValueSource::Default,
                },
            },
        },
        auxiliary: config::AuxiliaryRuntimeResolution {
            perles_dir: config::ResolvedValue {
                value: None,
                source: config::ValueSource::Default,
            },
            perles_db: config::ResolvedValue {
                value: None,
                source: config::ValueSource::Default,
            },
        },
        agent_command: None,
        opencode_theme: config::resolve_opencode_theme_runtime(&config::FileConfig::default()),
    }
}

/// Ensures a project session using the already-resolved runtime boundary.
///
/// The context is reconciled after resolving the session identity and before
/// any background or mode launch. Existing scoped context is retained; a
/// missing marker is initialized once for migration from pre-C sessions.
///
/// # Errors
/// Returns an error when session identity resolution, context reconciliation,
/// layout/bootstrap, or attach operations fail.
pub fn ensure_project_session_with_runtime_context(
    project_dir: &Path,
    runtime_context: &RuntimeContext,
    pane_count: u8,
    no_worktrees: bool,
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    let mut trace = StartupTrace::begin();
    let identity = resolve_session_identity(project_dir)?;
    let session_name = identity.session_name.clone();
    trace.mark("resolve-session-identity");
    let session_exists = tmux.session_exists(&identity.session_name)?;
    let requested_session_context = runtime_context.session_context();
    let session_context = if session_exists {
        tmux.resolve_session_runtime_context(&identity.session_name, &requested_session_context)?
    } else {
        requested_session_context
    };
    let remote_path = session_context.remote_path.as_deref();
    let remote_server_url = session_context.remote_server_url.as_deref();
    let remote_routing_active = remote_path
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
        && remote_server_url
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
    trace.mark("resolve-remote-routing-active");
    let resolved_remote_path = resolve_remote_path(
        &identity.project_dir,
        if remote_routing_active {
            remote_path
        } else {
            None
        },
    )?;
    trace.mark("resolve-remote-path");
    let remote_project_dir = resolved_remote_path.effective_path;
    let created_session = !session_exists;
    let mut ownership = TeardownOwnership::for_new_session(&session_name);
    let action = if created_session {
        trace.mark("tmux-session-missing");
        tmux.create_detached_session(&identity.session_name, &identity.project_dir)?;
        trace.mark("tmux-create-detached-session");
        SessionAction::Create
    } else {
        trace.mark("tmux-session-exists");
        SessionAction::Attach
    };

    // Everything after a successful create is part of this invocation's
    // bootstrap. If it fails, rollback is allowed to remove only the session
    // this invocation created.
    let startup_result = bootstrap::run(
        bootstrap::BootstrapRequest {
            identity,
            session_context: &session_context,
            runtime_context,
            remote_project_dir,
            remote_routing_active: resolved_remote_path.remapped,
            action,
            created_session,
            pane_count,
            no_worktrees,
            ownership: &mut ownership,
        },
        tmux,
        &mut trace,
    );

    if created_session {
        startup_result.map_err(|bootstrap_error| {
            bootstrap::rollback_created_session(tmux, &session_name, &ownership, bootstrap_error)
        })
    } else {
        startup_result
    }
}

fn shared_server_password(runtime_context: &RuntimeContext) -> Option<&str> {
    runtime_context
        .remote
        .shared_server
        .url
        .value
        .as_deref()
        .filter(|value| !value.trim().is_empty())?;
    runtime_context
        .remote
        .shared_server
        .password
        .value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

const STARTUP_TRACE_ENV: &str = "EZM_STARTUP_TRACE";

#[derive(Debug, Clone)]
struct StartupTraceStep {
    label: &'static str,
    elapsed_since_start: Duration,
    elapsed_since_last: Duration,
}

struct StartupTrace {
    enabled: bool,
    started_at: Instant,
    last_mark: Instant,
    steps: Vec<StartupTraceStep>,
}

impl StartupTrace {
    fn begin() -> Self {
        let enabled = startup_trace_enabled();
        let now = Instant::now();
        Self {
            enabled,
            started_at: now,
            last_mark: now,
            steps: Vec::new(),
        }
    }

    fn mark(&mut self, label: &'static str) {
        if !self.enabled {
            return;
        }

        let now = Instant::now();
        self.steps.push(StartupTraceStep {
            label,
            elapsed_since_start: now.saturating_duration_since(self.started_at),
            elapsed_since_last: now.saturating_duration_since(self.last_mark),
        });
        self.last_mark = now;
    }

    fn emit_pre_attach_summary(&self, session_name: &str, action_label: &str) {
        if !self.enabled {
            return;
        }

        eprintln!(
            "startup-trace summary phase=pre-attach session={session_name} action={action_label} total_ms={:.2}",
            millis(self.last_mark.saturating_duration_since(self.started_at))
        );

        for step in &self.steps {
            eprintln!(
                "startup-trace step={} delta_ms={:.2} total_ms={:.2}",
                step.label,
                millis(step.elapsed_since_last),
                millis(step.elapsed_since_start)
            );
        }
    }
}

fn startup_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var(STARTUP_TRACE_ENV)
            .ok()
            .is_some_and(|value| parse_enabled_value(&value))
    })
}

fn parse_enabled_value(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
}

fn should_open_auxiliary_synchronously() -> bool {
    !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal()
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn spawn_auxiliary_viewer_open(session_name: &str) -> Result<(), std::io::Error> {
    let binary = resolve_ezm_binary_for_internal_command();
    Command::new(binary)
        .arg("__internal")
        .arg("auxiliary")
        .arg("--session")
        .arg(session_name)
        .arg("--action")
        .arg("open")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

fn resolve_ezm_binary_for_internal_command() -> PathBuf {
    std::env::var(EZM_BIN_ENV)
        .ok()
        .and_then(|value| normalize_shell_binary_hint(&value))
        .filter(|candidate| binary_hint_looks_like_single_executable(candidate))
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("ezm"))
}

#[cfg(test)]
mod tests {
    use crate::session::binary_hint::{
        binary_hint_looks_like_single_executable, normalize_shell_binary_hint,
    };

    use super::parse_enabled_value;

    #[test]
    fn recognizes_common_enabled_values() {
        for value in ["1", "true", "TRUE", "yes", "on"] {
            assert!(
                parse_enabled_value(value),
                "expected value `{value}` to be enabled"
            );
        }
    }

    #[test]
    fn rejects_disabled_or_empty_values() {
        for value in ["0", "false", "no", "off", "", "maybe"] {
            assert!(
                !parse_enabled_value(value),
                "expected value `{value}` to be disabled"
            );
        }
    }

    #[test]
    fn normalizes_binary_hint_quotes_and_rejects_multi_token_values() {
        assert_eq!(
            normalize_shell_binary_hint("'/tmp/ezm'"),
            Some(String::from("/tmp/ezm"))
        );
        assert_eq!(
            normalize_shell_binary_hint("\\\"/tmp/ezm\\\""),
            Some(String::from("/tmp/ezm"))
        );
        assert!(binary_hint_looks_like_single_executable("/tmp/ezm"));
        assert!(!binary_hint_looks_like_single_executable(
            "/tmp/ezm __internal focus"
        ));
    }
}
