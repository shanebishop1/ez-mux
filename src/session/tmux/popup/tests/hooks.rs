use super::super::hooks::{
    POPUP_PARENT_CLEANUP_HOOK_MARKER, hooks_contain_popup_parent_cleanup, popup_cleanup_hook_names,
    popup_parent_cleanup_hook_command, popup_parent_cleanup_hook_install_command_for_name,
    reconcile_popup_parent_cleanup_hook_with_runner,
};
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

const PRIVATE_SENTINEL_OPTION: &str = "@ezm_popup_hook_private_sentinel";
static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(0);

#[test]
fn popup_cleanup_hook_names_match_popup_cleanup_entries_only() {
    let hooks = concat!(
        "session-closed[0] run-shell -b \"tmux kill-session -t \\\"#{hook_session_name}__popup_slot_1\\\"; : # EZM_POPUP_PARENT_CLEANUP_V2\"\n",
        "session-closed[1] display-message keep-me\n",
        "session-closed[2] run-shell -b \"/tmp/ezm __internal popup-parent-closed --session \\\"#{hook_session_name}\\\"\"\n",
        "session-closed[3] run-shell -b \"tmux kill-session -t \\\"#{hook_session_name}__popup_slot_5\\\"\"\n"
    );

    assert_eq!(
        popup_cleanup_hook_names(hooks),
        vec![
            String::from("session-closed[0]"),
            String::from("session-closed[2]"),
        ]
    );
}

#[test]
fn popup_parent_cleanup_hook_command_invokes_shell_cleanup_route() {
    let rendered = popup_parent_cleanup_hook_command();
    assert!(rendered.starts_with("run-shell -b \""));
    assert!(rendered.contains("#{q:hook_session_name}"));
    assert!(rendered.contains("-F '##{session_id}'"));
    assert!(rendered.contains("__popup_slot_1"));
    assert!(rendered.contains("__popup_slot_5"));
    assert!(rendered.contains("__mode_cache"));
    assert!(rendered.contains("EZM_POPUP_PARENT_CLEANUP_V2"));
    assert!(rendered.ends_with('"'));
    assert!(!rendered.contains("'\"'\"'"));
}

#[test]
fn popup_parent_cleanup_hook_rejects_non_ezm_names_before_dynamic_targets() {
    let rendered = popup_parent_cleanup_hook_command();
    assert!(rendered.contains("#{m:*[!a-z0-9-]*,#{hook_session_name}}"));
    assert!(rendered.contains("#{m:ezm-*-????????????,#{hook_session_name}}"));
}

#[test]
fn popup_parent_cleanup_hook_uses_only_stable_session_ids_for_kill_targets() {
    let rendered = popup_parent_cleanup_hook_command();
    assert!(!rendered.contains("kill-session -t #{hook_session_name}"));
    assert!(rendered.contains("xargs -n 1 tmux kill-session -t"));
}

#[test]
fn popup_parent_cleanup_hook_detection_uses_script_marker() {
    let hooks = "session-closed[0] run-shell -b \"tmux has-session -t \\\"#{hook_session_name}__popup_slot_1\\\"; : # EZM_POPUP_PARENT_CLEANUP_V2\"";
    assert!(hooks_contain_popup_parent_cleanup(hooks));
}

#[test]
fn popup_cleanup_hook_names_ignore_non_popup_cleanup_hooks() {
    let hooks = concat!(
        "session-closed\n",
        "session-closed[0] display-message keep-me\n",
        "pane-died[0] run-shell -b \"echo other\"\n"
    );

    assert!(popup_cleanup_hook_names(hooks).is_empty());
}

#[test]
fn popup_cleanup_hook_names_include_current_parent_cleanup_hook_entries() {
    let hooks = "session-closed[0] run-shell -b \"tmux has-session -t \\\"#{hook_session_name}__popup_slot_1\\\"; : # EZM_POPUP_PARENT_CLEANUP_V2\"";
    assert_eq!(
        popup_cleanup_hook_names(hooks),
        vec![String::from("session-closed[0]")]
    );
}

#[test]
fn popup_cleanup_hook_names_include_legacy_internal_cleanup_entries() {
    let hooks = "session-closed[2] run-shell -b \"/tmp/ezm __internal popup-parent-closed --session \\\"#{hook_session_name}\\\"\"";
    assert_eq!(
        popup_cleanup_hook_names(hooks),
        vec![String::from("session-closed[2]")]
    );
}

#[test]
fn popup_cleanup_reconciliation_chooses_free_index_without_claiming_999() {
    let hooks = concat!(
        "session-closed[0] display-message occupied-zero\n",
        "session-closed[999] display-message unrelated\n"
    );

    assert!(popup_cleanup_hook_names(hooks).is_empty());
    let selected = super::super::hooks::popup_parent_cleanup_hook_name_for_reconciliation(hooks);
    assert_eq!(selected, "session-closed[1]");
}

#[test]
fn popup_hook_reconciliation_preserves_unrelated_index_999() {
    let tmux = IsolatedTmux::new(short_private_socket());
    tmux.reconcile_hook();
    tmux.set_hook("session-closed[999]", "display-message unrelated");

    tmux.reconcile_hook();
    tmux.reconcile_hook();

    let hooks = tmux.show_hooks();
    assert!(
        hooks
            .lines()
            .any(|line| { line.starts_with("session-closed[999] display-message unrelated") })
    );
    assert!(hooks.lines().any(|line| {
        line.starts_with("session-closed[0] ") && line.contains(POPUP_PARENT_CLEANUP_HOOK_MARKER)
    }));
    assert!(!hooks.lines().any(|line| {
        line.starts_with("session-closed[999] ") && line.contains(POPUP_PARENT_CLEANUP_HOOK_MARKER)
    }));
}

#[test]
fn popup_cleanup_hook_isolated_socket_regression_covers_shell_metacharacters() {
    let tmux = IsolatedTmux::new(short_private_socket());

    let legitimate_parent = "ezm-project-0123456789ab";
    tmux.new_session(legitimate_parent);
    for suffix in ["__popup_slot_1", "__mode_slot_1", "__mode_cache"] {
        let helper = format!("{legitimate_parent}{suffix}");
        tmux.new_session(&helper);
    }
    tmux.install_hook();
    tmux.kill_session(legitimate_parent);

    wait_until(Duration::from_secs(2), || {
        !tmux.session_names().iter().any(|name| {
            name == "ezm-project-0123456789ab__popup_slot_1"
                || name == "ezm-project-0123456789ab__mode_slot_1"
                || name == "ezm-project-0123456789ab__mode_cache"
        })
    });
    assert!(tmux.session_names().iter().any(|name| name == "survivor"));

    for parent in [
        "ezm-$(tmux set-option -g @ezm_popup_hook_private_sentinel pwned)-0123456789ab",
        "ezm-`tmux set-option -g @ezm_popup_hook_private_sentinel pwned`-0123456789ab",
        "ezm-quote'0123456789ab",
        "ezm-quote\"0123456789ab",
        "ezm-back\\slash-0123456789ab",
        "ezm-space name-0123456789ab",
        "ezm-meta;name-0123456789ab",
    ] {
        assert_adversarial_parent_is_untouched(parent);
    }
}

fn assert_adversarial_parent_is_untouched(parent: &str) {
    let tmux = IsolatedTmux::new(short_private_socket());
    tmux.set_global_option(PRIVATE_SENTINEL_OPTION, "clean");
    let actual_parent = tmux.new_session(parent);
    let requested_helper = format!("{actual_parent}__popup_slot_1");
    let actual_helper = tmux.new_session(&requested_helper);
    tmux.install_hook();
    tmux.kill_session(&actual_parent);

    sleep(Duration::from_millis(100));
    assert!(
        tmux.session_names()
            .iter()
            .any(|name| name == &actual_helper),
        "cleanup command touched {actual_parent}: {:?}",
        tmux.session_names()
    );
    assert_eq!(
        tmux.global_option(PRIVATE_SENTINEL_OPTION),
        "clean",
        "crafted session name executed the cleanup payload: {actual_parent}"
    );
}

fn short_private_socket() -> PathBuf {
    let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::Relaxed);
    PathBuf::from(format!("/tmp/ezm-popup-hook-{}-{id}", std::process::id()))
}

struct IsolatedTmux {
    socket: PathBuf,
    tmux_context: String,
}

impl IsolatedTmux {
    fn new(socket: PathBuf) -> Self {
        let tmux_context = format!("{},99999,0", socket.display());
        let tmux = Self {
            socket,
            tmux_context,
        };
        tmux.run(&["new-session", "-d", "-s", "survivor"]);
        tmux
    }

    fn run(&self, args: &[&str]) -> Output {
        let output = self.run_raw(args);
        assert!(
            output.status.success(),
            "tmux {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn run_raw(&self, args: &[&str]) -> Output {
        Command::new("tmux")
            .arg("-S")
            .arg(&self.socket)
            .arg("-f")
            .arg("/dev/null")
            .args(args)
            .env("TMUX", &self.tmux_context)
            .output()
            .expect("tmux command")
    }

    fn install_hook(&self) {
        let args = popup_parent_cleanup_hook_install_command_for_name("session-closed[0]");
        let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
        self.run(&refs);
    }

    fn set_hook(&self, hook_name: &str, command: &str) {
        self.run(&["set-hook", "-g", hook_name, command]);
    }

    fn show_hooks(&self) -> String {
        let output = self.run(&["show-hooks", "-g", "session-closed"]);
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn reconcile_hook(&self) {
        reconcile_popup_parent_cleanup_hook_with_runner(
            || Ok(self.run_raw(&["show-hooks", "-g", "session-closed"])),
            |args| {
                let output = self.run_raw(args);
                assert!(
                    output.status.success(),
                    "tmux {:?} failed: {}",
                    args,
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
            },
        )
        .expect("popup cleanup hook reconciliation");
    }

    fn set_global_option(&self, name: &str, value: &str) {
        self.run(&["set-option", "-g", name, value]);
    }

    fn global_option(&self, name: &str) -> String {
        let output = self.run(&["show-options", "-gqv", name]);
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn new_session(&self, requested_name: &str) -> String {
        let output = self.run(&[
            "new-session",
            "-d",
            "-s",
            requested_name,
            "-P",
            "-F",
            "#{session_name}",
        ]);
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn kill_session(&self, name: &str) {
        let session_id = self
            .session_names_with_ids()
            .into_iter()
            .find_map(|(session_id, session_name)| (session_name == name).then_some(session_id))
            .unwrap_or_else(|| panic!("session not found: {name}"));
        self.run(&["kill-session", "-t", &session_id]);
    }

    fn session_names(&self) -> Vec<String> {
        self.session_names_with_ids()
            .into_iter()
            .map(|(_, name)| name)
            .collect()
    }

    fn session_names_with_ids(&self) -> Vec<(String, String)> {
        let output = self.run(&["list-sessions", "-F", "#{session_id}|#{session_name}"]);
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let (session_id, session_name) = line.split_once('|')?;
                Some((session_id.to_owned(), session_name.to_owned()))
            })
            .collect()
    }
}

impl Drop for IsolatedTmux {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .arg("-S")
            .arg(&self.socket)
            .arg("-f")
            .arg("/dev/null")
            .arg("kill-server")
            .env("TMUX", &self.tmux_context)
            .output();
    }
}

fn wait_until(timeout: Duration, predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + timeout;
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for tmux state"
        );
        sleep(Duration::from_millis(20));
    }
}
