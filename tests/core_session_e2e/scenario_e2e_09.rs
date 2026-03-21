use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, RemotePathEvidence, SessionSnapshot, create_remote_remap_fixture,
    extract_stdout_field, map_settle, prepare_fresh_create_path, read_slot_snapshot, sample,
    settle_snapshot,
};

#[allow(clippy::too_many_lines)]
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
    let effective_remote_dir_prefix =
        extract_stdout_field(&launch.stdout, "remote_dir_prefix").unwrap_or_default();
    let remote_dir_prefix_source =
        extract_stdout_field(&launch.stdout, "remote_dir_prefix_source").unwrap_or_default();
    let opencode_attach_url =
        extract_stdout_field(&launch.stdout, "opencode_attach_url").unwrap_or_default();
    let opencode_server_url_source =
        extract_stdout_field(&launch.stdout, "opencode_server_url_source").unwrap_or_default();
    let opencode_server_host =
        extract_stdout_field(&launch.stdout, "opencode_server_host").unwrap_or_default();
    let opencode_server_host_source =
        extract_stdout_field(&launch.stdout, "opencode_server_host_source").unwrap_or_default();
    let opencode_server_port =
        extract_stdout_field(&launch.stdout, "opencode_server_port").unwrap_or_default();
    let opencode_server_port_source =
        extract_stdout_field(&launch.stdout, "opencode_server_port_source").unwrap_or_default();
    let opencode_server_password_set =
        extract_stdout_field(&launch.stdout, "opencode_server_password_set")
            .is_some_and(|value| value == "true");
    let opencode_server_password_source =
        extract_stdout_field(&launch.stdout, "opencode_server_password_source").unwrap_or_default();

    let expected_attach_url = String::from("http://127.0.0.1:4096");
    let remote_prefix_source_is_env = remote_dir_prefix_source == "env";
    let remote_prefix_matches = effective_remote_dir_prefix == remote_prefix;
    let attach_url_matches_default = opencode_attach_url == expected_attach_url;
    let server_url_source_is_default = opencode_server_url_source == "default";
    let server_host_matches_default = opencode_server_host == "127.0.0.1";
    let server_host_source_is_default = opencode_server_host_source == "default";
    let server_port_matches_default = opencode_server_port == "4096";
    let server_port_source_is_default = opencode_server_port_source == "default";
    let password_source_is_default = opencode_server_password_source == "default";

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
        "effective remote prefix = {effective_remote_dir_prefix} (source={remote_dir_prefix_source})"
    ));
    assertions.push(format!("effective attach url = {opencode_attach_url}"));
    assertions.push(format!(
        "effective server host = {opencode_server_host} (source={opencode_server_host_source})"
    ));
    assertions.push(format!(
        "effective server port = {opencode_server_port} (source={opencode_server_port_source})"
    ));
    assertions.push(format!(
        "effective password configured flag = {opencode_server_password_set} (source={opencode_server_password_source})"
    ));
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
        && remote_prefix_source_is_env
        && remote_prefix_matches
        && attach_url_matches_default
        && server_url_source_is_default
        && server_host_matches_default
        && server_host_source_is_default
        && server_port_matches_default
        && server_port_source_is_default
        && !opencode_server_password_set
        && password_source_is_default
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
            remote_dir_prefix_source,
            expected_mapped_path,
            effective_mapped_path,
            remap_applied,
            opencode_attach_url,
            opencode_server_url_source,
            opencode_server_host,
            opencode_server_host_source,
            opencode_server_port,
            opencode_server_port_source,
            opencode_server_password_set,
            opencode_server_password_source,
        }),
        helper_state: None,
    }
}
