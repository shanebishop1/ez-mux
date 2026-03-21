#[path = "support/focus5_amendment_t1_1_red_support.rs"]
mod red_support;
mod support;

use red_support::{
    center_pane_id, create_worktree_fixture, extract_stdout_field, install_failing_opencode_stub,
    pane_current_command, parse_switch_table, paths_equivalent, read_pane_widths,
    read_slot_snapshot, write_cluster_evidence,
};

use support::foundation_harness::FoundationHarness;

#[test]
fn red_startup_attach_open_visibility_requires_visibility_evidence() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-red")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();

    let attached_clients = if session.is_empty() {
        String::new()
    } else {
        harness
            .tmux_capture(&["list-clients", "-t", &session, "-F", "#{client_tty}"])
            .unwrap_or_default()
    };
    let attached_client_count = attached_clients
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .count();
    let pty_probe = if session.is_empty() {
        None
    } else {
        Some(harness.run_ezm_with_pty_attach_probe(harness.project_root(), &[], &[], 0, &session))
    };
    let pty_attach_observed = pty_probe
        .as_ref()
        .and_then(|probe| probe.as_ref().ok())
        .is_some_and(|probe| probe.observed_attached_client);
    let pty_probe_exit = pty_probe
        .as_ref()
        .and_then(|probe| probe.as_ref().ok())
        .map_or(-1, |probe| probe.exit_code);
    let pty_probe_error = pty_probe
        .as_ref()
        .and_then(|probe| probe.as_ref().err())
        .cloned()
        .unwrap_or_default();
    let explicit_visibility_diagnostic = launch.stderr.contains("attach skipped")
        || launch.stderr.contains("non-interactive")
        || launch.stdout.contains("attach_visibility")
        || launch.stdout.contains("attach_observed");

    let evidence = vec![
        format!("exit_code={} session_action={action}", launch.exit_code),
        format!("session={session}"),
        format!("attached_client_count={attached_client_count}"),
        format!("pty_attach_observed={pty_attach_observed}"),
        format!("pty_probe_exit={pty_probe_exit}"),
        format!("pty_probe_error={pty_probe_error}"),
        format!("explicit_visibility_diagnostic_present={explicit_visibility_diagnostic}"),
        format!("stdout={}", launch.stdout.trim()),
        format!("stderr={}", launch.stderr.trim()),
    ];
    write_cluster_evidence(&harness, "startup-attach-open-visibility", &evidence)
        .unwrap_or_else(|error| panic!("failed writing cluster evidence: {error}"));

    let pass = launch.exit_code == 0
        && action == "create"
        && !session.is_empty()
        && attached_client_count > 0
        && pty_attach_observed;

    assert!(
        pass,
        "startup attach/open visibility contract regression detected:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn red_startup_mode_worktree_center_slot_mapping_matches_focus5_reference() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-red")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let fixture = create_worktree_fixture(&harness)
        .unwrap_or_else(|error| panic!("fixture setup failed: {error}"));
    let launch = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("fixture startup launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot: {error}"));
    let pane_widths = read_pane_widths(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading pane widths: {error}"));
    let center_pane = center_pane_id(&pane_widths).unwrap_or_default();
    let center_slot = slots
        .iter()
        .find(|slot| slot.pane_id == center_pane)
        .map(|slot| slot.slot_id);
    let slot1_worktree = slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .map(|slot| slot.worktree.clone())
        .unwrap_or_default();
    let slot1_pane = slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();
    let slot1_command = if slot1_pane.is_empty() {
        String::new()
    } else {
        pane_current_command(&harness, &slot1_pane).unwrap_or_default()
    };
    let slot1_matches_expected = paths_equivalent(
        &slot1_worktree,
        &fixture.expected_slot1_worktree.display().to_string(),
    );
    let slot1_launches_opencode = slot1_command.contains("opencode");
    let no_negative_worktrees = slots.iter().all(|slot| slot.worktree != "-1");

    let evidence = vec![
        format!("exit_code={} session={session}", launch.exit_code),
        format!("center_pane={center_pane}"),
        format!("center_slot={center_slot:?}"),
        format!("slot1_pane={slot1_pane}"),
        format!("slot1_command={slot1_command}"),
        format!("slot1_worktree={slot1_worktree}"),
        format!(
            "expected_slot1_worktree={}",
            fixture.expected_slot1_worktree.display()
        ),
        format!("slot1_matches_expected={slot1_matches_expected}"),
        format!("slot1_launches_opencode={slot1_launches_opencode}"),
        format!("no_negative_worktrees={no_negative_worktrees}"),
    ];
    write_cluster_evidence(&harness, "startup-mode-worktree-correctness", &evidence)
        .unwrap_or_else(|error| panic!("failed writing cluster evidence: {error}"));

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && center_slot == Some(1)
        && slot1_matches_expected
        && slot1_launches_opencode
        && no_negative_worktrees;

    assert!(
        pass,
        "startup mode/worktree correctness regression detected:\n{}",
        evidence.join("\n")
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn red_style_parity_requires_center_blue_border_and_text_inheritance() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-red")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let pane_widths = read_pane_widths(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading pane widths: {error}"));
    let center_pane = center_pane_id(&pane_widths).unwrap_or_default();
    let border_style = harness
        .tmux_capture(&[
            "show-window-options",
            "-v",
            "-t",
            &format!("{session}:0"),
            "pane-border-style",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let window_style = harness
        .tmux_capture(&[
            "show-window-options",
            "-v",
            "-t",
            &format!("{session}:0"),
            "window-style",
        ])
        .unwrap_or_default()
        .trim()
        .to_owned();
    let center_pane_style = if center_pane.is_empty() {
        String::new()
    } else {
        harness
            .tmux_capture(&[
                "show-options",
                "-p",
                "-v",
                "-t",
                &center_pane,
                "window-style",
            ])
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let center_pane_active_style = if center_pane.is_empty() {
        String::new()
    } else {
        harness
            .tmux_capture(&[
                "show-options",
                "-p",
                "-v",
                "-t",
                &center_pane,
                "window-active-style",
            ])
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let center_border_label = if center_pane.is_empty() {
        String::new()
    } else {
        harness
            .tmux_capture(&[
                "show-options",
                "-p",
                "-v",
                "-t",
                &center_pane,
                "@ezm_border_label",
            ])
            .unwrap_or_default()
            .trim()
            .to_owned()
    };

    let expected_center_color = "#5ac8e0";
    let border_matches_expected_center = border_style.contains(expected_center_color);
    let pane_text_inherits_slot_color = center_pane_style.contains(expected_center_color)
        && center_pane_active_style.contains(expected_center_color);
    let connected_border_prefix_present =
        center_border_label.contains("─·① ·─") && center_border_label.contains("────────────────");

    let evidence = vec![
        format!("exit_code={} session={session}", launch.exit_code),
        format!("center_pane={center_pane}"),
        format!("pane_border_style={border_style}"),
        format!("center_pane_style={center_pane_style}"),
        format!("center_pane_active_style={center_pane_active_style}"),
        format!("center_border_label={center_border_label}"),
        format!("window_style={window_style}"),
        format!("border_matches_expected_center={border_matches_expected_center}"),
        format!("pane_text_inherits_slot_color={pane_text_inherits_slot_color}"),
        format!("connected_border_prefix_present={connected_border_prefix_present}"),
    ];
    write_cluster_evidence(&harness, "style-parity", &evidence)
        .unwrap_or_else(|error| panic!("failed writing cluster evidence: {error}"));

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && border_matches_expected_center
        && pane_text_inherits_slot_color
        && connected_border_prefix_present;

    assert!(
        pass,
        "runtime style parity regression detected:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn red_keybind_parity_requires_prefix_f_focus_route() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-red")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup launch failed: {error}"));
    let prefix_f_binding = harness
        .tmux_capture(&["list-keys", "-T", "prefix", "f"])
        .unwrap_or_default();
    let focus_table = parse_switch_table(&prefix_f_binding);
    let slot_one_binding = focus_table
        .as_deref()
        .map(|table| {
            harness
                .tmux_capture(&["list-keys", "-T", table, "1"])
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let focus_route_present = focus_table.is_some();
    let focus_slot_route_present =
        slot_one_binding.contains("--slot 1") && slot_one_binding.contains("__internal focus");

    let evidence = vec![
        format!("exit_code={}", launch.exit_code),
        format!("prefix_f_binding={}", prefix_f_binding.trim()),
        format!("focus_table={focus_table:?}"),
        format!("slot_one_binding={}", slot_one_binding.trim()),
        format!("focus_route_present={focus_route_present}"),
        format!("focus_slot_route_present={focus_slot_route_present}"),
    ];
    write_cluster_evidence(&harness, "keybind-parity", &evidence)
        .unwrap_or_else(|error| panic!("failed writing cluster evidence: {error}"));

    let pass = launch.exit_code == 0 && focus_route_present && focus_slot_route_present;

    assert!(
        pass,
        "keybind parity regression detected:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn red_local_vs_remote_diagnostics_and_failure_surfacing() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-red")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));
    install_failing_opencode_stub(&harness)
        .unwrap_or_else(|error| panic!("failed installing opencode RED stub: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let local_output_is_remote_clean = !launch.stdout.contains("remote_project_dir=")
        && !launch.stdout.contains("remote_dir_prefix=")
        && !launch.stdout.contains("opencode_attach_url=");
    let local_mode_diagnostic_present = launch.stdout.contains("routing_mode=local")
        || launch.stdout.contains("remote_routing_active=false");
    let opencode_launch_attempted =
        launch.stdout.contains("opencode") || launch.stderr.contains("opencode");
    let mode_failure_surfaced = launch.exit_code != 0
        && (launch.stderr.contains("opencode")
            || launch.stderr.contains("mode")
            || launch.stdout.contains("mode_failure"));

    let evidence = vec![
        format!("startup_exit_code={} session={session}", launch.exit_code),
        format!("local_output_is_remote_clean={local_output_is_remote_clean}"),
        format!("local_mode_diagnostic_present={local_mode_diagnostic_present}"),
        format!("opencode_launch_attempted={opencode_launch_attempted}"),
        format!("mode_failure_surfaced={mode_failure_surfaced}"),
        format!("startup_stdout={}", launch.stdout.trim()),
        format!("startup_stderr={}", launch.stderr.trim()),
    ];
    write_cluster_evidence(&harness, "local-vs-remote-diagnostics", &evidence)
        .unwrap_or_else(|error| panic!("failed writing cluster evidence: {error}"));

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && local_output_is_remote_clean
        && local_mode_diagnostic_present
        && opencode_launch_attempted
        && mode_failure_surfaced;

    assert!(
        pass,
        "local-vs-remote diagnostics/failure surfacing regression detected:\n{}",
        evidence.join("\n")
    );
}
