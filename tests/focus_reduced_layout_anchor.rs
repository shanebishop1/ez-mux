#[path = "support/focus5_amendment_t1_1_red_support.rs"]
mod red_support;
mod support;

use red_support::{extract_stdout_field, read_slot_snapshot};
use support::foundation_harness::FoundationHarness;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PaneGeometry {
    pane_id: String,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

#[test]
fn two_pane_startup_makes_main_slot_noticeably_wider() {
    let harness = FoundationHarness::new_for_suite("focus-reduced-layout")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let launch = harness
        .run_ezm(&["--verbose", "2"], &[], 0)
        .unwrap_or_else(|error| panic!("launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot: {error}"));
    let geometry = read_pane_geometry(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading pane geometry: {error}"));

    let main_pane = slot_pane_id(&slots, 1);
    let side_pane = slot_pane_id(&slots, 2);
    let main_geometry = pane_geometry_by_id(&geometry, &main_pane)
        .unwrap_or_else(|| panic!("missing main pane geometry for {main_pane}"));
    let side_geometry = pane_geometry_by_id(&geometry, &side_pane)
        .unwrap_or_else(|| panic!("missing side pane geometry for {side_pane}"));

    assert_eq!(
        launch.exit_code,
        0,
        "launch stderr: {}",
        launch.stderr.trim()
    );
    assert!(main_geometry.width > side_geometry.width);
    assert!(main_geometry.left > side_geometry.left);
}

#[test]
fn four_pane_startup_makes_main_slot_wider_and_taller() {
    let harness = FoundationHarness::new_for_suite("focus-reduced-layout")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let launch = harness
        .run_ezm(&["--verbose", "4"], &[], 0)
        .unwrap_or_else(|error| panic!("launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot: {error}"));
    let geometry = read_pane_geometry(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading pane geometry: {error}"));

    let main_pane = slot_pane_id(&slots, 1);
    let side_pane = slot_pane_id(&slots, 2);
    let lower_pane = slot_pane_id(&slots, 4);
    let main_geometry = pane_geometry_by_id(&geometry, &main_pane)
        .unwrap_or_else(|| panic!("missing main pane geometry for {main_pane}"));
    let side_geometry = pane_geometry_by_id(&geometry, &side_pane)
        .unwrap_or_else(|| panic!("missing side pane geometry for {side_pane}"));
    let lower_geometry = pane_geometry_by_id(&geometry, &lower_pane)
        .unwrap_or_else(|| panic!("missing lower pane geometry for {lower_pane}"));

    assert_eq!(
        launch.exit_code,
        0,
        "launch stderr: {}",
        launch.stderr.trim()
    );
    assert!(main_geometry.left > side_geometry.left);
    assert!(main_geometry.top < lower_geometry.top);
    assert!(main_geometry.width > side_geometry.width);
    assert!(main_geometry.height > lower_geometry.height);
}

#[test]
fn focus_promotes_slot_into_startup_main_position_in_two_pane_layout() {
    assert_focus_uses_startup_main_position("focus-reduced-layout", "2", 2);
}

#[test]
fn focus_promotes_slot_into_startup_main_position_in_four_pane_layout() {
    assert_focus_uses_startup_main_position("focus-reduced-layout", "4", 4);
}

fn assert_focus_uses_startup_main_position(suite_name: &str, pane_arg: &str, target_slot: u8) {
    let harness = FoundationHarness::new_for_suite(suite_name)
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let launch = harness
        .run_ezm(&["--verbose", pane_arg], &[], 0)
        .unwrap_or_else(|error| panic!("launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();

    let before_slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot before focus: {error}"));
    let before_geometry = read_pane_geometry(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading pane geometry before focus: {error}"));

    let startup_main_pane = slot_pane_id(&before_slots, 1);
    let startup_main_geometry = pane_geometry_by_id(&before_geometry, &startup_main_pane)
        .unwrap_or_else(|| panic!("missing startup main pane geometry for {startup_main_pane}"))
        .clone();
    let target_pane = slot_pane_id(&before_slots, target_slot);

    let focus = run_focus_route(&harness, &session, target_slot)
        .unwrap_or_else(|error| panic!("focus invocation failed: {error}"));
    let after_geometry = read_pane_geometry(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading pane geometry after focus: {error}"));
    let selected_after = selected_pane_id(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading selected pane after focus: {error}"));
    let after_slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot after focus: {error}"));
    let target_after = pane_geometry_by_id(&after_geometry, &target_pane)
        .unwrap_or_else(|| panic!("missing target pane geometry for {target_pane}"));

    let repeat_focus = run_focus_route(&harness, &session, target_slot)
        .unwrap_or_else(|error| panic!("repeat focus invocation failed: {error}"));
    let repeat_geometry = read_pane_geometry(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading pane geometry after repeat focus: {error}"));
    let repeat_target = pane_geometry_by_id(&repeat_geometry, &target_pane)
        .unwrap_or_else(|| panic!("missing repeated target pane geometry for {target_pane}"));
    let selected_after_repeat = selected_pane_id(&harness, &session).unwrap_or_else(|error| {
        panic!("failed reading selected pane after repeated focus: {error}")
    });

    assert_eq!(
        launch.exit_code,
        0,
        "launch stderr: {}",
        launch.stderr.trim()
    );
    assert_eq!(action, "create");
    assert!(
        !session.is_empty(),
        "missing session in stdout: {}",
        launch.stdout
    );
    assert_eq!(focus.exit_code, 0, "focus stderr: {}", focus.stderr.trim());
    assert_eq!(
        repeat_focus.exit_code,
        0,
        "repeat focus stderr: {}",
        repeat_focus.stderr.trim()
    );
    assert_eq!(selected_after, target_pane);
    assert_eq!(selected_after_repeat, target_pane);
    assert_eq!(target_after.left, startup_main_geometry.left);
    assert_eq!(target_after.top, startup_main_geometry.top);
    assert_eq!(repeat_target.left, startup_main_geometry.left);
    assert_eq!(repeat_target.top, startup_main_geometry.top);
    assert!(slot_snapshots_match(&before_slots, &after_slots));
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

fn read_pane_geometry(
    harness: &FoundationHarness,
    session: &str,
) -> Result<Vec<PaneGeometry>, String> {
    let raw = harness.tmux_capture(&[
        "list-panes",
        "-t",
        &format!("{session}:0"),
        "-F",
        "#{pane_id}|#{pane_left}|#{pane_top}|#{pane_width}|#{pane_height}",
    ])?;

    let mut panes = Vec::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut parts = line.split('|');
        let pane_id = parts.next().unwrap_or_default().to_owned();
        let left = parts
            .next()
            .ok_or_else(|| format!("missing pane_left in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_left in `{line}`: {error}"))?;
        let top = parts
            .next()
            .ok_or_else(|| format!("missing pane_top in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_top in `{line}`: {error}"))?;
        let width = parts
            .next()
            .ok_or_else(|| format!("missing pane_width in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_width in `{line}`: {error}"))?;
        let height = parts
            .next()
            .ok_or_else(|| format!("missing pane_height in `{line}`"))?
            .parse::<i32>()
            .map_err(|error| format!("invalid pane_height in `{line}`: {error}"))?;
        panes.push(PaneGeometry {
            pane_id,
            left,
            top,
            width,
            height,
        });
    }

    Ok(panes)
}

fn pane_geometry_by_id<'a>(panes: &'a [PaneGeometry], pane_id: &str) -> Option<&'a PaneGeometry> {
    panes.iter().find(|pane| pane.pane_id == pane_id)
}

fn slot_pane_id(slots: &[red_support::SlotSnapshot], slot_id: u8) -> String {
    slots
        .iter()
        .find(|slot| slot.slot_id == slot_id)
        .map(|slot| slot.pane_id.clone())
        .unwrap_or_default()
}

fn slot_snapshots_match(
    left: &[red_support::SlotSnapshot],
    right: &[red_support::SlotSnapshot],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right.iter()).all(|(lhs, rhs)| {
            lhs.slot_id == rhs.slot_id && lhs.pane_id == rhs.pane_id && lhs.worktree == rhs.worktree
        })
}
