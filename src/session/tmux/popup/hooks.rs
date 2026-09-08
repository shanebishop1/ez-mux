use super::super::SessionError;
use super::super::command::{tmux_output, tmux_run};
use std::process::Output;

pub(super) const POPUP_PARENT_CLEANUP_HOOK_MARKER: &str = "EZM_POPUP_PARENT_CLEANUP_V2";
const POPUP_PARENT_CLEANUP_LEGACY_INTERNAL_MARKER: &str = "__internal popup-parent-closed";
const POPUP_PARENT_CLEANUP_HOOK_PREFIX: &str = "session-closed[";

pub(super) fn reconcile_popup_parent_cleanup_hook() -> Result<(), SessionError> {
    reconcile_popup_parent_cleanup_hook_with_runner(
        || tmux_output(&["show-hooks", "-g", "session-closed"]),
        tmux_run,
    )
}

pub(super) fn reconcile_popup_parent_cleanup_hook_with_runner(
    show_hooks: impl FnOnce() -> Result<Output, SessionError>,
    mut run: impl FnMut(&[&str]) -> Result<(), SessionError>,
) -> Result<(), SessionError> {
    let output = show_hooks()?;
    if !output.status.success() {
        return Err(SessionError::TmuxCommandFailed {
            command: String::from("show-hooks -g session-closed"),
            stderr: super::super::command::format_output_diagnostics(&output),
        });
    }

    let hooks = String::from_utf8_lossy(&output.stdout);
    for hook_name in popup_cleanup_hook_names_for_reconciliation(&hooks) {
        run(&["set-hook", "-gu", &hook_name])?;
    }

    let args = if hooks.lines().find_map(parse_hook_line).is_none() {
        popup_parent_cleanup_hook_install_command()
    } else {
        let hook_name = popup_parent_cleanup_hook_name_for_reconciliation(&hooks);
        popup_parent_cleanup_hook_install_command_for_name(&hook_name)
    };
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run(&refs)?;
    Ok(())
}

pub(super) fn popup_parent_cleanup_hook_install_command() -> Vec<String> {
    vec![
        String::from("set-hook"),
        String::from("-ag"),
        String::from("session-closed"),
        popup_parent_cleanup_hook_command(),
    ]
}

pub(super) fn popup_parent_cleanup_hook_install_command_for_name(hook_name: &str) -> Vec<String> {
    vec![
        String::from("set-hook"),
        String::from("-g"),
        hook_name.to_owned(),
        popup_parent_cleanup_hook_command(),
    ]
}

#[cfg(test)]
pub(super) fn popup_cleanup_hook_names(hooks: &str) -> Vec<String> {
    popup_cleanup_hook_names_for_reconciliation(hooks)
}

#[cfg(test)]
pub(super) fn hooks_contain_popup_parent_cleanup(hooks: &str) -> bool {
    hooks.contains(POPUP_PARENT_CLEANUP_HOOK_MARKER)
}

pub(super) fn popup_parent_cleanup_hook_command() -> String {
    let command = popup_parent_cleanup_script();
    format!("run-shell -b \"{}\"", tmux_escape_double_quoted(&command))
}

fn popup_parent_cleanup_script() -> String {
    let mut commands = Vec::with_capacity(12);
    commands.push(String::from(
        "if test \"#{m:*[!a-z0-9-]*,#{hook_session_name}}\" = 1; then exit 0; fi; if test \"#{m:ezm-*-????????????,#{hook_session_name}}\" != 1; then exit 0; fi",
    ));

    for slot_id in 1_u8..=5 {
        commands.push(format!(
            "tmux list-sessions -f '##{{==:##{{session_name}},#{{q:hook_session_name}}__popup_slot_{slot_id}}}' -F '##{{session_id}}' | xargs -n 1 tmux kill-session -t >/dev/null 2>&1"
        ));
        commands.push(format!(
            "tmux list-sessions -f '##{{==:##{{session_name}},#{{q:hook_session_name}}__mode_slot_{slot_id}}}' -F '##{{session_id}}' | xargs -n 1 tmux kill-session -t >/dev/null 2>&1"
        ));
    }
    commands.push(
        "tmux list-sessions -f '##{==:##{session_name},#{q:hook_session_name}__mode_cache}' -F '##{session_id}' | xargs -n 1 tmux kill-session -t >/dev/null 2>&1".to_owned(),
    );
    commands.push(format!(": # {POPUP_PARENT_CLEANUP_HOOK_MARKER}"));
    commands.join("; ")
}

fn tmux_escape_double_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
}

fn popup_cleanup_hook_names_for_reconciliation(hooks: &str) -> Vec<String> {
    hooks
        .lines()
        .filter_map(parse_hook_line)
        .filter(|(_, command)| is_ezm_owned_cleanup_hook(command))
        .map(|(name, _)| name.to_owned())
        .collect()
}

pub(super) fn popup_parent_cleanup_hook_name_for_reconciliation(hooks: &str) -> String {
    let owned_index = popup_cleanup_hook_names_for_reconciliation(hooks)
        .iter()
        .filter_map(|name| hook_index(name))
        .min();
    if let Some(index) = owned_index {
        return hook_name(index);
    }

    let occupied_indices = hooks
        .lines()
        .filter_map(parse_hook_line)
        .filter_map(|(name, _)| hook_index(name))
        .collect::<std::collections::BTreeSet<_>>();
    let free_index = (0..=occupied_indices.len()).find(|index| !occupied_indices.contains(index));
    hook_name(free_index.expect("hook index space must contain a free index"))
}

fn parse_hook_line(line: &str) -> Option<(&str, &str)> {
    let (name, command) = line.trim().split_once(char::is_whitespace)?;
    hook_index(name).map(|_| (name, command.trim()))
}

fn hook_index(name: &str) -> Option<usize> {
    name.strip_prefix(POPUP_PARENT_CLEANUP_HOOK_PREFIX)?
        .strip_suffix(']')?
        .parse()
        .ok()
}

fn hook_name(index: usize) -> String {
    format!("{POPUP_PARENT_CLEANUP_HOOK_PREFIX}{index}]")
}

fn is_ezm_owned_cleanup_hook(command: &str) -> bool {
    command.starts_with("run-shell -b ")
        && (command.contains(POPUP_PARENT_CLEANUP_HOOK_MARKER)
            || command.contains(POPUP_PARENT_CLEANUP_LEGACY_INTERNAL_MARKER))
}
