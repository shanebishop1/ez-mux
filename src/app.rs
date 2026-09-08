use std::io::IsTerminal;
use thiserror::Error;

use crate::cli::{AuxiliaryAction, Cli, Command, InternalCommand, LogsCommand};
use crate::config::{self, ConfigError, OperatingSystem, ValueSource};
use crate::logging::{self, LogOpener, LoggingError};
use crate::session::{self, SessionError};

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Logging(#[from] LoggingError),
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error("{0}")]
    Runtime(String),
    #[error("interrupted")]
    Interrupted,
}

/// Executes the parsed CLI command and returns a success message.
///
/// # Errors
///
/// Returns [`AppError::Config`] when config path resolution or TOML parsing fails,
/// and [`AppError::Logging`] when log-open operations fail.
pub fn execute(
    cli: Cli,
    env: &impl config::EnvProvider,
    os: OperatingSystem,
    active_log_root: &std::path::Path,
) -> Result<String, AppError> {
    execute_with_opener(cli, env, os, active_log_root, &logging::ProcessLogOpener)
}

pub(crate) fn execute_with_opener(
    cli: Cli,
    env: &impl config::EnvProvider,
    os: OperatingSystem,
    active_log_root: &std::path::Path,
    opener: &impl LogOpener,
) -> Result<String, AppError> {
    let Cli {
        verbose,
        panes,
        pane_shortcut,
        no_worktrees,
        command,
        ..
    } = cli;

    let message = match command {
        None => {
            let (pane_count, runtime_context) =
                resolve_launch_settings(env, os, panes.or(pane_shortcut))?;
            validate_resolved_remote_authority(
                runtime_context.remote.remote_server_url.value.as_deref(),
            )?;
            let outcome = execute_default_session_flow(
                &runtime_context,
                pane_count.value,
                no_worktrees,
                &session::ProcessTmuxClient,
            )?;
            let effective_runtime_context = if outcome.action == session::SessionAction::Attach {
                session::resolve_session_runtime_context(
                    &outcome.identity.session_name,
                    &runtime_context,
                )?
            } else {
                runtime_context.clone()
            };
            default_contract_summary_message(
                verbose > 0,
                &outcome,
                &effective_runtime_context.remote,
            )
        }
        Some(Command::Kill) => {
            let outcome = execute_kill_session_flow(&session::ProcessTmuxClient)?;
            format_kill_message(&outcome)
        }
        Some(Command::Repair) => {
            let outcome =
                session::repair_current_project_session_and_attach(&session::ProcessTmuxClient)?;
            format_repair_message(&outcome)
        }
        Some(Command::Logs(LogsCommand::OpenLatest)) => {
            execute_open_latest(active_log_root, os, opener)?
        }
        Some(Command::Preset { preset }) => {
            let (pane_count, runtime_context) =
                resolve_launch_settings(env, os, panes.or(pane_shortcut))?;
            validate_resolved_remote_authority(
                runtime_context.remote.remote_server_url.value.as_deref(),
            )?;
            let tmux = session::ProcessTmuxClient;
            let outcome = execute_default_session_flow(
                &runtime_context,
                pane_count.value,
                no_worktrees,
                &tmux,
            )?;
            let preset_outcome =
                session::apply_layout_preset(&outcome.identity.session_name, preset, &tmux)?;
            format!(
                "preset apply complete: session={}; preset={}",
                preset_outcome.session_name,
                preset_outcome.preset.label()
            )
        }
        Some(Command::Internal { command }) => {
            let loaded = config::load_config(env, os)?;
            let runtime_context = config::resolve_runtime_context(env, &loaded.values)?;
            execute_internal(command, &runtime_context)?
        }
    };

    Ok(message)
}

fn resolve_launch_settings(
    env: &impl config::EnvProvider,
    os: OperatingSystem,
    pane_count: Option<u8>,
) -> Result<(config::ResolvedValue<u8>, config::RuntimeContext), AppError> {
    let loaded = config::load_config(env, os)?;
    Ok((
        config::resolve_pane_count(pane_count, &loaded.values)?,
        config::resolve_runtime_context(env, &loaded.values)?,
    ))
}

fn execute_default_session_flow(
    runtime_context: &config::RuntimeContext,
    pane_count: u8,
    no_worktrees: bool,
    tmux: &impl session::TmuxClient,
) -> Result<session::SessionLaunchOutcome, AppError> {
    let project_dir = std::env::current_dir().map_err(session::SessionError::CurrentDir)?;
    execute_default_session_flow_for_runtime_context(
        project_dir.as_path(),
        runtime_context,
        pane_count,
        no_worktrees,
        tmux,
    )
}

fn execute_kill_session_flow(
    tmux: &impl session::TmuxClient,
) -> Result<session::TeardownOutcome, AppError> {
    let project_dir = std::env::current_dir().map_err(session::SessionError::CurrentDir)?;
    execute_kill_session_flow_for_project_dir(project_dir.as_path(), tmux)
}

fn execute_kill_session_flow_for_project_dir(
    project_dir: &std::path::Path,
    tmux: &impl session::TmuxClient,
) -> Result<session::TeardownOutcome, AppError> {
    let identity = session::resolve_session_identity(project_dir)?;
    session::teardown_session(&identity.session_name, tmux).map_err(AppError::Session)
}

#[cfg(test)]
fn execute_default_session_flow_for_project_dir(
    project_dir: &std::path::Path,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    remote_transport: session::RemoteTransportFlags,
    pane_count: u8,
    no_worktrees: bool,
    tmux: &impl session::TmuxClient,
) -> Result<session::SessionLaunchOutcome, AppError> {
    if remote_path.is_some() {
        validate_resolved_remote_authority(remote_server_url)?;
    }
    let mut runtime_context = config::RuntimeContext::default();
    runtime_context.remote.remote_path = config::ResolvedValue {
        value: remote_path.map(str::to_owned),
        source: if remote_path.is_some() {
            ValueSource::Env
        } else {
            ValueSource::Default
        },
    };
    runtime_context.remote.remote_server_url = config::ResolvedValue {
        value: remote_server_url.map(str::to_owned),
        source: if remote_server_url.is_some() {
            ValueSource::Env
        } else {
            ValueSource::Default
        },
    };
    runtime_context.remote.use_tssh = config::ResolvedValue {
        value: remote_transport.use_tssh,
        source: ValueSource::Env,
    };
    runtime_context.remote.use_mosh = config::ResolvedValue {
        value: remote_transport.use_mosh,
        source: ValueSource::Env,
    };

    execute_default_session_flow_for_runtime_context(
        project_dir,
        &runtime_context,
        pane_count,
        no_worktrees,
        tmux,
    )
}

fn execute_default_session_flow_for_runtime_context(
    project_dir: &std::path::Path,
    runtime_context: &config::RuntimeContext,
    pane_count: u8,
    no_worktrees: bool,
    tmux: &impl session::TmuxClient,
) -> Result<session::SessionLaunchOutcome, AppError> {
    validate_resolved_remote_authority(runtime_context.remote.remote_server_url.value.as_deref())?;

    match session::ensure_project_session_with_runtime_context(
        project_dir,
        runtime_context,
        pane_count,
        no_worktrees,
        tmux,
    ) {
        Ok(outcome) => Ok(outcome),
        Err(session::SessionError::Interrupted) => Err(AppError::Interrupted),
        Err(error) => Err(AppError::Session(error)),
    }
}

fn execute_open_latest(
    active_log_root: &std::path::Path,
    os: OperatingSystem,
    opener: &impl LogOpener,
) -> Result<String, AppError> {
    let opened_log_path = logging::open_latest_log(active_log_root, os, opener)?;
    Ok(format!("opened latest log: {}", opened_log_path.display()))
}

fn execute_internal(
    command: InternalCommand,
    runtime_context: &config::RuntimeContext,
) -> Result<String, AppError> {
    match command {
        InternalCommand::Swap { session, slot } => {
            let tmux = session::ProcessTmuxClient;
            session::TmuxClient::swap_slot_with_center(&tmux, &session, slot)?;
            Ok(String::new())
        }
        InternalCommand::Focus { session, slot } => {
            let tmux = session::ProcessTmuxClient;
            session::focus_slot(&session, slot, &tmux)?;
            Ok(String::new())
        }
        InternalCommand::Mode {
            session,
            slot,
            mode,
        } => execute_internal_mode(&session, slot, mode, runtime_context),
        InternalCommand::Popup {
            session,
            slot,
            client,
        } => execute_internal_popup(&session, slot, client.as_deref(), runtime_context),
        InternalCommand::Auxiliary { session, action } => {
            execute_internal_auxiliary(&session, action, runtime_context)
        }
        InternalCommand::Teardown { session } => {
            let tmux = session::ProcessTmuxClient;
            let outcome = session::teardown_session(&session, &tmux)?;
            Ok(format!(
                "internal teardown complete: session={}; project_session_removed={}; helper_sessions_removed={}; helper_processes_removed={}",
                outcome.session_name,
                outcome.project_session_removed,
                outcome.helper_sessions_removed,
                outcome.helper_processes_removed
            ))
        }
        InternalCommand::Preset { session, preset } => {
            let tmux = session::ProcessTmuxClient;
            let outcome = session::apply_layout_preset(&session, preset, &tmux)?;
            Ok(format!(
                "internal preset complete: session={}; preset={}",
                outcome.session_name,
                outcome.preset.label()
            ))
        }
    }
}

fn execute_internal_mode(
    session_name: &str,
    slot: u8,
    mode: session::SlotMode,
    runtime_context: &config::RuntimeContext,
) -> Result<String, AppError> {
    let tmux = session::ProcessTmuxClient;
    let runtime_context = session::resolve_session_runtime_context(session_name, runtime_context)?;
    validate_resolved_remote_authority(runtime_context.remote.remote_server_url.value.as_deref())?;
    let remote_path = remote_path_for_routing(&runtime_context.remote);
    let shared_server = shared_server_attach_config(&runtime_context.remote);
    let remote_context = session::RemoteModeContext {
        remote_path,
        remote_server_url: runtime_context.remote.remote_server_url.value.as_deref(),
        use_tssh: runtime_context.remote.use_tssh.value,
        use_mosh: runtime_context.remote.use_mosh.value,
    };
    let launch_context = session::SlotModeLaunchContext {
        remote_context,
        shared_server: shared_server.as_ref(),
        agent_command: runtime_context.agent_command.as_deref(),
        opencode_theme: runtime_context.opencode_theme.theme_for_slot(slot),
    };
    let outcome = session::switch_slot_mode(session_name, slot, mode, launch_context, &tmux)?;
    Ok(format!(
        "internal mode complete: session={}; slot={}; mode={}",
        outcome.session_name,
        outcome.slot_id,
        outcome.mode.label()
    ))
}

fn execute_internal_popup(
    session_name: &str,
    slot: u8,
    client_tty: Option<&str>,
    runtime_context: &config::RuntimeContext,
) -> Result<String, AppError> {
    let tmux = session::ProcessTmuxClient;
    let runtime_context = session::resolve_session_runtime_context(session_name, runtime_context)?;
    validate_resolved_remote_authority(runtime_context.remote.remote_server_url.value.as_deref())?;
    let remote_path = remote_path_for_routing(&runtime_context.remote);
    let outcome = session::toggle_popup_shell(
        session_name,
        slot,
        client_tty,
        remote_path,
        runtime_context.remote.remote_server_url.value.as_deref(),
        resolved_remote_transport_flags(&runtime_context.remote),
        &tmux,
    )?;
    Ok(format!(
        "internal popup complete: session={}; slot={}; action={}; cwd={}; width_pct={}; height_pct={}",
        outcome.session_name,
        outcome.slot_id,
        outcome.action.label(),
        outcome.cwd,
        outcome.width_pct,
        outcome.height_pct
    ))
}

fn execute_internal_auxiliary(
    session_name: &str,
    action: AuxiliaryAction,
    runtime_context: &config::RuntimeContext,
) -> Result<String, AppError> {
    let tmux = session::ProcessTmuxClient;
    let runtime_context = session::resolve_session_runtime_context(session_name, runtime_context)?;
    let open = matches!(action, AuxiliaryAction::Open);
    let outcome = session::auxiliary_viewer_with_runtime_context(
        session_name,
        open,
        &runtime_context.session_context(),
        &tmux,
    )?;
    Ok(format!(
        "internal auxiliary complete: session={}; action={}; window_name={}; window_id={}",
        outcome.session_name,
        outcome.action.label(),
        outcome.window_name,
        outcome.window_id.unwrap_or_else(|| String::from("none"))
    ))
}

fn format_kill_message(outcome: &session::TeardownOutcome) -> String {
    format!(
        "kill complete: session={}; project_session_removed={}; helper_sessions_removed={}; helper_processes_removed={}",
        outcome.session_name,
        outcome.project_session_removed,
        outcome.helper_sessions_removed,
        outcome.helper_processes_removed
    )
}

fn attach_visibility_label() -> &'static str {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        "interactive"
    } else {
        "non-interactive"
    }
}

fn shared_server_attach_config(
    remote_runtime: &config::RemoteRuntimeResolution,
) -> Option<session::SharedServerAttachConfig> {
    remote_path_for_routing(remote_runtime)?;

    if remote_runtime.shared_server.url.source == ValueSource::Default {
        return None;
    }

    let url = remote_runtime.shared_server.url.value.clone()?;
    Some(session::SharedServerAttachConfig { url })
}

fn remote_path_for_routing(remote_runtime: &config::RemoteRuntimeResolution) -> Option<&str> {
    remote_runtime
        .remote_path
        .value
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .filter(|_| {
            remote_runtime
                .remote_server_url
                .value
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        })
}

fn validate_resolved_remote_authority(remote_server_url: Option<&str>) -> Result<(), AppError> {
    let Some(remote_server_url) = remote_server_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    session::validate_remote_ssh_authority(remote_server_url).map_err(AppError::Session)
}

fn source_label(source: ValueSource) -> &'static str {
    source.label()
}

fn optional_value_label(value: Option<&str>) -> &str {
    value.unwrap_or("none")
}

fn redacted_optional_value_label(value: Option<&str>) -> String {
    value.map_or_else(
        || String::from("none"),
        session::redact_remote_authority_for_diagnostics,
    )
}

fn default_contract_summary_message(
    verbose: bool,
    outcome: &session::SessionLaunchOutcome,
    resolved_remote_runtime: &config::RemoteRuntimeResolution,
) -> String {
    if !verbose {
        return String::new();
    }

    let attach_visibility = attach_visibility_label();
    if outcome.remote_routing_active {
        format!(
            "ezm contract locked; session={}; session_action={}; routing_mode=remote; remote_routing_active=true; remote_transport={}; ezm_use_tssh_source={}; ezm_use_mosh_source={}; attach_visibility={}; remote_project_dir={}; remote_path={}; remote_path_source={}; ezm_remote_server_url={}; ezm_remote_server_url_source={}; opencode_attach_url={}; opencode_server_url_source={}; opencode_server_password_set={}; opencode_server_password_source={}",
            outcome.identity.session_name,
            outcome.action.label(),
            resolved_remote_transport_label(resolved_remote_runtime),
            source_label(resolved_remote_runtime.use_tssh.source),
            source_label(resolved_remote_runtime.use_mosh.source),
            attach_visibility,
            outcome.remote_project_dir.display(),
            optional_value_label(resolved_remote_runtime.remote_path.value.as_deref()),
            source_label(resolved_remote_runtime.remote_path.source),
            redacted_optional_value_label(
                resolved_remote_runtime.remote_server_url.value.as_deref(),
            ),
            source_label(resolved_remote_runtime.remote_server_url.source),
            redacted_optional_value_label(
                resolved_remote_runtime.shared_server.url.value.as_deref(),
            ),
            source_label(resolved_remote_runtime.shared_server.url.source),
            resolved_remote_runtime
                .shared_server
                .password
                .value
                .is_some(),
            source_label(resolved_remote_runtime.shared_server.password.source)
        )
    } else {
        format!(
            "ezm contract locked; session={}; session_action={}; routing_mode=local; remote_routing_active=false; attach_visibility={}",
            outcome.identity.session_name,
            outcome.action.label(),
            attach_visibility,
        )
    }
}

fn resolved_remote_transport_label(
    resolved_remote_runtime: &config::RemoteRuntimeResolution,
) -> &'static str {
    if resolved_remote_runtime.use_tssh.value {
        "tssh"
    } else if resolved_remote_runtime.use_mosh.value {
        "mosh"
    } else {
        "ssh"
    }
}

fn resolved_remote_transport_flags(
    resolved_remote_runtime: &config::RemoteRuntimeResolution,
) -> session::RemoteTransportFlags {
    session::RemoteTransportFlags {
        use_tssh: resolved_remote_runtime.use_tssh.value,
        use_mosh: resolved_remote_runtime.use_mosh.value,
    }
}

fn format_repair_message(outcome: &session::SessionRepairExecution) -> String {
    format!(
        "repair complete: session={}; action={}; healthy_slots={}; missing_visible_slots={}; missing_backing_slots={}; recreate_order={}; recreated_slots={}",
        outcome.session_name,
        outcome.action_label(),
        format_slot_ids(&outcome.healthy_slots),
        format_slot_ids(&outcome.missing_visible_slots),
        format_slot_ids(&outcome.missing_backing_slots),
        format_slot_ids(&outcome.recreate_order),
        format_slot_ids(&outcome.recreated_slots)
    )
}

fn format_slot_ids(slot_ids: &[u8]) -> String {
    if slot_ids.is_empty() {
        return String::from("none");
    }
    slot_ids
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests;
