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
    let loaded = config::load_config(env, os)?;
    let resolved_remote_runtime = config::resolve_remote_runtime(env, &loaded.values)?;
    let opencode_theme_runtime = config::resolve_opencode_theme_runtime(&loaded.values);
    let remote_path = remote_path_for_routing(&resolved_remote_runtime);

    let message = match cli.command {
        None => {
            let outcome = execute_default_session_flow(
                remote_path,
                resolved_remote_runtime.remote_server_url.value.as_deref(),
                &session::ProcessTmuxClient,
            )?;
            default_contract_summary_message(cli.verbose > 0, &outcome, &resolved_remote_runtime)
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
            let tmux = session::ProcessTmuxClient;
            let outcome = execute_default_session_flow(
                remote_path,
                resolved_remote_runtime.remote_server_url.value.as_deref(),
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
            remote_path,
            &resolved_remote_runtime,
            &opencode_theme_runtime,
        )?,
    };

    Ok(message)
}

fn execute_default_session_flow(
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    tmux: &impl session::TmuxClient,
) -> Result<session::SessionLaunchOutcome, AppError> {
    let project_dir = std::env::current_dir().map_err(session::SessionError::CurrentDir)?;
    execute_default_session_flow_for_project_dir(
        project_dir.as_path(),
        remote_path,
        remote_server_url,
        tmux,
    )
}

fn execute_default_session_flow_for_project_dir(
    project_dir: &std::path::Path,
    remote_path: Option<&str>,
    remote_server_url: Option<&str>,
    tmux: &impl session::TmuxClient,
) -> Result<session::SessionLaunchOutcome, AppError> {
    let identity = session::resolve_session_identity(project_dir)?;

    match session::ensure_project_session_with_remote_path(
        project_dir,
        remote_path,
        remote_server_url,
        tmux,
    ) {
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
    remote_path: Option<&str>,
    remote_runtime: &config::RemoteRuntimeResolution,
    opencode_theme_runtime: &config::OpencodeThemeRuntimeResolution,
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
            let remote_context = session::RemoteModeContext {
                remote_path,
                remote_server_url: remote_runtime.remote_server_url.value.as_deref(),
            };
            let outcome = session::switch_slot_mode(
                &session,
                slot,
                mode,
                remote_context,
                shared_server.as_ref(),
                opencode_theme_runtime.theme_for_slot(slot),
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
            let outcome = session::toggle_popup_shell(
                &session,
                slot,
                client.as_deref(),
                remote_path,
                remote_runtime.remote_server_url.value.as_deref(),
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
    remote_path_for_routing(remote_runtime)?;

    if remote_runtime.shared_server.url.source == ValueSource::Default {
        return None;
    }

    let url = remote_runtime.shared_server.url.value.clone()?;
    Some(session::SharedServerAttachConfig {
        url,
        password: remote_runtime.shared_server.password.value.clone(),
    })
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

fn source_label(source: ValueSource) -> &'static str {
    source.label()
}

fn optional_value_label(value: Option<&str>) -> &str {
    value.unwrap_or("none")
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
            "ezm contract locked; session={}; session_action={}; routing_mode=remote; remote_routing_active=true; attach_visibility={}; remote_project_dir={}; remote_path={}; remote_path_source={}; ezm_remote_server_url={}; ezm_remote_server_url_source={}; opencode_attach_url={}; opencode_server_url_source={}; opencode_server_password_set={}; opencode_server_password_source={}",
            outcome.identity.session_name,
            outcome.action.label(),
            attach_visibility,
            outcome.remote_project_dir.display(),
            optional_value_label(resolved_remote_runtime.remote_path.value.as_deref()),
            source_label(resolved_remote_runtime.remote_path.source),
            optional_value_label(resolved_remote_runtime.remote_server_url.value.as_deref()),
            source_label(resolved_remote_runtime.remote_server_url.source),
            optional_value_label(resolved_remote_runtime.shared_server.url.value.as_deref()),
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
