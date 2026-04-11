#[path = "support/focus5_amendment_t1_1_red_support.rs"]
mod red_support;
mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use red_support::{
    center_pane_id, create_worktree_fixture, extract_stdout_field, paths_equivalent,
    read_pane_widths, read_slot_snapshot, write_cluster_evidence,
};
use support::foundation_harness::FoundationHarness;

struct AttachProbeOutcome {
    probe_exit: i32,
    observed_attached_client: bool,
    attempts: Vec<(usize, i32, bool)>,
}

#[test]
fn t1_2_startup_attach_visibility_reports_non_interactive_and_observes_interactive_attach() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-2")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let create = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup create launch failed: {error}"));
    let create_action = extract_stdout_field(&create.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&create.stdout, "session").unwrap_or_default();
    let create_attach_visibility =
        extract_stdout_field(&create.stdout, "attach_visibility").unwrap_or_default();

    let pty_probe = run_attach_probe_with_retries(&harness, &session, 5)
        .unwrap_or_else(|error| panic!("interactive attach probe failed: {error}"));

    let attach = harness
        .run_ezm(&[], &[], 0)
        .unwrap_or_else(|error| panic!("startup attach launch failed: {error}"));
    let attach_action = extract_stdout_field(&attach.stdout, "session_action").unwrap_or_default();
    let attach_session = extract_stdout_field(&attach.stdout, "session").unwrap_or_default();
    let attach_visibility =
        extract_stdout_field(&attach.stdout, "attach_visibility").unwrap_or_default();

    let mut evidence = vec![
        format!("create_exit_code={}", create.exit_code),
        format!("create_action={create_action}"),
        format!("create_session={session}"),
        format!("create_attach_visibility={create_attach_visibility}"),
        format!("pty_probe_exit={}", pty_probe.probe_exit),
        format!("pty_attach_observed={}", pty_probe.observed_attached_client),
        format!("pty_probe_attempts={}", pty_probe.attempts.len()),
        format!("attach_exit_code={}", attach.exit_code),
        format!("attach_action={attach_action}"),
        format!("attach_session={attach_session}"),
        format!("attach_visibility={attach_visibility}"),
    ];
    for (attempt, exit_code, observed) in &pty_probe.attempts {
        evidence.push(format!(
            "pty_probe_attempt_{attempt}_exit={exit_code} observed={observed}"
        ));
    }
    let pty_probe_attempted = !pty_probe.attempts.is_empty();
    evidence.push(format!("pty_probe_attempted={pty_probe_attempted}"));
    write_cluster_evidence(&harness, "t1-2-startup-attach-visibility", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.2 evidence: {error}"));

    let pass = create.exit_code == 0
        && create_action == "create"
        && !session.is_empty()
        && create_attach_visibility == "non-interactive"
        && pty_probe_attempted
        && pty_probe.observed_attached_client
        && attach.exit_code == 0
        && attach_action == "attach"
        && attach_session == session
        && attach_visibility == "non-interactive";

    assert!(
        pass,
        "T-1.2 startup attach visibility contract failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn t1_2_startup_slots_launch_shell_for_underfilled_fallback_worktree_slots() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-2")
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
    let slot1_matches_expected = paths_equivalent(
        &slot1_worktree,
        &fixture.expected_slot1_worktree.display().to_string(),
    );

    let mut per_slot = Vec::new();
    for slot_id in 1_u8..=5 {
        let slot = slots
            .iter()
            .find(|slot| slot.slot_id == slot_id)
            .unwrap_or_else(|| panic!("missing slot snapshot for slot {slot_id}"));
        let mode = read_slot_mode(&harness, &session, slot_id)
            .unwrap_or_else(|error| panic!("failed reading slot mode for slot {slot_id}: {error}"));
        let pane_start_command =
            read_pane_start_command(&harness, &slot.pane_id).unwrap_or_else(|error| {
                panic!(
                    "failed reading pane_start_command for slot {slot_id} pane {}: {error}",
                    slot.pane_id
                )
            });
        let expects_agent = slot_id <= 3;
        let mode_matches = if expects_agent {
            mode == "agent"
        } else {
            mode == "shell"
        };
        let command_matches = if expects_agent {
            pane_start_command.is_empty() || pane_start_command.contains("opencode")
        } else {
            pane_start_command.is_empty()
                || pane_start_command.contains("${SHELL:-/bin/sh}")
                || pane_start_command.contains("exec zsh")
                || pane_start_command.contains("exec bash")
                || pane_start_command.contains("exec sh")
        };

        per_slot.push((
            slot_id,
            mode,
            pane_start_command,
            mode_matches,
            command_matches,
        ));
    }

    let no_negative_worktrees = slots.iter().all(|slot| slot.worktree != "-1");
    let startup_slot_modes_correct = per_slot
        .iter()
        .all(|(_, _, _, mode_matches, _)| *mode_matches);
    let startup_commands_correct = per_slot
        .iter()
        .all(|(_, _, _, _, command_matches)| *command_matches);

    let mut evidence = vec![
        format!("exit_code={} session={session}", launch.exit_code),
        format!("center_pane={center_pane}"),
        format!("center_slot={center_slot:?}"),
        format!("slot1_worktree={slot1_worktree}"),
        format!(
            "expected_slot1_worktree={}",
            fixture.expected_slot1_worktree.display()
        ),
        format!("slot1_matches_expected={slot1_matches_expected}"),
        format!("no_negative_worktrees={no_negative_worktrees}"),
        format!("startup_slot_modes_correct={startup_slot_modes_correct}"),
        format!("startup_commands_correct={startup_commands_correct}"),
    ];

    for (slot_id, mode, command, mode_matches, command_matches) in &per_slot {
        evidence.push(format!("slot{slot_id}_mode={mode}"));
        evidence.push(format!("slot{slot_id}_pane_start_command={command}"));
        evidence.push(format!("slot{slot_id}_mode_matches={mode_matches}"));
        evidence.push(format!("slot{slot_id}_command_matches={command_matches}"));
    }
    write_cluster_evidence(&harness, "t1-2-startup-mode-launch", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.2 evidence: {error}"));

    let pass = launch.exit_code == 0
        && !session.is_empty()
        && center_slot == Some(1)
        && slot1_matches_expected
        && no_negative_worktrees
        && startup_slot_modes_correct
        && startup_commands_correct;

    assert!(
        pass,
        "T-1.2 startup mode launch contract failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn t1_2_startup_single_worktree_fallback_slots_launch_shell_mode() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-2")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let project_dir = create_single_worktree_fixture(&harness)
        .unwrap_or_else(|error| panic!("single-worktree fixture setup failed: {error}"));
    let launch = harness
        .run_ezm_in_dir(&project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("single-worktree startup launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading single-worktree slot snapshot: {error}"));

    let mut per_slot = Vec::new();
    for slot_id in 1_u8..=5 {
        let slot = slots
            .iter()
            .find(|slot| slot.slot_id == slot_id)
            .unwrap_or_else(|| panic!("missing single-worktree slot snapshot for slot {slot_id}"));
        let mode = read_slot_mode(&harness, &session, slot_id).unwrap_or_else(|error| {
            panic!("failed reading single-worktree slot mode for slot {slot_id}: {error}")
        });
        let pane_start_command = read_pane_start_command(&harness, &slot.pane_id).unwrap_or_else(|error| {
            panic!(
                "failed reading single-worktree pane_start_command for slot {slot_id} pane {}: {error}",
                slot.pane_id
            )
        });

        per_slot.push((slot_id, mode, pane_start_command));
    }

    let slot1_agent = per_slot
        .iter()
        .find(|(slot_id, _, _)| *slot_id == 1)
        .is_some_and(|(_, mode, command)| {
            mode == "agent" && (command.is_empty() || command.contains("opencode"))
        });
    let fallback_slots_shell =
        per_slot
            .iter()
            .filter(|(slot_id, _, _)| *slot_id != 1)
            .all(|(_, mode, command)| {
                mode == "shell"
                    && (command.is_empty()
                        || command.contains("${SHELL:-/bin/sh}")
                        || command.contains("exec zsh")
                        || command.contains("exec bash")
                        || command.contains("exec sh"))
            });

    let mut evidence = vec![
        format!("exit_code={} session={session}", launch.exit_code),
        format!("slot1_agent={slot1_agent}"),
        format!("fallback_slots_shell={fallback_slots_shell}"),
    ];
    for (slot_id, mode, command) in &per_slot {
        evidence.push(format!("slot{slot_id}_mode={mode}"));
        evidence.push(format!("slot{slot_id}_pane_start_command={command}"));
    }
    write_cluster_evidence(
        &harness,
        "t1-2-single-worktree-startup-mode-launch",
        &evidence,
    )
    .unwrap_or_else(|error| panic!("failed writing single-worktree T-1.2 evidence: {error}"));

    assert!(
        launch.exit_code == 0 && !session.is_empty() && slot1_agent && fallback_slots_shell,
        "T-1.2 single-worktree startup mode launch contract failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn t1_2_startup_worktree_assignment_stays_deterministic_across_restart_without_negative_slots() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-2")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let fixture = create_worktree_fixture(&harness)
        .unwrap_or_else(|error| panic!("fixture setup failed: {error}"));

    let first = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("first launch failed: {error}"));
    let second = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("second launch failed: {error}"));

    let first_action = extract_stdout_field(&first.stdout, "session_action").unwrap_or_default();
    let second_action = extract_stdout_field(&second.stdout, "session_action").unwrap_or_default();
    let first_session = extract_stdout_field(&first.stdout, "session").unwrap_or_default();
    let second_session = extract_stdout_field(&second.stdout, "session").unwrap_or_default();

    let first_slots = read_slot_snapshot(&harness, &first_session)
        .unwrap_or_else(|error| panic!("failed reading first slot snapshot: {error}"));
    let second_slots = read_slot_snapshot(&harness, &second_session)
        .unwrap_or_else(|error| panic!("failed reading second slot snapshot: {error}"));

    let no_negative_worktrees = first_slots
        .iter()
        .chain(second_slots.iter())
        .all(|slot| slot.worktree != "-1");
    let worktree_mapping_stable = slot_worktree_mapping_stable(&first_slots, &second_slots);

    let evidence = vec![
        format!("first_exit_code={}", first.exit_code),
        format!("second_exit_code={}", second.exit_code),
        format!("first_action={first_action}"),
        format!("second_action={second_action}"),
        format!("first_session={first_session}"),
        format!("second_session={second_session}"),
        format!("worktree_mapping_stable={worktree_mapping_stable}"),
        format!("no_negative_worktrees={no_negative_worktrees}"),
    ];
    write_cluster_evidence(&harness, "t1-2-worktree-determinism", &evidence)
        .unwrap_or_else(|error| panic!("failed writing T-1.2 evidence: {error}"));

    let pass = first.exit_code == 0
        && second.exit_code == 0
        && first_action == "create"
        && second_action == "attach"
        && !first_session.is_empty()
        && first_session == second_session
        && worktree_mapping_stable
        && no_negative_worktrees;

    assert!(
        pass,
        "T-1.2 startup worktree determinism contract failed:\n{}",
        evidence.join("\n")
    );
}

#[test]
fn t1_2_startup_mode_launch_uses_slot_specific_assigned_worktree_cwd_for_five_worktrees() {
    let harness = FoundationHarness::new_for_suite("focus5-amendment-t1-2")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let fixture = create_five_worktree_numbered_fixture(&harness)
        .unwrap_or_else(|error| panic!("five-worktree fixture setup failed: {error}"));
    let launch = harness
        .run_ezm_in_dir(&fixture.project_dir, &[], &[], 0)
        .unwrap_or_else(|error| panic!("five-worktree startup launch failed: {error}"));
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();

    let slots = read_slot_snapshot(&harness, &session)
        .unwrap_or_else(|error| panic!("failed reading slot snapshot: {error}"));

    let mut evidence = vec![
        format!("exit_code={} session={session}", launch.exit_code),
        format!(
            "expected_slot_worktrees={:?}",
            fixture.expected_slot_worktrees
        ),
    ];

    let mut mapping_matches = true;
    let mut session_cwd_matches = true;
    let mut pane_cwd_matches = true;
    for slot_id in 1_u8..=5 {
        let index = usize::from(slot_id - 1);
        let expected_worktree = fixture.expected_slot_worktrees[index].display().to_string();
        let slot = slots
            .iter()
            .find(|slot| slot.slot_id == slot_id)
            .unwrap_or_else(|| panic!("missing slot snapshot for slot {slot_id}"));
        let session_cwd = read_slot_cwd(&harness, &session, slot_id)
            .unwrap_or_else(|error| panic!("failed reading slot cwd for slot {slot_id}: {error}"));
        let pane_cwd = read_pane_slot_cwd(&harness, &slot.pane_id).unwrap_or_else(|error| {
            panic!(
                "failed reading pane slot cwd for slot {slot_id} pane {}: {error}",
                slot.pane_id
            )
        });

        let slot_mapping_match = paths_equivalent(&slot.worktree, &expected_worktree);
        let slot_session_cwd_match = paths_equivalent(&session_cwd, &expected_worktree);
        let slot_pane_cwd_match = paths_equivalent(&pane_cwd, &expected_worktree);

        mapping_matches &= slot_mapping_match;
        session_cwd_matches &= slot_session_cwd_match;
        pane_cwd_matches &= slot_pane_cwd_match;

        evidence.push(format!(
            "slot{slot_id}_expected_worktree={expected_worktree}"
        ));
        evidence.push(format!("slot{slot_id}_worktree={}", slot.worktree));
        evidence.push(format!("slot{slot_id}_session_cwd={session_cwd}"));
        evidence.push(format!("slot{slot_id}_pane_cwd={pane_cwd}"));
        evidence.push(format!("slot{slot_id}_mapping_match={slot_mapping_match}"));
        evidence.push(format!(
            "slot{slot_id}_session_cwd_match={slot_session_cwd_match}"
        ));
        evidence.push(format!(
            "slot{slot_id}_pane_cwd_match={slot_pane_cwd_match}"
        ));
    }

    evidence.push(format!("mapping_matches={mapping_matches}"));
    evidence.push(format!("session_cwd_matches={session_cwd_matches}"));
    evidence.push(format!("pane_cwd_matches={pane_cwd_matches}"));

    write_cluster_evidence(
        &harness,
        "t1-2-startup-five-worktree-assigned-cwd",
        &evidence,
    )
    .unwrap_or_else(|error| panic!("failed writing five-worktree T-1.2 evidence: {error}"));

    assert!(
        launch.exit_code == 0
            && !session.is_empty()
            && mapping_matches
            && session_cwd_matches
            && pane_cwd_matches,
        "T-1.2 startup assigned cwd for five-worktree launch failed:\n{}",
        evidence.join("\n")
    );
}

fn read_slot_mode(
    harness: &FoundationHarness,
    session: &str,
    slot_id: u8,
) -> Result<String, String> {
    harness
        .tmux_capture(&[
            "show-options",
            "-v",
            "-t",
            session,
            &format!("@ezm_slot_{slot_id}_mode"),
        ])
        .map(|value| value.trim().to_owned())
}

fn read_pane_start_command(harness: &FoundationHarness, pane_id: &str) -> Result<String, String> {
    harness
        .tmux_capture(&[
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{pane_start_command}",
        ])
        .map(|value| value.trim().to_owned())
}

fn read_slot_cwd(
    harness: &FoundationHarness,
    session: &str,
    slot_id: u8,
) -> Result<String, String> {
    harness
        .tmux_capture(&[
            "show-options",
            "-v",
            "-t",
            session,
            &format!("@ezm_slot_{slot_id}_cwd"),
        ])
        .map(|value| value.trim().to_owned())
}

fn read_pane_slot_cwd(harness: &FoundationHarness, pane_id: &str) -> Result<String, String> {
    harness
        .tmux_capture(&["show-options", "-pv", "-t", pane_id, "@ezm_slot_cwd"])
        .map(|value| value.trim().to_owned())
}

fn slot_worktree_mapping_stable(
    left: &[red_support::SlotSnapshot],
    right: &[red_support::SlotSnapshot],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right.iter()).all(|(lhs, rhs)| {
            lhs.slot_id == rhs.slot_id && paths_equivalent(&lhs.worktree, &rhs.worktree)
        })
}

fn run_attach_probe_with_retries(
    harness: &FoundationHarness,
    session: &str,
    max_attempts: usize,
) -> Result<AttachProbeOutcome, String> {
    let attempts = max_attempts.max(1);
    let mut history = Vec::with_capacity(attempts);
    let mut final_exit_code = -1;
    let mut observed_attached_client = false;

    for attempt in 1..=attempts {
        let probe = harness
            .run_ezm_with_pty_attach_probe(harness.project_root(), &[], &[], 0, session)
            .map_err(|error| {
                format!(
                    "attempt {attempt}/{attempts} failed while probing interactive attach: {error}"
                )
            })?;

        final_exit_code = probe.exit_code;
        observed_attached_client = probe.observed_attached_client;
        history.push((attempt, probe.exit_code, probe.observed_attached_client));

        if observed_attached_client {
            break;
        }
    }

    Ok(AttachProbeOutcome {
        probe_exit: final_exit_code,
        observed_attached_client,
        attempts: history,
    })
}

struct FiveWorktreeFixture {
    project_dir: PathBuf,
    expected_slot_worktrees: [PathBuf; 5],
}

fn create_five_worktree_numbered_fixture(
    harness: &FoundationHarness,
) -> Result<FiveWorktreeFixture, String> {
    let fixture_root = harness.work_dir().join("t12-five-worktree-numbered");
    let wt_1 = fixture_root.join("ez-mux-1");
    let wt_2 = fixture_root.join("ez-mux-2");
    let wt_3 = fixture_root.join("ez-mux-3");
    let wt_4 = fixture_root.join("ez-mux-4");
    let wt_5 = fixture_root.join("ez-mux-5");

    if fixture_root.exists() {
        fs::remove_dir_all(&fixture_root).map_err(|error| {
            format!(
                "failed resetting five-worktree fixture root {}: {error}",
                fixture_root.display()
            )
        })?;
    }

    fs::create_dir_all(&wt_1)
        .map_err(|error| format!("failed creating five-worktree fixture project: {error}"))?;

    run_git(&wt_1, &["init", "--initial-branch", "main"])?;
    run_git(&wt_1, &["config", "user.email", "e2e@example.invalid"])?;
    run_git(&wt_1, &["config", "user.name", "E2E Harness"])?;
    fs::write(wt_1.join("README.md"), "# five worktree fixture\n")
        .map_err(|error| format!("failed writing five-worktree fixture README: {error}"))?;
    run_git(&wt_1, &["add", "README.md"])?;
    run_git(&wt_1, &["commit", "-m", "fixture init"])?;

    let wt_2_arg = wt_2.display().to_string();
    let wt_3_arg = wt_3.display().to_string();
    let wt_4_arg = wt_4.display().to_string();
    let wt_5_arg = wt_5.display().to_string();

    run_git(
        &wt_1,
        &["worktree", "add", "--detach", wt_2_arg.as_str(), "HEAD"],
    )?;
    run_git(
        &wt_1,
        &["worktree", "add", "--detach", wt_3_arg.as_str(), "HEAD"],
    )?;
    run_git(
        &wt_1,
        &["worktree", "add", "--detach", wt_4_arg.as_str(), "HEAD"],
    )?;
    run_git(
        &wt_1,
        &["worktree", "add", "--detach", wt_5_arg.as_str(), "HEAD"],
    )?;

    Ok(FiveWorktreeFixture {
        project_dir: wt_1.clone(),
        expected_slot_worktrees: [wt_1, wt_2, wt_3, wt_4, wt_5],
    })
}

fn create_single_worktree_fixture(harness: &FoundationHarness) -> Result<PathBuf, String> {
    let fixture_root = harness.work_dir().join("t12-single-worktree");
    let project_dir = fixture_root.join("project");

    if fixture_root.exists() {
        fs::remove_dir_all(&fixture_root).map_err(|error| {
            format!(
                "failed resetting single-worktree fixture root {}: {error}",
                fixture_root.display()
            )
        })?;
    }

    fs::create_dir_all(&project_dir)
        .map_err(|error| format!("failed creating single-worktree fixture project: {error}"))?;

    run_git(&project_dir, &["init", "--initial-branch", "main"])?;
    run_git(
        &project_dir,
        &["config", "user.email", "e2e@example.invalid"],
    )?;
    run_git(&project_dir, &["config", "user.name", "E2E Harness"])?;
    fs::write(project_dir.join("README.md"), "# single worktree fixture\n")
        .map_err(|error| format!("failed writing single-worktree fixture README: {error}"))?;
    run_git(&project_dir, &["add", "README.md"])?;
    run_git(&project_dir, &["commit", "-m", "fixture init"])?;

    Ok(project_dir)
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
