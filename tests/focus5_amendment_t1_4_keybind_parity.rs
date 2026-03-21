#[path = "support/focus5_amendment_t1_1_red_support.rs"]
mod red_support;
mod support;

use std::fs;

use red_support::{
    SlotSnapshot, extract_stdout_field, parse_switch_table, paths_equivalent, read_slot_snapshot,
};
use support::foundation_harness::FoundationHarness;

#[test]
fn t1_4_restores_focus_and_core_runtime_keybind_matrix_on_create_and_attach_paths() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-4")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let create = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("create-path launch failed: {error}"));
    let create_action = extract_stdout_field(&create.stdout, "session_action").unwrap_or_default();
    let create_session = extract_stdout_field(&create.stdout, "session").unwrap_or_default();
    let create_matrix = read_keybind_matrix(&harness)
        .unwrap_or_else(|error| panic!("failed reading create-path keybind matrix: {error}"));

    let attach = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("attach-path launch failed: {error}"));
    let attach_action = extract_stdout_field(&attach.stdout, "session_action").unwrap_or_default();
    let attach_session = extract_stdout_field(&attach.stdout, "session").unwrap_or_default();
    let attach_matrix = read_keybind_matrix(&harness)
        .unwrap_or_else(|error| panic!("failed reading attach-path keybind matrix: {error}"));

    let evidence = vec![
        format!("create_exit_code={}", create.exit_code),
        format!("create_action={create_action}"),
        format!("create_session={create_session}"),
        format!("attach_exit_code={}", attach.exit_code),
        format!("attach_action={attach_action}"),
        format!("attach_session={attach_session}"),
        format!(
            "create_focus_prefix_route_present={}",
            create_matrix.focus_prefix_route_present
        ),
        format!(
            "create_focus_slot_route_present={}",
            create_matrix.focus_slot_route_present
        ),
        format!(
            "create_core_runtime_routes_present={}",
            create_matrix.core_runtime_routes_present
        ),
        format!(
            "create_internal_route_shell_safe={}",
            create_matrix.internal_route_shell_safe
        ),
        format!(
            "attach_focus_prefix_route_present={}",
            attach_matrix.focus_prefix_route_present
        ),
        format!(
            "attach_focus_slot_route_present={}",
            attach_matrix.focus_slot_route_present
        ),
        format!(
            "attach_core_runtime_routes_present={}",
            attach_matrix.core_runtime_routes_present
        ),
        format!(
            "attach_internal_route_shell_safe={}",
            attach_matrix.internal_route_shell_safe
        ),
        format!("create_prefix_f_binding={}", create_matrix.prefix_f_binding),
        format!(
            "create_focus_slot_binding={}",
            create_matrix.focus_slot_binding
        ),
        format!("create_mode_binding={}", create_matrix.mode_binding),
        format!("create_popup_binding={}", create_matrix.popup_binding),
        format!("attach_prefix_f_binding={}", attach_matrix.prefix_f_binding),
        format!(
            "attach_focus_slot_binding={}",
            attach_matrix.focus_slot_binding
        ),
        format!("attach_mode_binding={}", attach_matrix.mode_binding),
        format!("attach_popup_binding={}", attach_matrix.popup_binding),
    ];
    write_green_cluster_evidence(&harness, "t1-4-keybind-matrix", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.4 keybind evidence: {error}"));

    let pass = create.exit_code == 0
        && attach.exit_code == 0
        && create_action == "create"
        && attach_action == "attach"
        && !create_session.is_empty()
        && create_session == attach_session
        && create_matrix.focus_prefix_route_present
        && create_matrix.focus_slot_route_present
        && create_matrix.core_runtime_routes_present
        && create_matrix.internal_route_shell_safe
        && attach_matrix.focus_prefix_route_present
        && attach_matrix.focus_slot_route_present
        && attach_matrix.core_runtime_routes_present
        && attach_matrix.internal_route_shell_safe;

    assert!(
        pass,
        "T-1.4 keybind matrix parity restoration failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn t1_4_prefix_f_focus_flow_is_deterministic_on_create_and_attach_paths() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-4")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let create = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("create-path launch failed: {error}"));
    let create_action = extract_stdout_field(&create.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&create.stdout, "session").unwrap_or_default();

    let before_create_slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading create-path slots: {error}"));
    let create_target_slot = 3_u8;
    let create_target_pane = slot_pane_id(&before_create_slots, create_target_slot);
    let create_focus = run_focus_route(&harness, &session, create_target_slot)
        .unwrap_or_else(|error| panic!("failed executing create-path focus route: {error}"));
    let selected_after_create = selected_pane_id(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading selected pane after create focus: {error}"));
    let after_create_slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading post-create-focus slots: {error}"));
    let create_mapping_stable = slot_snapshots_match(&before_create_slots, &after_create_slots);

    let attach = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("attach-path launch failed: {error}"));
    let attach_action = extract_stdout_field(&attach.stdout, "session_action").unwrap_or_default();
    let attach_session = extract_stdout_field(&attach.stdout, "session").unwrap_or_default();

    let before_attach_slots = read_slot_snapshot(&harness, &attach_session)
        .unwrap_or_else(|error| panic!("failed reading attach-path slots: {error}"));
    let attach_target_slot = 5_u8;
    let attach_target_pane = slot_pane_id(&before_attach_slots, attach_target_slot);
    let attach_focus = run_focus_route(&harness, &attach_session, attach_target_slot)
        .unwrap_or_else(|error| panic!("failed executing attach-path focus route: {error}"));
    let selected_after_attach = selected_pane_id(&harness, &attach_session)
        .unwrap_or_else(|error| panic!("failed reading selected pane after attach focus: {error}"));
    let attach_repeat_focus = run_focus_route(&harness, &attach_session, attach_target_slot)
        .unwrap_or_else(|error| {
            panic!("failed repeating attach-path focus route for determinism: {error}")
        });
    let selected_after_attach_repeat =
        selected_pane_id(&harness, &attach_session).unwrap_or_else(|error| {
            panic!("failed reading selected pane after repeated focus: {error}")
        });
    let after_attach_slots = read_slot_snapshot(&harness, &attach_session)
        .unwrap_or_else(|error| panic!("failed reading post-attach-focus slots: {error}"));
    let attach_mapping_stable = slot_snapshots_match(&before_attach_slots, &after_attach_slots);

    let evidence = vec![
        format!("create_exit_code={}", create.exit_code),
        format!("create_action={create_action}"),
        format!("session={session}"),
        format!("create_target_slot={create_target_slot}"),
        format!("create_target_pane={create_target_pane}"),
        format!("create_focus_exit_code={}", create_focus.exit_code),
        format!("create_focus_stdout={}", create_focus.stdout.trim()),
        format!("create_focus_stderr={}", create_focus.stderr.trim()),
        format!("selected_after_create={selected_after_create}"),
        format!("create_mapping_stable={create_mapping_stable}"),
        format!("attach_exit_code={}", attach.exit_code),
        format!("attach_action={attach_action}"),
        format!("attach_session={attach_session}"),
        format!("attach_target_slot={attach_target_slot}"),
        format!("attach_target_pane={attach_target_pane}"),
        format!("attach_focus_exit_code={}", attach_focus.exit_code),
        format!("attach_focus_stdout={}", attach_focus.stdout.trim()),
        format!("attach_focus_stderr={}", attach_focus.stderr.trim()),
        format!("selected_after_attach={selected_after_attach}"),
        format!(
            "attach_repeat_focus_exit_code={}",
            attach_repeat_focus.exit_code
        ),
        format!(
            "attach_repeat_focus_stdout={}",
            attach_repeat_focus.stdout.trim()
        ),
        format!(
            "attach_repeat_focus_stderr={}",
            attach_repeat_focus.stderr.trim()
        ),
        format!("selected_after_attach_repeat={selected_after_attach_repeat}"),
        format!("attach_mapping_stable={attach_mapping_stable}"),
    ];
    write_green_cluster_evidence(&harness, "t1-4-focus-flow", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.4 focus flow evidence: {error}"));

    let pass = create.exit_code == 0
        && create_action == "create"
        && !session.is_empty()
        && create_focus.exit_code == 0
        && selected_after_create == create_target_pane
        && create_mapping_stable
        && attach.exit_code == 0
        && attach_action == "attach"
        && attach_session == session
        && attach_focus.exit_code == 0
        && selected_after_attach == attach_target_pane
        && attach_repeat_focus.exit_code == 0
        && selected_after_attach_repeat == attach_target_pane
        && attach_mapping_stable;

    assert!(
        pass,
        "T-1.4 prefix+f+<slot> deterministic focus parity failed:\n{}",
        evidence.join("\n")
    );
}

#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)]
struct KeybindMatrix {
    prefix_f_binding: String,
    focus_slot_binding: String,
    mode_binding: String,
    popup_binding: String,
    focus_prefix_route_present: bool,
    focus_slot_route_present: bool,
    core_runtime_routes_present: bool,
    internal_route_shell_safe: bool,
}

fn read_keybind_matrix(harness: &FoundationHarness) -> Result<KeybindMatrix, String> {
    let prefix_f_binding = harness
        .tmux_capture(&["list-keys", "-T", "prefix", "f"])?
        .trim()
        .to_owned();
    let focus_table = parse_switch_table(&prefix_f_binding);
    let focus_slot_binding = focus_table
        .as_deref()
        .map(|table| harness.tmux_capture(&["list-keys", "-T", table, "1"]))
        .transpose()?
        .unwrap_or_default()
        .trim()
        .to_owned();
    let mode_binding = harness
        .tmux_capture(&["list-keys", "-T", "prefix", "u"])?
        .trim()
        .to_owned();
    let popup_binding = harness
        .tmux_capture(&["list-keys", "-T", "prefix", "P"])?
        .trim()
        .to_owned();

    let focus_prefix_route_present = focus_table.is_some();
    let focus_slot_route_present =
        focus_slot_binding.contains("__internal focus") && focus_slot_binding.contains("--slot 1");
    let internal_route_shell_safe = focus_slot_binding.contains("__internal focus")
        && mode_binding.contains("__internal mode")
        && popup_binding.contains("__internal popup")
        && !focus_slot_binding.contains("'#{session_name}'")
        && !mode_binding.contains("'#{session_name}'")
        && !mode_binding.contains("'#{@ezm_slot_id}'")
        && !popup_binding.contains("'#{session_name}'")
        && !popup_binding.contains("'#{@ezm_slot_id}'");

    let core_checks = [
        ("prefix", "g", "ezm-swap"),
        ("prefix", "u", "__internal mode"),
        ("prefix", "a", "--mode agent"),
        ("prefix", "S", "--mode shell"),
        ("prefix", "N", "--mode neovim"),
        ("prefix", "G", "--mode lazygit"),
        ("prefix", "P", "__internal popup"),
        ("ezm-swap", "1", "__internal swap"),
    ];
    let core_runtime_routes_present = core_checks.iter().all(|(table, key, marker)| {
        harness
            .tmux_capture(&["list-keys", "-T", table, key])
            .unwrap_or_default()
            .contains(marker)
    });

    Ok(KeybindMatrix {
        prefix_f_binding,
        focus_slot_binding,
        mode_binding,
        popup_binding,
        focus_prefix_route_present,
        focus_slot_route_present,
        core_runtime_routes_present,
        internal_route_shell_safe,
    })
}

fn run_focus_route(
    harness: &FoundationHarness,
    session: &str,
    slot_id: u8,
) -> Result<support::foundation_harness::CmdOutput, String> {
    let slot_id_arg = slot_id.to_string();
    let args = [
        "__internal",
        "focus",
        "--session",
        session,
        "--slot",
        slot_id_arg.as_str(),
    ];
    harness.run_ezm(&args, &[], 0)
}

fn selected_pane_id(harness: &FoundationHarness, session: &str) -> Result<String, String> {
    harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .map(|value| value.trim().to_owned())
}

fn slot_pane_id(slots: &[SlotSnapshot], slot_id: u8) -> String {
    slots
        .iter()
        .find(|slot| slot.slot_id == slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default()
}

fn slot_snapshots_match(left: &[SlotSnapshot], right: &[SlotSnapshot]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right.iter()).all(|(lhs, rhs)| {
            lhs.slot_id == rhs.slot_id
                && lhs.pane_id == rhs.pane_id
                && paths_equivalent(&lhs.worktree, &rhs.worktree)
        })
}

fn write_green_cluster_evidence(
    harness: &FoundationHarness,
    cluster: &str,
    evidence: &[String],
) -> Result<(), String> {
    let dir = harness.artifact_dir.join("triage-green");
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed creating triage-green evidence directory: {error}"))?;
    fs::write(dir.join(format!("{cluster}.txt")), evidence.join("\n"))
        .map_err(|error| format!("failed writing triage-green evidence file: {error}"))
}
