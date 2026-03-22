use std::path::Path;

use super::DEFAULT_CENTER_WIDTH_PCT;
use super::LayoutPreset;
use super::SessionError;
use super::SlotRegistry;
use super::build_registry_for_canonical_panes;
use super::canonical_five_pane_column_widths;
use super::command::{tmux_output_value, tmux_primary_window_target, tmux_run};
use super::keybinds::install_runtime_keybinds;
use super::options::{set_pane_option, set_session_option};
use super::slot_swap::validate_canonical_slot_registry;
use super::style::apply_runtime_style_defaults;
use super::worktree::discover_worktrees_for_slots;
use crate::config::EZM_BIN_ENV;

mod preset;

pub(super) const LAYOUT_MODE_KEY: &str = "@ezm_layout_mode";
pub(super) const LAYOUT_MODE_FIVE_PANE: &str = "five-pane";
pub(super) const LAYOUT_MODE_THREE_PANE: &str = "three-pane";
pub(super) const SLOT_SUSPENDED_KEY_PREFIX: &str = "@ezm_slot_";

pub(super) fn bootstrap_default_layout(
    session_name: &str,
    project_dir: &Path,
) -> Result<(), SessionError> {
    let target = tmux_primary_window_target(session_name)?;
    let initial_pane = tmux_output_value(&["list-panes", "-t", &target, "-F", "#{pane_id}"])?
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .ok_or_else(|| SessionError::TmuxCommandFailed {
            command: format!("list-panes -t {target} -F #{{pane_id}}"),
            stderr: String::from("tmux returned no pane id for initial session window"),
        })?
        .to_owned();

    let window_width =
        tmux_output_value(&["display-message", "-p", "-t", &target, "#{window_width}"])?
            .trim()
            .parse::<u16>()
            .map_err(|error| SessionError::TmuxCommandFailed {
                command: format!("display-message -p -t {target} #{{window_width}}"),
                stderr: format!("failed parsing window width: {error}"),
            })?;
    let (_left_width, center_width, right_width) =
        canonical_five_pane_column_widths(window_width, DEFAULT_CENTER_WIDTH_PCT);

    let mut created_panes = Vec::with_capacity(4);
    let result = (|| {
        let right_top = split_pane_horizontal(&initial_pane, right_width)?;
        created_panes.push(right_top.clone());
        let center = split_pane_horizontal(&initial_pane, center_width)?;
        created_panes.push(center.clone());
        let left_bottom = split_pane_vertical(&initial_pane)?;
        created_panes.push(left_bottom.clone());
        let right_bottom = split_pane_vertical(&right_top)?;
        created_panes.push(right_bottom.clone());

        let canonical_pane_ids = [
            center,
            initial_pane.clone(),
            right_top,
            left_bottom,
            right_bottom,
        ];
        let discovery = discover_worktrees_for_slots(project_dir);
        if let Some(warning) = &discovery.warning {
            eprintln!("warning: {warning}");
        }
        let populated_slots = discovery.worktrees.len().min(5);
        let registry =
            build_registry_for_canonical_panes(&canonical_pane_ids, &discovery.worktrees)?;
        persist_registry(session_name, &registry, populated_slots)?;
        set_session_option(session_name, &preset::slot_suspended_key(4), "0")?;
        set_session_option(session_name, &preset::slot_suspended_key(5), "0")?;
        set_session_option(session_name, LAYOUT_MODE_KEY, LAYOUT_MODE_FIVE_PANE)?;
        install_runtime_keybinds()?;
        if should_apply_runtime_styles_during_bootstrap() {
            apply_runtime_style_defaults(session_name)?;
        }
        launch_startup_slot_modes(session_name)?;

        if should_validate_registry_after_bootstrap() {
            validate_canonical_slot_registry(session_name)?;
        }
        tmux_run(&["select-pane", "-t", &canonical_pane_ids[0]])
    })();

    if let Err(error) = result {
        if let Err(compensation_error) = kill_created_panes(&created_panes) {
            return Err(SessionError::TmuxCommandFailed {
                command: format!("bootstrap-default-layout -t {session_name}"),
                stderr: format!(
                    "layout bootstrap failed: {error}; compensation failed while cleaning panes: {compensation_error}"
                ),
            });
        }

        return Err(error);
    }

    Ok(())
}

pub(super) fn apply_layout_preset(
    session_name: &str,
    preset: LayoutPreset,
) -> Result<(), SessionError> {
    preset::apply_layout_preset(session_name, preset)
}

fn split_pane_horizontal(target_pane: &str, new_width: u16) -> Result<String, SessionError> {
    tmux_output_value(&[
        "split-window",
        "-h",
        "-t",
        target_pane,
        "-l",
        &new_width.to_string(),
        "-P",
        "-F",
        "#{pane_id}",
    ])
    .map(|value| value.trim().to_owned())
}

fn split_pane_vertical(target_pane: &str) -> Result<String, SessionError> {
    tmux_output_value(&[
        "split-window",
        "-v",
        "-t",
        target_pane,
        "-P",
        "-F",
        "#{pane_id}",
    ])
    .map(|value| value.trim().to_owned())
}

fn persist_registry(
    session_name: &str,
    registry: &SlotRegistry,
    populated_slots: usize,
) -> Result<(), SessionError> {
    let write_strategy = bootstrap_registry_write_strategy();

    for binding in registry.bindings() {
        let mode = startup_mode_for_slot(binding.slot_id, populated_slots);
        let slot_pane_key = format!("@ezm_slot_{}_pane", binding.slot_id);
        let slot_worktree_key = format!("@ezm_slot_{}_worktree", binding.slot_id);
        let slot_cwd_key = format!("@ezm_slot_{}_cwd", binding.slot_id);
        let slot_mode_key = format!("@ezm_slot_{}_mode", binding.slot_id);
        let worktree_value = binding.worktree_path.display().to_string();
        write_strategy.write_session_option(session_name, &slot_pane_key, &binding.pane_id)?;
        write_strategy.write_session_option(session_name, &slot_worktree_key, &worktree_value)?;
        write_strategy.write_session_option(session_name, &slot_cwd_key, &worktree_value)?;
        write_strategy.write_session_option(session_name, &slot_mode_key, mode)?;

        let pane_worktree_key = "@ezm_slot_worktree";
        let pane_slot_key = "@ezm_slot_id";
        let pane_cwd_key = "@ezm_slot_cwd";
        let pane_mode_key = "@ezm_slot_mode";
        write_strategy.write_pane_option(
            &binding.pane_id,
            pane_slot_key,
            &binding.slot_id.to_string(),
        )?;
        write_strategy.write_pane_option(&binding.pane_id, pane_worktree_key, &worktree_value)?;
        write_strategy.write_pane_option(&binding.pane_id, pane_cwd_key, &worktree_value)?;
        write_strategy.write_pane_option(&binding.pane_id, pane_mode_key, mode)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryWriteStrategy {
    SetOnly,
}

impl RegistryWriteStrategy {
    fn write_session_option(
        self,
        session_name: &str,
        key: &str,
        value: &str,
    ) -> Result<(), SessionError> {
        match self {
            Self::SetOnly => set_session_option(session_name, key, value),
        }
    }

    fn write_pane_option(self, pane_id: &str, key: &str, value: &str) -> Result<(), SessionError> {
        match self {
            Self::SetOnly => set_pane_option(pane_id, key, value),
        }
    }
}

fn bootstrap_registry_write_strategy() -> RegistryWriteStrategy {
    RegistryWriteStrategy::SetOnly
}

fn should_validate_registry_after_bootstrap() -> bool {
    false
}

fn should_apply_runtime_styles_during_bootstrap() -> bool {
    false
}

fn startup_mode_for_slot(_slot_id: u8, _populated_slots: usize) -> &'static str {
    "agent"
}

fn kill_created_panes(created_panes: &[String]) -> Result<(), SessionError> {
    for pane_id in created_panes.iter().rev() {
        tmux_run(&["kill-pane", "-t", pane_id])?;
    }

    Ok(())
}

fn launch_startup_slot_modes(session_name: &str) -> Result<(), SessionError> {
    let ezm_bin = resolved_ezm_bin_shell_token();

    for slot_id in 1_u8..=5 {
        let command = startup_mode_schedule_command(&ezm_bin, session_name, slot_id);
        tmux_run(&["run-shell", "-b", &command])?;
    }

    Ok(())
}

fn startup_mode_schedule_command(ezm_bin: &str, session_name: &str, slot_id: u8) -> String {
    format!(
        "sleep 0.05; {ezm_bin} __internal mode --session {} --slot {slot_id} --mode agent </dev/null >/dev/null 2>&1",
        shell_single_quote(session_name)
    )
}

fn resolved_ezm_bin_shell_token() -> String {
    let env_ezm_bin = std::env::var(EZM_BIN_ENV)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let current_exe = std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string());

    shell_single_quote(
        &env_ezm_bin
            .or(current_exe)
            .unwrap_or_else(|| String::from("ezm")),
    )
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::{
        RegistryWriteStrategy, bootstrap_registry_write_strategy,
        should_apply_runtime_styles_during_bootstrap, should_validate_registry_after_bootstrap,
        startup_mode_for_slot, startup_mode_schedule_command,
    };

    #[test]
    fn startup_mode_defaults_visible_slots_to_agent_when_worktree_candidates_are_underfilled() {
        let modes = (1_u8..=5)
            .map(|slot_id| startup_mode_for_slot(slot_id, 1))
            .collect::<Vec<_>>();

        assert_eq!(modes, vec!["agent", "agent", "agent", "agent", "agent"]);
    }

    #[test]
    fn startup_mode_schedule_command_runs_internal_mode_in_background() {
        let rendered = startup_mode_schedule_command("'ezm'", "ezm-demo", 3);
        assert!(rendered.contains("sleep 0.05;"));
        assert!(rendered.contains("__internal mode"));
        assert!(rendered.contains("--session 'ezm-demo'"));
        assert!(rendered.contains("--slot 3"));
        assert!(rendered.contains("--mode agent"));
        assert!(rendered.contains("</dev/null >/dev/null 2>&1"));
    }

    #[test]
    fn bootstrap_registry_uses_set_only_writes() {
        assert_eq!(
            bootstrap_registry_write_strategy(),
            RegistryWriteStrategy::SetOnly
        );
    }

    #[test]
    fn bootstrap_skips_full_registry_validation_roundtrip() {
        assert!(!should_validate_registry_after_bootstrap());
    }

    #[test]
    fn bootstrap_defers_runtime_style_application() {
        assert!(!should_apply_runtime_styles_during_bootstrap());
    }
}
