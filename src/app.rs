use std::io::IsTerminal;
use thiserror::Error;

use crate::cli::{AuxiliaryAction, Cli, Command, InternalCommand, LogsCommand};
use crate::config::OPERATOR_ENV;
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
    let loaded = config::load_config(env, os)?;
    let resolved_operator = config::resolve_operator(
        cli.operator,
        env.get_var(OPERATOR_ENV),
        loaded.values.operator.clone(),
    );
    let resolved_remote_runtime = config::resolve_remote_runtime(env, &loaded.values)?;

    let message = match cli.command {
        None => {
            let outcome = execute_default_session_flow(
                resolved_remote_runtime.remote_dir_prefix.value.as_deref(),
                &session::ProcessTmuxClient,
            )?;
            let attach_visibility = attach_visibility_label();

            if outcome.remote_routing_active {
                format!(
                    "ezm v1 contract locked; operator source={}. session={}; session_action={}; routing_mode=remote; remote_routing_active=true; attach_visibility={}; remote_project_dir={}; remote_dir_prefix={}; remote_dir_prefix_source={}; opencode_attach_url={}; opencode_server_url_source={}; opencode_server_host={}; opencode_server_host_source={}; opencode_server_port={}; opencode_server_port_source={}; opencode_server_password_set={}; opencode_server_password_source={}",
                    source_label(resolved_operator.source),
                    outcome.identity.session_name,
                    outcome.action.label(),
                    attach_visibility,
                    outcome.remote_project_dir.display(),
                    optional_value_label(
                        resolved_remote_runtime.remote_dir_prefix.value.as_deref()
                    ),
                    source_label(resolved_remote_runtime.remote_dir_prefix.source),
                    resolved_remote_runtime.shared_server.attach_url,
                    source_label(resolved_remote_runtime.shared_server.url.source),
                    resolved_remote_runtime.shared_server.host.value,
                    source_label(resolved_remote_runtime.shared_server.host.source),
                    resolved_remote_runtime.shared_server.port.value,
                    source_label(resolved_remote_runtime.shared_server.port.source),
                    resolved_remote_runtime
                        .shared_server
                        .password
                        .value
                        .is_some(),
                    source_label(resolved_remote_runtime.shared_server.password.source)
                )
            } else {
                format!(
                    "ezm v1 contract locked; operator source={}. session={}; session_action={}; routing_mode=local; remote_routing_active=false; attach_visibility={}",
                    source_label(resolved_operator.source),
                    outcome.identity.session_name,
                    outcome.action.label(),
                    attach_visibility,
                )
            }
        }
        Some(Command::Repair) => {
            let outcome = session::repair_current_project_session(&session::ProcessTmuxClient)?;
            format_repair_message(&outcome)
        }
        Some(Command::Logs(LogsCommand::OpenLatest)) => {
            execute_open_latest(active_log_root, os, opener)?
        }
        Some(Command::Preset { preset }) => {
            let tmux = session::ProcessTmuxClient;
            let outcome = execute_default_session_flow(
                resolved_remote_runtime.remote_dir_prefix.value.as_deref(),
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
        Some(Command::Internal { command }) => execute_internal(
            command,
            resolved_operator.value.as_deref(),
            &resolved_remote_runtime,
        )?,
    };

    Ok(message)
}

fn execute_default_session_flow(
    remote_prefix: Option<&str>,
    tmux: &impl session::TmuxClient,
) -> Result<session::SessionLaunchOutcome, AppError> {
    let project_dir = std::env::current_dir().map_err(session::SessionError::CurrentDir)?;
    execute_default_session_flow_for_project_dir(&project_dir, remote_prefix, tmux)
}

fn execute_default_session_flow_for_project_dir(
    project_dir: &std::path::Path,
    remote_prefix: Option<&str>,
    tmux: &impl session::TmuxClient,
) -> Result<session::SessionLaunchOutcome, AppError> {
    let identity = session::resolve_session_identity(project_dir)?;

    match session::ensure_project_session_with_remote_prefix(project_dir, remote_prefix, tmux) {
        Ok(outcome) => Ok(outcome),
        Err(session::SessionError::Interrupted) => {
            let _ = session::teardown_session(&identity.session_name, tmux);
            Err(AppError::Interrupted)
        }
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
    operator: Option<&str>,
    remote_runtime: &config::RemoteRuntimeResolution,
) -> Result<String, AppError> {
    match command {
        InternalCommand::Swap { session, slot } => {
            let tmux = session::ProcessTmuxClient;
            session::TmuxClient::swap_slot_with_center(&tmux, &session, slot)?;
            Ok(internal_swap_success_message(&session, slot))
        }
        InternalCommand::Focus { session, slot } => {
            let tmux = session::ProcessTmuxClient;
            let outcome = session::focus_slot(&session, slot, &tmux)?;
            Ok(internal_focus_success_message(
                &outcome.session_name,
                outcome.slot_id,
            ))
        }
        InternalCommand::Mode {
            session,
            slot,
            mode,
        } => {
            let tmux = session::ProcessTmuxClient;
            let shared_server = shared_server_attach_config(remote_runtime);
            let outcome = session::switch_slot_mode(
                &session,
                slot,
                mode,
                operator,
                remote_runtime.remote_dir_prefix.value.as_deref(),
                shared_server.as_ref(),
                &tmux,
            )?;
            Ok(format!(
                "internal mode complete: session={}; slot={}; mode={}",
                outcome.session_name,
                outcome.slot_id,
                outcome.mode.label()
            ))
        }
        InternalCommand::Popup {
            session,
            slot,
            client,
        } => {
            let tmux = session::ProcessTmuxClient;
            let outcome = session::toggle_popup_shell(&session, slot, client.as_deref(), &tmux)?;
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
        InternalCommand::Auxiliary { session, action } => {
            let tmux = session::ProcessTmuxClient;
            let open = matches!(action, AuxiliaryAction::Open);
            let outcome = session::auxiliary_viewer(&session, open, &tmux)?;
            Ok(format!(
                "internal auxiliary complete: session={}; action={}; window_name={}; window_id={}",
                outcome.session_name,
                outcome.action.label(),
                outcome.window_name,
                outcome.window_id.unwrap_or_else(|| String::from("none"))
            ))
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

fn internal_swap_success_message(session_name: &str, slot_id: u8) -> String {
    let _ = (session_name, slot_id);
    String::new()
}

fn internal_focus_success_message(session_name: &str, slot_id: u8) -> String {
    let _ = (session_name, slot_id);
    String::new()
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
    remote_runtime.remote_dir_prefix.value.as_ref()?;

    let explicit = remote_runtime.shared_server.url.source != ValueSource::Default
        || remote_runtime.shared_server.host.source != ValueSource::Default
        || remote_runtime.shared_server.port.source != ValueSource::Default
        || remote_runtime.shared_server.password.source != ValueSource::Default;

    explicit.then(|| session::SharedServerAttachConfig {
        url: remote_runtime.shared_server.attach_url.clone(),
        password: remote_runtime.shared_server.password.value.clone(),
    })
}

fn source_label(source: ValueSource) -> &'static str {
    source.label()
}

fn optional_value_label(value: Option<&str>) -> &str {
    value.unwrap_or("none")
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
