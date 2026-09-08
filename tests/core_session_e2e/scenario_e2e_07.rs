use std::fs;
use std::time::Duration;

use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, DEFAULT_POLL_INTERVAL, DEFAULT_TIMEOUT, SessionSnapshot, check_key_binding,
    extract_stdout_field, map_settle, normalize_existing_path, paths_equivalent, poll_until,
    prepare_fresh_create_path, read_slot_snapshot, sample, settle_snapshot,
};

const POPUP_TRANSITION_TIMEOUT: Duration = Duration::from_secs(10);

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    fn stage(message: &str) {
        eprintln!("core_session_e2e: E2E-07 {message}");
    }

    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-07 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-07 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-07 failed reading slot snapshot: {error}"));
    let slot_pane = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let slot_id = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{@ezm_slot_id}",
        ])
        .ok()
        .and_then(|raw| raw.trim().parse::<u8>().ok())
        .filter(|slot| (1..=5).contains(slot))
        .unwrap_or(2);

    let popup_cwd_path = harness.work_dir().join("e2e07-popup-cwd");
    fs::create_dir_all(&popup_cwd_path)
        .unwrap_or_else(|error| panic!("E2E-07 failed creating popup cwd fixture: {error}"));
    let popup_cwd = normalize_existing_path(&popup_cwd_path)
        .unwrap_or_else(|| popup_cwd_path.display().to_string());
    let slot_cwd_key = format!("@ezm_slot_{slot_id}_cwd");
    harness
        .tmux_capture(&["set-option", "-t", &session, &slot_cwd_key, &popup_cwd])
        .unwrap_or_else(|error| panic!("E2E-07 failed setting slot cwd fixture: {error}"));

    let popup_keybind_check = check_key_binding(harness, "prefix", "P", &["__internal popup"]);
    let popup_keybind_present = popup_keybind_check.pass;
    assertions.push(popup_keybind_check.detail);
    assertions.push(format!(
        "popup keybind prefix+P routes to internal popup runtime = {popup_keybind_present}"
    ));

    let mut client = harness
        .spawn_tmux_client(&session, 24, 80)
        .unwrap_or_else(|error| panic!("E2E-07 failed attaching real tmux client: {error}"));
    stage("PTY client attached");
    let client_tty = client.client_tty().to_owned();
    harness
        .tmux_capture(&["set-option", "-t", &session, &slot_cwd_key, &popup_cwd])
        .unwrap_or_else(|error| panic!("E2E-07 failed refreshing slot cwd fixture: {error}"));

    client.send_prefix_key("P").unwrap_or_else(|error| {
        panic!("E2E-07 failed sending popup open key through PTY: {error}")
    });
    stage("popup open key sent");

    let popup_session = format!("{session}__popup_slot_{slot_id}");
    let popup_exists_after_open =
        poll_until(POPUP_TRANSITION_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
            Ok(harness
                .tmux_capture(&["has-session", "-t", &popup_session])
                .is_ok())
        })
        .unwrap_or_else(|error| panic!("E2E-07 failed polling popup open state: {error}"));
    stage("popup helper open state checked");

    let popup_visible_after_open =
        poll_until(POPUP_TRANSITION_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
            Ok(popup_session_has_attached_client(harness, &popup_session))
        })
        .unwrap_or_else(|error| panic!("E2E-07 failed polling popup visibility state: {error}"));
    stage("popup open state observed");
    stage("reading popup metadata");
    let popup_recorded_cwd = harness
        .tmux_capture(&["show-options", "-v", "-t", &popup_session, "@ezm_popup_cwd"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let popup_pane_cwd = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{popup_session}:0.0"),
            "#{pane_current_path}",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let popup_pane_pid_after_open = harness
        .tmux_capture(&["list-panes", "-t", &popup_session, "-F", "#{pane_pid}"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    stage("popup metadata read");
    let popup_width = harness
        .tmux_capture(&["show-options", "-v", "-t", &session, "@ezm_popup_width_pct"])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let popup_height = harness
        .tmux_capture(&[
            "show-options",
            "-v",
            "-t",
            &session,
            "@ezm_popup_height_pct",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();

    client.send_prefix_key("P").unwrap_or_else(|error| {
        panic!("E2E-07 failed sending popup close key through PTY: {error}")
    });
    stage("popup close key sent");

    let popup_not_visible_after_close =
        poll_until(POPUP_TRANSITION_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
            Ok(!popup_session_has_attached_client(harness, &popup_session))
        })
        .unwrap_or_else(|error| {
            panic!("E2E-07 failed polling popup close visibility state: {error}")
        });
    stage("popup close state observed");
    let popup_exists_after_close = harness
        .tmux_capture(&["has-session", "-t", &popup_session])
        .is_ok();
    let selected_after_close = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();

    client.send_prefix_key("P").unwrap_or_else(|error| {
        panic!("E2E-07 failed sending popup reopen key through PTY: {error}")
    });
    stage("popup reopen key sent");
    let popup_exists_before_parent_kill =
        poll_until(POPUP_TRANSITION_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
            Ok(harness
                .tmux_capture(&["has-session", "-t", &popup_session])
                .is_ok())
        })
        .unwrap_or_else(|error| panic!("E2E-07 failed polling popup reopen state: {error}"));
    let popup_visible_after_reopen =
        poll_until(POPUP_TRANSITION_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
            Ok(popup_session_has_attached_client(harness, &popup_session))
        })
        .unwrap_or_else(|error| panic!("E2E-07 failed polling popup reopen visibility: {error}"));
    stage("popup reopen state observed");
    let popup_pane_pid_after_reopen = harness
        .tmux_capture(&["list-panes", "-t", &popup_session, "-F", "#{pane_pid}"])
        .unwrap_or_default()
        .trim()
        .to_owned();

    let terminal_output = client.terminal_output();
    let popup_key_path_clean = !terminal_output.contains("if-shell: too many arguments");
    let popup_key_path_used_no_fallback = popup_exists_after_open
        && popup_visible_after_open
        && popup_not_visible_after_close
        && popup_exists_before_parent_kill
        && popup_visible_after_reopen;
    let popup_failure_diagnostics = if popup_visible_after_open
        && popup_not_visible_after_close
        && popup_visible_after_reopen
    {
        None
    } else {
        Some(format!(
            "client_tty={client_tty}; clients={}; panes={}; options={}; terminal_output={terminal_output:?}",
            harness
                .tmux_capture(&["list-clients", "-F", "#{session_name}|#{client_tty}"])
                .unwrap_or_else(|error| format!("<unavailable: {error}>")),
            harness
                .tmux_capture(&[
                    "list-panes",
                    "-a",
                    "-F",
                    "#{session_name}:#{pane_id}|#{pane_current_path}"
                ])
                .unwrap_or_else(|error| format!("<unavailable: {error}>")),
            harness
                .tmux_capture(&["show-options", "-t", &session])
                .unwrap_or_else(|error| format!("<unavailable: {error}>")),
        ))
    };
    stage("dropping PTY client");
    drop(client);
    stage("PTY client dropped");

    stage("killing parent session");
    harness
        .tmux_capture(&["kill-session", "-t", &session])
        .unwrap_or_else(|error| panic!("E2E-07 failed killing parent session: {error}"));
    stage("parent session killed");

    let popup_removed_after_parent_kill =
        poll_until(DEFAULT_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
            Ok(harness
                .tmux_capture(&["has-session", "-t", &popup_session])
                .is_err())
        })
        .unwrap_or_else(|error| {
            panic!("E2E-07 failed polling popup cleanup after parent kill: {error}")
        });

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "popup helper session exists after open = {popup_exists_after_open}"
    ));
    assertions.push(format!(
        "popup visibly opens after open = {popup_visible_after_open}"
    ));
    assertions.push(format!(
        "popup helper session persists after close = {popup_exists_after_close}"
    ));
    assertions.push(format!(
        "popup visibly closes after close = {popup_not_visible_after_close}"
    ));
    assertions.push(format!(
        "popup recorded cwd matches slot cwd fixture = {}",
        paths_equivalent(&popup_recorded_cwd, &popup_cwd)
    ));
    assertions.push(format!(
        "popup pane cwd matches slot cwd fixture (best effort) = {}",
        paths_equivalent(&popup_pane_cwd, &popup_cwd)
    ));
    assertions.push(format!("popup width pct = {popup_width}"));
    assertions.push(format!("popup height pct = {popup_height}"));
    assertions.push(format!(
        "focus returns to originating pane after close = {}",
        selected_after_close == slot_pane
    ));
    assertions.push(format!(
        "popup helper session exists before parent kill = {popup_exists_before_parent_kill}"
    ));
    assertions.push(format!(
        "popup visibly opens again after reopen = {popup_visible_after_reopen}"
    ));
    assertions.push(format!(
        "prefix-P popup path used no direct fallback = {popup_key_path_used_no_fallback}"
    ));
    assertions.push(format!(
        "prefix-P popup path emitted no if-shell arity error = {popup_key_path_clean}"
    ));
    assertions.push(format!(
        "popup pane pid is stable across close/reopen = {}",
        !popup_pane_pid_after_open.is_empty()
            && popup_pane_pid_after_open == popup_pane_pid_after_reopen
    ));
    assertions.push(format!(
        "popup helper session removed after parent kill = {popup_removed_after_parent_kill}"
    ));
    if let Some(diagnostics) = popup_failure_diagnostics {
        assertions.push(format!(
            "popup failure diagnostics before cleanup: {diagnostics}"
        ));
    }

    let settle = settle_snapshot(harness, "E2E-07");

    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && popup_keybind_present
        && popup_exists_after_open
        && popup_visible_after_open
        && popup_exists_after_close
        && popup_not_visible_after_close
        && !popup_pane_pid_after_open.is_empty()
        && popup_width == "70"
        && popup_height == "70"
        && selected_after_close == slot_pane
        && popup_exists_before_parent_kill
        && popup_visible_after_reopen
        && popup_key_path_used_no_fallback
        && popup_key_path_clean
        && popup_pane_pid_after_open == popup_pane_pid_after_reopen
        && popup_removed_after_parent_kill
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-07"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: session,
            exists: session_exists,
            count: session_count,
        },
        layout: None,
        slots: Some(slots),
        remote_path: None,
        helper_state: None,
    }
}

fn popup_session_has_attached_client(harness: &FoundationHarness, popup_session: &str) -> bool {
    harness
        .tmux_capture(&["list-clients", "-F", "#{session_name}|#{client_tty}"])
        .is_ok_and(|clients| {
            clients.lines().any(|line| {
                line.split_once('|').is_some_and(|(session, tty)| {
                    session == popup_session && !tty.trim().is_empty()
                })
            })
        })
}

#[test]
fn popup_without_attached_client_does_not_claim_visibility() {
    let harness = FoundationHarness::new_for_suite("popup-no-client")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));
    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("no-client popup setup failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let slot = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{@ezm_slot_id}",
        ])
        .ok()
        .and_then(|value| value.trim().parse::<u8>().ok())
        .filter(|slot| (1..=5).contains(slot))
        .unwrap_or(1)
        .to_string();
    let popup = [
        "__internal",
        "popup",
        "--session",
        session.as_str(),
        "--slot",
        slot.as_str(),
    ];
    let _ = harness
        .run_ezm(&popup, &[], 0)
        .unwrap_or_else(|error| panic!("no-client popup invocation failed to execute: {error}"));

    let clients = harness
        .tmux_capture(&["list-clients", "-F", "#{session_name}|#{client_tty}"])
        .unwrap_or_default();
    assert!(
        clients.trim().is_empty(),
        "non-interactive popup probe unexpectedly attached a client: {clients}"
    );
}
