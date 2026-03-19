mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use ez_mux::session::resolve_session_identity;
use serde::Serialize;

use support::foundation_harness::{CmdOutput, FoundationHarness, TmuxSettleEvidence};

const CORE_IDS: [&str; 5] = ["E2E-01", "E2E-02", "E2E-03", "E2E-04", "E2E-05"];
const CENTER_WIDTH_TARGET_PCT: i32 = 38;
const CENTER_WIDTH_TOLERANCE_PCT: i32 = 3;

#[derive(Serialize)]
struct RunMetadata {
    run_id: String,
    commit_sha: String,
    os: String,
    shell: String,
    tmux_version: String,
    artifact_dir: String,
    test_ids: Vec<String>,
    pass_total: usize,
    fail_total: usize,
}

#[derive(Serialize)]
struct CommandSample {
    args: Vec<String>,
    exit_code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Serialize)]
struct SettleEvidence {
    attempts: u32,
    poll_interval_ms: u64,
    timeout_ms: u64,
    stable: bool,
    sessions: String,
    windows: String,
    panes: String,
}

#[derive(Serialize)]
struct SessionSnapshot {
    name: String,
    exists: bool,
    count: usize,
}

#[derive(Serialize)]
struct LayoutSnapshot {
    pane_count: usize,
    window_width: i32,
    center_width: i32,
    center_width_pct: i32,
    center_width_target_pct: i32,
    center_width_tolerance_pct: i32,
    center_within_tolerance: bool,
    left_column_panes: usize,
    center_column_panes: usize,
    right_column_panes: usize,
}

#[derive(Serialize)]
struct SlotSnapshot {
    slot_id: u8,
    pane_id: String,
    worktree: String,
}

#[derive(Clone)]
struct PaneGeometry {
    id: String,
    left: i32,
    width: i32,
}

struct WorktreeFixture {
    project_dir: PathBuf,
    canonical_project_dir: PathBuf,
    extra_worktrees: Vec<PathBuf>,
}

#[derive(Serialize)]
struct CaseEvidence {
    id: String,
    pass: bool,
    assertions: Vec<String>,
    samples: Vec<CommandSample>,
    settle: SettleEvidence,
    snapshot: SessionSnapshot,
    layout: Option<LayoutSnapshot>,
    slots: Option<Vec<SlotSnapshot>>,
}

#[derive(Serialize)]
struct SuiteEvidence {
    metadata: RunMetadata,
    cases: Vec<CaseEvidence>,
}

#[test]
fn core_session_e2e_suite() {
    let harness = FoundationHarness::new_for_suite("core-session-orchestration")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let cases = vec![
        case_e2e_01(&harness),
        case_e2e_02(&harness),
        case_e2e_03(&harness),
        case_e2e_04(&harness),
        case_e2e_05(&harness),
    ];

    write_case_artifacts(&harness.artifact_dir.join("cases"), &cases)
        .unwrap_or_else(|error| panic!("failed writing case evidence artifacts: {error}"));

    let pass_total = cases.iter().filter(|case| case.pass).count();
    let fail_total = cases.len() - pass_total;

    let summary = SuiteEvidence {
        metadata: RunMetadata {
            run_id: harness.run_id.clone(),
            commit_sha: read_commit_sha(harness.project_root()),
            os: std::env::consts::OS.to_owned(),
            shell: harness.shell.clone(),
            tmux_version: harness
                .tmux_version()
                .unwrap_or_else(|error| format!("unknown ({error})")),
            artifact_dir: harness.artifact_dir.display().to_string(),
            test_ids: CORE_IDS.iter().map(|id| (*id).to_string()).collect(),
            pass_total,
            fail_total,
        },
        cases,
    };

    write_json(&harness.artifact_dir.join("summary.json"), &summary)
        .unwrap_or_else(|error| panic!("failed writing summary evidence: {error}"));

    assert_eq!(
        summary.metadata.fail_total, 0,
        "core session E2E suite contains failures; inspect summary artifact"
    );
}

fn case_e2e_01(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let first = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-01 first launch failed: {error}"));
    let second = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-01 second launch failed: {error}"));

    samples.push(sample(&[], &first));
    samples.push(sample(&[], &second));

    let first_action = extract_stdout_field(&first.stdout, "session_action").unwrap_or_default();
    let second_action = extract_stdout_field(&second.stdout, "session_action").unwrap_or_default();
    let first_session = extract_stdout_field(&first.stdout, "session").unwrap_or_default();
    let second_session = extract_stdout_field(&second.stdout, "session").unwrap_or_default();

    assertions.push(format!("first action = {first_action}"));
    assertions.push(format!("second action = {second_action}"));
    assertions.push(format!("first session = {first_session}"));
    assertions.push(format!("second session = {second_session}"));
    assertions.push(format!(
        "session names match = {}",
        first_session == second_session
    ));

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-01 settle evidence failed: {error}"));

    let sessions: Vec<&str> = settle
        .sessions
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let session_count = sessions
        .iter()
        .copied()
        .filter(|name| *name == second_session)
        .count();
    let session_exists = session_count == 1;

    assertions.push(format!(
        "session appears once in tmux snapshot = {session_exists}"
    ));

    let pass = first.exit_code == 0
        && second.exit_code == 0
        && first_action == "create"
        && second_action == "attach"
        && !first_session.is_empty()
        && first_session == second_session
        && session_exists
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-01"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: second_session,
            exists: session_exists,
            count: session_count,
        },
        layout: None,
        slots: None,
    }
}

fn case_e2e_02(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-02 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-02 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        session == expected_session
    ));

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-02 settle evidence failed: {error}"));

    let (layout_snapshot, layout_assertions) = inspect_layout(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-02 failed to inspect layout: {error}"));
    assertions.extend(layout_assertions);

    let sessions: Vec<&str> = settle
        .sessions
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let session_count = sessions
        .iter()
        .copied()
        .filter(|name| *name == session)
        .count();
    let session_exists = session_count == 1;

    let pass = launch.exit_code == 0
        && launch_action == "create"
        && !session.is_empty()
        && session == expected_session
        && session_exists
        && settle.stable
        && layout_snapshot.pane_count == 5
        && layout_snapshot.center_column_panes == 1
        && layout_snapshot.left_column_panes == 2
        && layout_snapshot.right_column_panes == 2
        && layout_snapshot.center_within_tolerance;

    CaseEvidence {
        id: String::from("E2E-02"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: session,
            exists: session_exists,
            count: session_count,
        },
        layout: Some(layout_snapshot),
        slots: None,
    }
}

#[allow(clippy::too_many_lines)]
fn case_e2e_03(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let fixture = create_worktree_fixture(harness)
        .unwrap_or_else(|error| panic!("E2E-03 fixture setup failed: {error}"));
    assertions.push(format!(
        "fixture project = {}",
        fixture.canonical_project_dir.display()
    ));
    assertions.push(format!(
        "fixture extra worktrees = [{}]",
        fixture
            .extra_worktrees
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    let expected_session = prepare_fresh_create_path(harness, &fixture.project_dir)
        .unwrap_or_else(|error| panic!("E2E-03 setup failed: {error}"));

    let first = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-03 first launch failed: {error}"));
    let second = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-03 second launch failed: {error}"));

    samples.push(sample(&[], &first));
    samples.push(sample(&[], &second));

    let first_action = extract_stdout_field(&first.stdout, "session_action").unwrap_or_default();
    let second_action = extract_stdout_field(&second.stdout, "session_action").unwrap_or_default();
    let first_session = extract_stdout_field(&first.stdout, "session").unwrap_or_default();
    let second_session = extract_stdout_field(&second.stdout, "session").unwrap_or_default();
    assertions.push(format!("first action = {first_action}"));
    assertions.push(format!("second action = {second_action}"));
    assertions.push(format!("first session = {first_session}"));
    assertions.push(format!("second session = {second_session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        second_session == expected_session
    ));

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-03 settle evidence failed: {error}"));

    let first_slots = read_slot_snapshot(harness, &first_session)
        .unwrap_or_else(|error| panic!("E2E-03 failed reading first slot snapshot: {error}"));
    let second_slots = read_slot_snapshot(harness, &second_session)
        .unwrap_or_else(|error| panic!("E2E-03 failed reading second slot snapshot: {error}"));

    assertions.push(format!(
        "canonical slot count first run = {}",
        first_slots.len()
    ));
    assertions.push(format!(
        "canonical slot count second run = {}",
        second_slots.len()
    ));
    assertions.push(format!(
        "slot mapping stable across relaunch = {}",
        slot_snapshots_match(&first_slots, &second_slots)
    ));

    let pane_count = first_slots
        .iter()
        .map(|slot| slot.pane_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .len();
    let canonical_ids = first_slots
        .iter()
        .map(|slot| slot.slot_id)
        .collect::<Vec<_>>();
    let distinct_worktrees = first_slots
        .iter()
        .map(|slot| slot.worktree.clone())
        .collect::<std::collections::HashSet<_>>()
        .len();

    assertions.push(format!("unique pane ids in slot map = {pane_count}"));
    assertions.push(format!("canonical slot ids = {canonical_ids:?}"));
    assertions.push(format!(
        "distinct worktrees assigned on create path = {distinct_worktrees}"
    ));

    let expected_worktree_cycle = expected_worktree_cycle(&fixture);
    let deterministic_assignment_verified =
        expected_worktree_cycle
            .iter()
            .all(|(slot_id, expected_worktree)| {
                first_slots
                    .iter()
                    .find(|slot| slot.slot_id == *slot_id)
                    .is_some_and(|slot| slot.worktree == *expected_worktree)
            });
    assertions.push(format!(
        "deterministic slot/worktree cycle matches expected fixture mapping = {deterministic_assignment_verified}"
    ));

    let remap_target = second_slots
        .iter()
        .find(|slot| slot.slot_id == 2)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();
    assertions.push(format!(
        "remap target uses existing slot-2 pane = {remap_target}"
    ));

    let remap_attempt = harness.tmux_capture(&[
        "set-option",
        "-t",
        &second_session,
        "@ezm_slot_1_pane",
        &remap_target,
    ]);
    let remap_applied = remap_attempt.is_ok();
    assertions.push(format!(
        "direct remap overwrite accepted by tmux = {remap_applied}"
    ));

    let post_attempt_slots = read_slot_snapshot(harness, &second_session)
        .unwrap_or_else(|error| panic!("E2E-03 failed reading post-remap slots: {error}"));
    let slot1_overwritten = post_attempt_slots
        .iter()
        .find(|slot| slot.slot_id == 1)
        .is_some_and(|slot| slot.pane_id == remap_target);
    assertions.push(format!(
        "slot-1 option overwritten by remap attempt = {slot1_overwritten}"
    ));

    let third = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-03 remap validation launch failed: {error}"));
    samples.push(sample(&[], &third));

    let third_failed = third.exit_code != 0;
    let remap_diagnostic_reported = third.stderr.contains("canonical slot identity mismatch")
        || third.stdout.contains("canonical slot identity mismatch");
    assertions.push(format!(
        "remapped session rejected on relaunch = {third_failed}"
    ));
    assertions.push(format!(
        "canonical mismatch diagnostics surfaced = {remap_diagnostic_reported}"
    ));

    let session_exists = !second_session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = first.exit_code == 0
        && second.exit_code == 0
        && first_action == "create"
        && second_action == "attach"
        && !first_session.is_empty()
        && first_session == second_session
        && second_session == expected_session
        && settle.stable
        && first_slots.len() == 5
        && second_slots.len() == 5
        && pane_count == 5
        && slot_snapshots_match(&first_slots, &second_slots)
        && distinct_worktrees >= 3
        && deterministic_assignment_verified
        && remap_applied
        && slot1_overwritten
        && third_failed
        && remap_diagnostic_reported;

    CaseEvidence {
        id: String::from("E2E-03"),
        pass,
        assertions,
        samples,
        settle: map_settle(settle),
        snapshot: SessionSnapshot {
            name: second_session,
            exists: session_exists,
            count: session_count,
        },
        layout: None,
        slots: Some(first_slots),
    }
}

#[allow(clippy::too_many_lines)]
fn case_e2e_04(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-04 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-04 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        session == expected_session
    ));

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-04 settle evidence failed: {error}"));

    let before_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-04 failed reading pre-swap slots: {error}"));
    let before_geometry = read_pane_geometry(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-04 failed reading pre-swap geometry: {error}"));

    let slot_id = 1_u8;
    let slot_pane_id = before_slots
        .iter()
        .find(|slot| slot.slot_id == slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();
    let center_before = center_pane_from_geometry(&before_geometry);
    let slot_before = pane_geometry_by_id(&before_geometry, &slot_pane_id);

    assertions.push(format!("swap slot target = {slot_id}"));
    assertions.push(format!("slot pane before swap = {slot_pane_id}"));
    assertions.push(format!("center pane before swap = {center_before}"));

    let slot_id_text = slot_id.to_string();
    let swap_args = vec![
        "__internal",
        "swap",
        "--session",
        &session,
        "--slot",
        &slot_id_text,
    ];
    let swap = harness
        .run_ezm(&swap_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-04 swap invocation failed: {error}"));
    samples.push(sample(&swap_args, &swap));

    let after_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-04 failed reading post-swap slots: {error}"));
    let after_geometry = read_pane_geometry(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-04 failed reading post-swap geometry: {error}"));
    let center_after = center_pane_from_geometry(&after_geometry);
    let slot_after = pane_geometry_by_id(&after_geometry, &slot_pane_id);
    let selected_after = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .unwrap_or_else(|error| panic!("E2E-04 failed reading selected pane: {error}"))
        .trim()
        .to_owned();

    assertions.push(format!(
        "slot identity registry unchanged after swap = {}",
        slot_snapshots_match(&before_slots, &after_slots)
    ));
    assertions.push(format!("center pane after swap = {center_after}"));
    assertions.push(format!("selected pane after swap = {selected_after}"));

    let slot_moved_to_center = center_after == slot_pane_id;
    let slot_position_changed = slot_before
        .zip(slot_after)
        .is_some_and(|(before, after)| before.left != after.left);
    let selected_target_slot = selected_after == slot_pane_id;

    assertions.push(format!("slot moved to center = {slot_moved_to_center}"));
    assertions.push(format!("slot position changed = {slot_position_changed}"));
    assertions.push(format!(
        "selected pane is swapped slot = {selected_target_slot}"
    ));

    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && settle.stable
        && swap.exit_code == 0
        && slot_snapshots_match(&before_slots, &after_slots)
        && slot_moved_to_center
        && slot_position_changed
        && selected_target_slot;

    CaseEvidence {
        id: String::from("E2E-04"),
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
        slots: Some(after_slots),
    }
}

#[allow(clippy::too_many_lines)]
fn case_e2e_05(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-05 setup failed: {error}"));

    let launch = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("E2E-05 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!(
        "session matches expected identity = {}",
        session == expected_session
    ));

    let settle = harness
        .settle_tmux_snapshot(Duration::from_millis(50), Duration::from_secs(2))
        .unwrap_or_else(|error| panic!("E2E-05 settle evidence failed: {error}"));

    let before_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-05 failed reading pre-zoom slots: {error}"));
    let before_geometry = read_pane_geometry(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-05 failed reading pre-zoom geometry: {error}"));
    let center_before = center_pane_from_geometry(&before_geometry);
    let swap_slot_id = 4_u8;
    let swap_slot_pane = before_slots
        .iter()
        .find(|slot| slot.slot_id == swap_slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default();

    harness
        .tmux_capture(&["resize-pane", "-Z", "-t", &center_before])
        .unwrap_or_else(|error| panic!("E2E-05 failed to enable zoom: {error}"));

    let zoom_before = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{window_zoomed_flag}",
        ])
        .unwrap_or_else(|error| panic!("E2E-05 failed reading pre-swap zoom flag: {error}"))
        .trim()
        .to_owned();
    assertions.push(format!("zoom flag before swap = {zoom_before}"));

    let swap_slot_id_text = swap_slot_id.to_string();
    let swap_args = vec![
        "__internal",
        "swap",
        "--session",
        &session,
        "--slot",
        &swap_slot_id_text,
    ];
    let swap = harness
        .run_ezm(&swap_args, &[], 0)
        .unwrap_or_else(|error| panic!("E2E-05 swap invocation failed: {error}"));
    samples.push(sample(&swap_args, &swap));

    let zoom_after = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{window_zoomed_flag}",
        ])
        .unwrap_or_else(|error| panic!("E2E-05 failed reading post-swap zoom flag: {error}"))
        .trim()
        .to_owned();

    let selected_after = harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            &format!("{session}:0"),
            "#{pane_id}",
        ])
        .unwrap_or_else(|error| panic!("E2E-05 failed reading selected pane after swap: {error}"))
        .trim()
        .to_owned();

    assertions.push(format!("zoom flag after swap = {zoom_after}"));
    assertions.push(format!(
        "selected pane after zoomed swap = {selected_after}"
    ));

    harness
        .tmux_capture(&["resize-pane", "-Z", "-t", &selected_after])
        .unwrap_or_else(|error| panic!("E2E-05 failed to disable zoom: {error}"));

    let after_geometry = read_pane_geometry(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-05 failed reading post-unzoom geometry: {error}"));
    let after_slots = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-05 failed reading post-swap slots: {error}"));
    let center_after = center_pane_from_geometry(&after_geometry);

    let zoom_preserved = zoom_before == "1" && zoom_after == "1";
    let selected_target_slot = selected_after == swap_slot_pane;
    let swapped_slot_now_center = center_after == swap_slot_pane;

    assertions.push(format!(
        "zoom preserved across swap/select = {zoom_preserved}"
    ));
    assertions.push(format!(
        "selected pane is slot-{swap_slot_id} = {selected_target_slot}"
    ));
    assertions.push(format!(
        "slot-{swap_slot_id} moved to center after unzoom = {swapped_slot_now_center}"
    ));
    assertions.push(format!(
        "slot identity registry unchanged after zoomed swap = {}",
        slot_snapshots_match(&before_slots, &after_slots)
    ));

    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && settle.stable
        && swap.exit_code == 0
        && zoom_preserved
        && selected_target_slot
        && swapped_slot_now_center
        && slot_snapshots_match(&before_slots, &after_slots);

    CaseEvidence {
        id: String::from("E2E-05"),
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
        slots: Some(after_slots),
    }
}

fn sample(args: &[&str], output: &CmdOutput) -> CommandSample {
    CommandSample {
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
        exit_code: output.exit_code,
        stdout: output.stdout.clone(),
        stderr: output.stderr.clone(),
    }
}

fn map_settle(settle: TmuxSettleEvidence) -> SettleEvidence {
    SettleEvidence {
        attempts: settle.attempts,
        poll_interval_ms: settle.poll_interval_ms,
        timeout_ms: settle.timeout_ms,
        stable: settle.stable,
        sessions: settle.sessions,
        windows: settle.windows,
        panes: settle.panes,
    }
}

fn extract_stdout_field(stdout: &str, key: &str) -> Option<String> {
    let marker = format!("{key}=");
    let start = stdout.find(&marker)? + marker.len();
    let tail = &stdout[start..];
    let end = tail.find(';').unwrap_or(tail.len());
    Some(tail[..end].trim().trim_end_matches('.').to_owned())
}

#[allow(clippy::too_many_lines)]
fn inspect_layout(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<(LayoutSnapshot, Vec<String>), String> {
    let window_width_raw = harness.tmux_capture(&[
        "display-message",
        "-p",
        "-t",
        &format!("{session_name}:0"),
        "#{window_width}",
    ])?;
    let window_width = window_width_raw
        .trim()
        .parse::<i32>()
        .map_err(|error| format!("invalid window width `{window_width_raw}`: {error}"))?;

    let pane_dump = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session_name}:0"),
        "-F",
        "#{pane_id}|#{pane_width}|#{pane_height}|#{pane_left}",
    ])?;

    let mut panes = Vec::new();
    for line in pane_dump
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split('|');
        let pane_id = parts.next().unwrap_or_default().to_owned();
        let pane_width = parts
            .next()
            .ok_or_else(|| format!("missing pane width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane width in `{line}`: {error}"))?;
        let pane_height = parts
            .next()
            .ok_or_else(|| format!("missing pane height in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane height in `{line}`: {error}"))?;
        let pane_left = parts
            .next()
            .ok_or_else(|| format!("missing pane left in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane left in `{line}`: {error}"))?;
        panes.push((pane_id, pane_width, pane_height, pane_left));
    }

    let max_height = panes
        .iter()
        .map(|(_, _, height, _)| *height)
        .max()
        .unwrap_or(0);

    let mut columns = std::collections::BTreeMap::<i32, Vec<(String, i32, i32)>>::new();
    for (pane_id, pane_width, pane_height, pane_left) in &panes {
        columns
            .entry(*pane_left)
            .or_default()
            .push((pane_id.clone(), *pane_width, *pane_height));
    }

    let left_column_panes = columns.values().next().map_or(0, std::vec::Vec::len);
    let right_column_panes = columns.values().last().map_or(0, std::vec::Vec::len);

    let mut center_width = 0;
    let mut center_column_panes = 0;
    for panes_in_column in columns.values() {
        if panes_in_column.len() == 1 {
            center_column_panes = 1;
            center_width = panes_in_column[0].1;
            if panes_in_column[0].2 < max_height {
                center_column_panes = 0;
            }
            break;
        }
    }

    let center_width_pct = if window_width > 0 {
        (center_width * 100) / window_width
    } else {
        0
    };
    let delta = (center_width_pct - CENTER_WIDTH_TARGET_PCT).abs();
    let center_within_tolerance = delta <= CENTER_WIDTH_TOLERANCE_PCT;

    let assertions = vec![
        format!("pane count = {}", panes.len()),
        format!("window width = {window_width}"),
        format!("center width = {center_width}"),
        format!(
            "center width pct = {center_width_pct} (target={} +/- {})",
            CENTER_WIDTH_TARGET_PCT, CENTER_WIDTH_TOLERANCE_PCT
        ),
        format!(
            "left/center/right panes = {left_column_panes}/{center_column_panes}/{right_column_panes}"
        ),
        format!("center width within tolerance = {center_within_tolerance}"),
    ];

    Ok((
        LayoutSnapshot {
            pane_count: panes.len(),
            window_width,
            center_width,
            center_width_pct,
            center_width_target_pct: CENTER_WIDTH_TARGET_PCT,
            center_width_tolerance_pct: CENTER_WIDTH_TOLERANCE_PCT,
            center_within_tolerance,
            left_column_panes,
            center_column_panes,
            right_column_panes,
        },
        assertions,
    ))
}

fn read_slot_snapshot(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<Vec<SlotSnapshot>, String> {
    let mut slots = Vec::new();
    for slot_id in 1_u8..=5 {
        let pane_key = format!("@ezm_slot_{slot_id}_pane");
        let worktree_key = format!("@ezm_slot_{slot_id}_worktree");

        let pane_id = harness
            .tmux_capture(&["show-options", "-v", "-t", session_name, &pane_key])?
            .trim()
            .to_owned();
        let worktree = harness
            .tmux_capture(&["show-options", "-v", "-t", session_name, &worktree_key])?
            .trim()
            .to_owned();

        slots.push(SlotSnapshot {
            slot_id,
            pane_id,
            worktree,
        });
    }
    Ok(slots)
}

fn slot_snapshots_match(left: &[SlotSnapshot], right: &[SlotSnapshot]) -> bool {
    if left.len() != right.len() {
        return false;
    }

    left.iter().zip(right.iter()).all(|(lhs, rhs)| {
        lhs.slot_id == rhs.slot_id && lhs.pane_id == rhs.pane_id && lhs.worktree == rhs.worktree
    })
}

fn create_worktree_fixture(harness: &FoundationHarness) -> Result<WorktreeFixture, String> {
    let fixture_root = harness.work_dir().join("e2e03-worktree-fixture");
    let project_dir = fixture_root.join("project");
    let wt_a = fixture_root.join("wt-a");
    let wt_b = fixture_root.join("wt-b");

    if fixture_root.exists() {
        fs::remove_dir_all(&fixture_root).map_err(|error| {
            format!(
                "failed resetting fixture root {}: {error}",
                fixture_root.display()
            )
        })?;
    }
    fs::create_dir_all(&project_dir).map_err(|error| {
        format!(
            "failed creating fixture project {}: {error}",
            project_dir.display()
        )
    })?;

    run_git(&project_dir, &["init"])?;
    run_git(
        &project_dir,
        &["config", "user.email", "e2e@example.invalid"],
    )?;
    run_git(&project_dir, &["config", "user.name", "E2E Harness"])?;
    fs::write(project_dir.join("README.md"), "# fixture\n")
        .map_err(|error| format!("failed writing fixture README: {error}"))?;
    run_git(&project_dir, &["add", "README.md"])?;
    run_git(&project_dir, &["commit", "-m", "fixture init"])?;

    let primary_worktree_arg = wt_a.to_string_lossy().into_owned();
    let secondary_checkout_path = wt_b.to_string_lossy().into_owned();
    run_git(
        &project_dir,
        &["worktree", "add", "--detach", &primary_worktree_arg, "HEAD"],
    )?;
    run_git(
        &project_dir,
        &[
            "worktree",
            "add",
            "--detach",
            &secondary_checkout_path,
            "HEAD",
        ],
    )?;

    Ok(WorktreeFixture {
        project_dir: project_dir.clone(),
        canonical_project_dir: project_dir
            .canonicalize()
            .map_err(|error| format!("failed canonicalizing fixture project: {error}"))?,
        extra_worktrees: vec![
            wt_a.canonicalize()
                .map_err(|error| format!("failed canonicalizing fixture wt-a: {error}"))?,
            wt_b.canonicalize()
                .map_err(|error| format!("failed canonicalizing fixture wt-b: {error}"))?,
        ],
    })
}

fn expected_worktree_cycle(fixture: &WorktreeFixture) -> Vec<(u8, String)> {
    let mut ordered = vec![fixture.canonical_project_dir.clone()];
    let mut extras = fixture.extra_worktrees.clone();
    extras.sort();
    ordered.extend(extras);

    (1_u8..=5)
        .enumerate()
        .map(|(index, slot_id)| {
            (
                slot_id,
                ordered[index % ordered.len()].display().to_string(),
            )
        })
        .collect()
}

fn run_git(repo_dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .map_err(|error| format!("failed running git {args:?}: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    Err(format!(
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

fn read_pane_geometry(
    harness: &FoundationHarness,
    session_name: &str,
) -> Result<Vec<PaneGeometry>, String> {
    let raw = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session_name}:0"),
        "-F",
        "#{pane_id}|#{pane_left}|#{pane_width}",
    ])?;

    let mut panes = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut parts = line.split('|');
        let pane_id = parts.next().unwrap_or_default().to_owned();
        let pane_left = parts
            .next()
            .ok_or_else(|| format!("missing pane_left in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_left in `{line}`: {error}"))?;
        let pane_width = parts
            .next()
            .ok_or_else(|| format!("missing pane_width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_width in `{line}`: {error}"))?;

        panes.push(PaneGeometry {
            id: pane_id,
            left: pane_left,
            width: pane_width,
        });
    }

    Ok(panes)
}

fn center_pane_from_geometry(geometry: &[PaneGeometry]) -> String {
    geometry
        .iter()
        .max_by_key(|pane| (pane.width, -pane.left))
        .map(|pane| pane.id.clone())
        .unwrap_or_default()
}

fn pane_geometry_by_id<'a>(
    geometry: &'a [PaneGeometry],
    pane_id: &str,
) -> Option<&'a PaneGeometry> {
    geometry.iter().find(|pane| pane.id == pane_id)
}

fn prepare_fresh_create_path(
    harness: &FoundationHarness,
    project_dir: &Path,
) -> Result<String, String> {
    let identity = resolve_session_identity(project_dir)
        .map_err(|error| format!("failed resolving expected session identity: {error}"))?;

    let _ = harness.tmux_capture(&["kill-session", "-t", &identity.session_name]);

    match harness.tmux_capture(&["has-session", "-t", &identity.session_name]) {
        Ok(_) => Err(format!(
            "expected no existing session `{}` before create-path test",
            identity.session_name
        )),
        Err(_) => Ok(identity.session_name),
    }
}

fn read_commit_sha(project_root: &Path) -> String {
    let output = std::process::Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir(project_root)
        .output();

    match output {
        Ok(result) if result.status.success() => {
            String::from_utf8_lossy(&result.stdout).trim().to_owned()
        }
        _ => String::from("unknown"),
    }
}

fn write_case_artifacts(dir: &Path, cases: &[CaseEvidence]) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|error| format!("failed creating case directory: {error}"))?;
    for case in cases {
        let path = dir.join(format!("{}.json", case.id));
        write_json(&path, case)?;
    }
    Ok(())
}

fn write_json(path: &PathBuf, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed serializing json for {path:?}: {error}"))?;
    fs::write(path, json).map_err(|error| format!("failed writing json {path:?}: {error}"))
}
