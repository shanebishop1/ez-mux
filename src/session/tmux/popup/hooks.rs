use super::super::SessionError;
use super::super::command::tmux_run;

pub(super) const POPUP_PARENT_CLEANUP_HOOK_MARKER: &str = "EZM_POPUP_PARENT_CLEANUP_V2";
pub(super) const POPUP_PARENT_CLEANUP_HOOK_NAME: &str = "session-closed[999]";
#[cfg(test)]
const POPUP_PARENT_CLEANUP_LEGACY_INTERNAL_MARKER: &str = "__internal popup-parent-closed";

pub(super) fn reconcile_popup_parent_cleanup_hook() -> Result<(), SessionError> {
    let args = popup_parent_cleanup_hook_install_command();
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    tmux_run(&refs)?;
    Ok(())
}

pub(super) fn popup_parent_cleanup_hook_install_command() -> Vec<String> {
    vec![
        String::from("set-hook"),
        String::from("-g"),
        String::from(POPUP_PARENT_CLEANUP_HOOK_NAME),
        popup_parent_cleanup_hook_command(),
    ]
}

#[cfg(test)]
pub(super) fn popup_cleanup_hook_names(hooks: &str) -> Vec<String> {
    hooks
        .lines()
        .filter(|line| {
            line.contains(POPUP_PARENT_CLEANUP_LEGACY_INTERNAL_MARKER)
                || (line.contains("#{hook_session_name}__popup_slot_")
                    && !line.contains(POPUP_PARENT_CLEANUP_HOOK_MARKER))
        })
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
pub(super) fn hooks_contain_popup_parent_cleanup(hooks: &str) -> bool {
    hooks.contains(POPUP_PARENT_CLEANUP_HOOK_MARKER)
}

pub(super) fn popup_parent_cleanup_hook_command() -> String {
    let command = popup_parent_cleanup_script();
    format!("run-shell -b \"{}\"", shell_escape_double_quoted(&command))
}

fn popup_parent_cleanup_script() -> String {
    let mut commands = Vec::with_capacity(12);
    for slot_id in 1_u8..=5 {
        commands.push(format!(
            "tmux has-session -t \"#{{hook_session_name}}__popup_slot_{slot_id}\" 2>/dev/null && tmux kill-session -t \"#{{hook_session_name}}__popup_slot_{slot_id}\" >/dev/null 2>&1"
        ));
        commands.push(format!(
            "tmux has-session -t \"#{{hook_session_name}}__mode_slot_{slot_id}\" 2>/dev/null && tmux kill-session -t \"#{{hook_session_name}}__mode_slot_{slot_id}\" >/dev/null 2>&1"
        ));
    }
    commands.push(
        "tmux has-session -t \"#{hook_session_name}__mode_cache\" 2>/dev/null && tmux kill-session -t \"#{hook_session_name}__mode_cache\" >/dev/null 2>&1".to_owned(),
    );
    commands.push(format!(": # {POPUP_PARENT_CLEANUP_HOOK_MARKER}"));
    commands.join("; ")
}

fn shell_escape_double_quoted(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}
