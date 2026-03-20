use std::fs;

use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, SessionSnapshot, extract_stdout_field, map_settle, normalize_existing_path,
    paths_equivalent, prepare_fresh_create_path, read_slot_snapshot, sample, settle_snapshot,
};

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
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

    let slot_id = 4_u8;
    let slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-07 failed reading slot snapshot: {error}"));
    let slot_pane = slots
        .iter()
        .find(|slot| slot.slot_id == slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    let popup_cwd_path = harness.work_dir().join("e2e07-popup-cwd");
    fs::create_dir_all(&popup_cwd_path)
        .unwrap_or_else(|error| panic!("E2E-07 failed creating popup cwd fixture: {error}"));
    let popup_cwd = normalize_existing_path(&popup_cwd_path)
        .unwrap_or_else(|| popup_cwd_path.display().to_string());
    let slot_cwd_key = format!("@ezm_slot_{slot_id}_cwd");
    harness
        .tmux_capture(&["set-option", "-t", &session, &slot_cwd_key, &popup_cwd])
        .unwrap_or_else(|error| panic!("E2E-07 failed setting slot cwd fixture: {error}"));

    harness
        .tmux_capture(&["select-pane", "-t", &slot_pane])
        .unwrap_or_else(|error| panic!("E2E-07 failed selecting originating pane: {error}"));

    let slot_id_text = slot_id.to_string();
    let open_args = vec![
        "__internal",
        "popup",
        "--session",
        &session,
        "--slot",
        &slot_id_text,
    ];
    let open = harness
        .run_ezm(&open_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-07 popup open failed to execute: {error}"));
    samples.push(sample(&open_args, &open));

    let popup_session = format!("{session}__popup_slot_{slot_id}");
    let popup_exists_after_open = harness
        .tmux_capture(&["has-session", "-t", &popup_session])
        .is_ok();
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

    let close = harness
        .run_ezm(&open_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-07 popup close failed to execute: {error}"));
    samples.push(sample(&open_args, &close));

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

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!("open exit_code = {}", open.exit_code));
    assertions.push(format!("close exit_code = {}", close.exit_code));
    assertions.push(format!(
        "popup helper session exists after open = {popup_exists_after_open}"
    ));
    assertions.push(format!(
        "popup helper session removed after close = {}",
        !popup_exists_after_close
    ));
    assertions.push(format!(
        "popup cwd matches slot cwd fixture = {}",
        paths_equivalent(&popup_pane_cwd, &popup_cwd)
    ));
    assertions.push(format!("popup width pct = {popup_width}"));
    assertions.push(format!("popup height pct = {popup_height}"));
    assertions.push(format!(
        "focus returns to originating pane after close = {}",
        selected_after_close == slot_pane
    ));

    let settle = settle_snapshot(harness, "E2E-07");

    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && open.exit_code == 0
        && close.exit_code == 0
        && popup_exists_after_open
        && !popup_exists_after_close
        && paths_equivalent(&popup_pane_cwd, &popup_cwd)
        && popup_width == "70"
        && popup_height == "70"
        && selected_after_close == slot_pane
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
    }
}
