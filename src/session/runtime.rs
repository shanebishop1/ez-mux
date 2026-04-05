use std::io::IsTerminal;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use super::SessionError;
use super::SessionIdentity;
use super::TmuxClient;
use super::resolve_remote_path;
use super::resolve_session_identity;
use crate::config::EZM_BIN_ENV;
use crate::config::{EZM_REMOTE_PATH_ENV, EZM_REMOTE_SERVER_URL_ENV};

pub const DEFAULT_STARTUP_PANE_COUNT: u8 = 5;

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
    let remote_path = std::env::var(EZM_REMOTE_PATH_ENV).ok();
    let remote_server_url = std::env::var(EZM_REMOTE_SERVER_URL_ENV).ok();

    ensure_project_session_with_remote_path(
        project_dir,
        remote_path.as_deref(),
        remote_server_url.as_deref(),
        DEFAULT_STARTUP_PANE_COUNT,
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
    pane_count: u8,
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    ensure_project_session_with_remote_path_and_options(
        project_dir,
        remote_path,
        remote_server_url,
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
    pane_count: u8,
    no_worktrees: bool,
    tmux: &impl TmuxClient,
) -> Result<SessionLaunchOutcome, SessionError> {
    let mut trace = StartupTrace::begin();
    let identity = resolve_session_identity(project_dir)?;
    trace.mark("resolve-session-identity");
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
    let action = if tmux.session_exists(&identity.session_name)? {
        trace.mark("tmux-session-exists");
        tmux.validate_session_invariants(&identity.session_name)?;
        trace.mark("tmux-validate-invariants");
        SessionAction::Attach
    } else {
        trace.mark("tmux-session-missing");
        tmux.create_detached_session(&identity.session_name, &identity.project_dir)?;
        trace.mark("tmux-create-detached-session");
        tmux.bootstrap_default_layout(
            &identity.session_name,
            &identity.project_dir,
            pane_count,
            no_worktrees,
        )?;
        trace.mark("tmux-bootstrap-default-layout");
        SessionAction::Create
    };
    if should_open_auxiliary_synchronously() {
        tmux.auxiliary_viewer(&identity.session_name, true)?;
        trace.mark("tmux-auxiliary-viewer-sync-non-interactive");
    } else if let Err(source) = spawn_auxiliary_viewer_open(&identity.session_name) {
        eprintln!(
            "warning: failed scheduling auxiliary viewer open in background; falling back to synchronous open: {source}"
        );
        tmux.auxiliary_viewer(&identity.session_name, true)?;
        trace.mark("tmux-auxiliary-viewer-sync-fallback");
    } else {
        trace.mark("tmux-auxiliary-viewer-scheduled");
    }
    trace.emit_pre_attach_summary(&identity.session_name, action.label());
    tmux.attach_session(&identity.session_name)?;

    Ok(SessionLaunchOutcome {
        identity,
        remote_project_dir,
        remote_routing_active: resolved_remote_path.remapped,
        action,
    })
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

fn normalize_shell_binary_hint(value: &str) -> Option<String> {
    let mut normalized = value.trim();

    loop {
        let previous = normalized;
        normalized = strip_quote_like_prefix(normalized);
        normalized = strip_quote_like_suffix(normalized);
        normalized = normalized.trim();
        if normalized == previous {
            break;
        }
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_owned())
    }
}

fn binary_hint_looks_like_single_executable(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '\'' | '"' | '\0'))
}

fn strip_quote_like_prefix(value: &str) -> &str {
    if let Some(stripped) = value.strip_prefix("\\\"") {
        return stripped;
    }

    if let Some(stripped) = value.strip_prefix("\\'") {
        return stripped;
    }

    if let Some(stripped) = value.strip_prefix('"') {
        return stripped;
    }

    if let Some(stripped) = value.strip_prefix('\'') {
        return stripped;
    }

    value
}

fn strip_quote_like_suffix(value: &str) -> &str {
    if let Some(stripped) = value.strip_suffix("\\\"") {
        return stripped;
    }

    if let Some(stripped) = value.strip_suffix("\\'") {
        return stripped;
    }

    if let Some(stripped) = value.strip_suffix('"') {
        return stripped;
    }

    if let Some(stripped) = value.strip_suffix('\'') {
        return stripped;
    }

    value
}

#[cfg(test)]
mod tests {
    use super::{
        binary_hint_looks_like_single_executable, normalize_shell_binary_hint, parse_enabled_value,
    };

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
