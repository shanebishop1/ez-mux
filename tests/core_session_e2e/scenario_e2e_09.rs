use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, RemotePathEvidence, SessionSnapshot, create_remote_remap_fixture,
    extract_stdout_field, map_settle, prepare_fresh_create_path, read_slot_snapshot, sample,
    settle_snapshot,
};

pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let fixture = create_remote_remap_fixture(harness)
        .unwrap_or_else(|error| panic!("E2E-09 remote fixture setup failed: {error}"));
    let remote_prefix = fixture.remote_prefix.display().to_string();
    let expected_mapped_path = fixture.expected_mapped_path.display().to_string();

    let expected_session = prepare_fresh_create_path(harness, &fixture.project_dir)
        .unwrap_or_else(|error| panic!("E2E-09 setup failed: {error}"));

    let launch = harness
        .run_ezm_in_dir(
            &fixture.project_dir,
            &[],
            &[("OPENCODE_REMOTE_DIR_PREFIX", &remote_prefix)],
            0,
        )
        .unwrap_or_else(|error| panic!("E2E-09 launch failed: {error}"));
    samples.push(sample(&[], &launch));

    let launch_action = extract_stdout_field(&launch.stdout, "session_action").unwrap_or_default();
    let session = extract_stdout_field(&launch.stdout, "session").unwrap_or_default();
    let effective_mapped_path =
        extract_stdout_field(&launch.stdout, "remote_project_dir").unwrap_or_default();

    let slot_snapshot = read_slot_snapshot(harness, &session)
        .unwrap_or_else(|error| panic!("E2E-09 failed reading slot snapshot: {error}"));
    let slot_paths_match = slot_snapshot
        .iter()
        .all(|slot| slot.worktree == expected_mapped_path);

    assertions.push(format!("launch action = {launch_action}"));
    assertions.push(format!("session = {session}"));
    assertions.push(format!("effective mapped path = {effective_mapped_path}"));
    assertions.push(format!("expected mapped path = {expected_mapped_path}"));
    assertions.push(format!(
        "slot registry mapped path match = {slot_paths_match}"
    ));

    let settle = settle_snapshot(harness, "E2E-09");
    let session_exists = !session.is_empty();
    let session_count = usize::from(session_exists);
    let remap_applied = effective_mapped_path == expected_mapped_path;
    let pass = launch.exit_code == 0
        && launch_action == "create"
        && session == expected_session
        && remap_applied
        && slot_paths_match
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-09"),
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
        slots: Some(slot_snapshot),
        remote_path: Some(RemotePathEvidence {
            local_project_dir: fixture.project_dir.display().to_string(),
            remote_prefix,
            expected_mapped_path,
            effective_mapped_path,
            remap_applied,
        }),
    }
}
