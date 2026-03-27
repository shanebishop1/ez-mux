use std::path::Path;

use super::DEFAULT_CENTER_WIDTH_PCT;
use super::LayoutPreset;
use super::SessionError;
use super::SlotRegistry;
use super::build_registry_for_canonical_panes;
use super::canonical_five_pane_column_widths;
use super::command::{tmux_output_value, tmux_run, tmux_run_batch};
use super::keybinds::install_runtime_keybinds;
use super::slot_swap::validate_canonical_slot_registry;
use super::style::apply_runtime_style_defaults_for_target;
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
    let BootstrapAnchor {
        window_target: target,
        pane_id: initial_pane,
        window_width,
    } = resolve_bootstrap_anchor(session_name)?;
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
        install_runtime_keybinds()?;
        if should_apply_runtime_styles_during_bootstrap() {
            apply_runtime_style_defaults_for_target(session_name, &target)?;
        }
        launch_startup_slot_modes(session_name, &canonical_pane_ids[0])?;

        if should_validate_registry_after_bootstrap() {
            validate_canonical_slot_registry(session_name)?;
        }
        Ok(())
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct BootstrapAnchor {
    window_target: String,
    pane_id: String,
    window_width: u16,
}

fn resolve_bootstrap_anchor(session_name: &str) -> Result<BootstrapAnchor, SessionError> {
    let command = format!(
        "display-message -p -t {session_name} #{{window_id}}|#{{pane_id}}|#{{window_width}}"
    );
    let output = tmux_output_value(&[
        "display-message",
        "-p",
        "-t",
        session_name,
        "#{window_id}|#{pane_id}|#{window_width}",
    ])?;
    parse_bootstrap_anchor(&output).map_err(|reason| SessionError::TmuxCommandFailed {
        command,
        stderr: reason,
    })
}

fn parse_bootstrap_anchor(output: &str) -> Result<BootstrapAnchor, String> {
    let row = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| String::from("tmux returned no bootstrap anchor row"))?;
    let mut parts = row.split('|');

    let window_target = parts
        .next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing window id in bootstrap row: {row}"))?
        .trim()
        .to_owned();
    let pane_id = parts
        .next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing pane id in bootstrap row: {row}"))?
        .trim()
        .to_owned();
    let window_width_raw = parts
        .next()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing window width in bootstrap row: {row}"))?
        .trim();
    let window_width = window_width_raw.parse::<u16>().map_err(|error| {
        format!("invalid window width `{window_width_raw}` in bootstrap row `{row}`: {error}")
    })?;

    Ok(BootstrapAnchor {
        window_target,
        pane_id,
        window_width,
    })
}

fn persist_registry(
    session_name: &str,
    registry: &SlotRegistry,
    populated_slots: usize,
) -> Result<(), SessionError> {
    let write_strategy = bootstrap_registry_write_strategy();
    let mut commands = Vec::new();

    for binding in registry.bindings() {
        let mode = startup_mode_for_slot(binding.slot_id, populated_slots);
        let slot_pane_key = format!("@ezm_slot_{}_pane", binding.slot_id);
        let slot_worktree_key = format!("@ezm_slot_{}_worktree", binding.slot_id);
        let slot_cwd_key = format!("@ezm_slot_{}_cwd", binding.slot_id);
        let slot_mode_key = format!("@ezm_slot_{}_mode", binding.slot_id);
        let worktree_value = binding.worktree_path.display().to_string();
        commands.push(write_strategy.session_option_command(
            session_name,
            &slot_pane_key,
            &binding.pane_id,
        ));
        commands.push(write_strategy.session_option_command(
            session_name,
            &slot_worktree_key,
            &worktree_value,
        ));
        commands.push(write_strategy.session_option_command(
            session_name,
            &slot_cwd_key,
            &worktree_value,
        ));
        commands.push(write_strategy.session_option_command(session_name, &slot_mode_key, mode));

        let pane_worktree_key = "@ezm_slot_worktree";
        let pane_slot_key = "@ezm_slot_id";
        let pane_cwd_key = "@ezm_slot_cwd";
        let pane_mode_key = "@ezm_slot_mode";
        commands.push(write_strategy.pane_option_command(
            &binding.pane_id,
            pane_slot_key,
            &binding.slot_id.to_string(),
        ));
        commands.push(write_strategy.pane_option_command(
            &binding.pane_id,
            pane_worktree_key,
            &worktree_value,
        ));
        commands.push(write_strategy.pane_option_command(
            &binding.pane_id,
            pane_cwd_key,
            &worktree_value,
        ));
        commands.push(write_strategy.pane_option_command(&binding.pane_id, pane_mode_key, mode));
    }

    commands.push(vec![
        String::from("set-option"),
        String::from("-t"),
        session_name.to_owned(),
        preset::slot_suspended_key(4),
        String::from("0"),
    ]);
    commands.push(vec![
        String::from("set-option"),
        String::from("-t"),
        session_name.to_owned(),
        preset::slot_suspended_key(5),
        String::from("0"),
    ]);
    commands.push(vec![
        String::from("set-option"),
        String::from("-t"),
        session_name.to_owned(),
        String::from(LAYOUT_MODE_KEY),
        String::from(LAYOUT_MODE_FIVE_PANE),
    ]);

    tmux_run_batch(&commands)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryWriteStrategy {
    SetOnly,
}

impl RegistryWriteStrategy {
    fn session_option_command(self, session_name: &str, key: &str, value: &str) -> Vec<String> {
        match self {
            Self::SetOnly => vec![
                String::from("set-option"),
                String::from("-t"),
                session_name.to_owned(),
                key.to_owned(),
                value.to_owned(),
            ],
        }
    }

    fn pane_option_command(self, pane_id: &str, key: &str, value: &str) -> Vec<String> {
        match self {
            Self::SetOnly => vec![
                String::from("set-option"),
                String::from("-p"),
                String::from("-t"),
                pane_id.to_owned(),
                key.to_owned(),
                value.to_owned(),
            ],
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
    true
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

fn launch_startup_slot_modes(session_name: &str, center_pane_id: &str) -> Result<(), SessionError> {
    let ezm_bin = resolved_ezm_bin_shell_token();
    let mut commands = Vec::with_capacity(6);

    for slot_id in 1_u8..=5 {
        let command = startup_mode_schedule_command(&ezm_bin, session_name, slot_id);
        commands.push(vec![String::from("run-shell"), String::from("-b"), command]);
    }

    commands.push(vec![
        String::from("select-pane"),
        String::from("-t"),
        center_pane_id.to_owned(),
    ]);

    tmux_run_batch(&commands)
}

fn startup_mode_schedule_command(ezm_bin: &str, session_name: &str, slot_id: u8) -> String {
    format!(
        "sleep 0.05; EZM_STARTUP_SLOT_MODE=1 {ezm_bin} __internal mode --session {} --slot {slot_id} --mode agent </dev/null >/dev/null 2>&1",
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
        RegistryWriteStrategy, bootstrap_registry_write_strategy, parse_bootstrap_anchor,
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
        assert!(rendered.contains("EZM_STARTUP_SLOT_MODE=1"));
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
    fn bootstrap_applies_runtime_style_on_first_attach() {
        assert!(should_apply_runtime_styles_during_bootstrap());
    }

    #[test]
    fn parse_bootstrap_anchor_reads_window_pane_and_width() {
        let parsed = parse_bootstrap_anchor("@9|%42|192\n").expect("parse bootstrap anchor");
        assert_eq!(parsed.window_target, String::from("@9"));
        assert_eq!(parsed.pane_id, String::from("%42"));
        assert_eq!(parsed.window_width, 192);
    }
}
