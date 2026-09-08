use crate::support::foundation_harness::FoundationHarness;

use super::core_support::{
    CaseEvidence, DEFAULT_POLL_INTERVAL, HelperLifecycleEvidence, SessionSnapshot, map_settle,
    poll_until, popup_helper_session_name, prepare_fresh_create_path, read_helper_state_snapshot,
    sample, settle_snapshot, wait_for_helper_pids_to_exit,
};

const INTERRUPT_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

#[allow(clippy::too_many_lines)]
pub(super) fn run(harness: &FoundationHarness) -> CaseEvidence {
    let mut assertions = Vec::new();
    let mut samples = Vec::new();

    let expected_session = prepare_fresh_create_path(harness, harness.project_root())
        .unwrap_or_else(|error| panic!("E2E-19 setup failed: {error}"));
    let path = harness.path_with_fake_perles();
    let perles_env = [("PATH", path.as_str())];

    let session = expected_session.clone();
    let popup_session = popup_helper_session_name(&session, 4);
    let auxiliary_open_args = vec![
        "__internal",
        "auxiliary",
        "--session",
        &session,
        "--action",
        "open",
    ];
    let mut auxiliary_open = None;
    let mut before_state = None;

    let interrupt_probe = harness
        .run_ezm_with_pty_interrupt(
            harness.project_root(),
            &[],
            &perles_env,
            0,
            &session,
            || {
                harness.tmux_capture(&[
                    "new-session",
                    "-d",
                    "-s",
                    &popup_session,
                    "sleep",
                    "3600",
                ])?;

                let auxiliary = harness.run_ezm(&auxiliary_open_args, &perles_env, 0)?;
                if auxiliary.exit_code != 0 {
                    return Err(format!(
                        "auxiliary open failed with exit code {}: {}",
                        auxiliary.exit_code, auxiliary.stderr
                    ));
                }
                auxiliary_open = Some(auxiliary);
                before_state = Some(read_helper_state_snapshot(harness, &session));
                Ok(())
            },
        )
        .unwrap_or_else(|error| panic!("E2E-19 interrupt probe failed: {error}"));

    let auxiliary_open =
        auxiliary_open.expect("E2E-19 attached callback did not run auxiliary open");
    let before_state = before_state.expect("E2E-19 attached callback did not capture helper state");
    samples.push(sample(&auxiliary_open_args, &auxiliary_open));

    let popup_present_before_interrupt = before_state
        .helper_sessions
        .iter()
        .any(|helper| helper == &popup_session);

    let interrupt_cleanup_path = interrupt_probe.observed_attached_client
        && interrupt_probe.signal_sent
        && interrupt_probe.exit_code == 130;

    let cleanup_observed = poll_until(INTERRUPT_CLEANUP_TIMEOUT, DEFAULT_POLL_INTERVAL, || {
        let project_gone = harness
            .tmux_capture(&["has-session", "-t", &session])
            .is_err();
        let helpers_gone = read_helper_state_snapshot(harness, &session)
            .helper_sessions
            .is_empty();
        Ok(project_gone && helpers_gone)
    })
    .unwrap_or_else(|error| panic!("E2E-19 failed polling interrupt cleanup: {error}"));

    let project_session_present_after_interrupt = harness
        .tmux_capture(&["has-session", "-t", &session])
        .is_ok();
    let after_state = read_helper_state_snapshot(harness, &session);
    let leaked_helper_pids = wait_for_helper_pids_to_exit(
        &before_state.helper_pane_pids,
        INTERRUPT_CLEANUP_TIMEOUT,
        DEFAULT_POLL_INTERVAL,
    )
    .unwrap_or_else(|error| panic!("E2E-19 failed polling helper pid shutdown: {error}"));

    assertions.push(String::from(
        "interrupt launch started from absent session = true",
    ));
    assertions.push(format!("session = {session}"));
    assertions.push(String::from(
        "owned popup helper fixture created after attach = true",
    ));
    assertions.push(format!(
        "auxiliary open exit_code = {}",
        auxiliary_open.exit_code
    ));
    assertions.push(format!(
        "signal event sent while attach active = {}",
        interrupt_probe.signal_sent && interrupt_probe.observed_attached_client
    ));
    assertions.push(format!(
        "interrupt exit_code = {}",
        interrupt_probe.exit_code
    ));
    assertions.push(format!(
        "interrupt path met attach+signal+130 criteria = {interrupt_cleanup_path}"
    ));
    assertions.push(format!(
        "interrupt probe diagnostics = {}",
        interrupt_probe.diagnostics
    ));
    assertions.push(format!(
        "popup helper session exists before interrupt = {popup_present_before_interrupt}"
    ));
    assertions.push(format!(
        "helper sessions present before interrupt = {}",
        !before_state.helper_sessions.is_empty()
    ));
    assertions.push(format!(
        "helper pane pids present before interrupt = {}",
        !before_state.helper_pane_pids.is_empty()
    ));
    assertions.push(format!(
        "bounded interrupt cleanup observed = {cleanup_observed}"
    ));
    assertions.push(format!(
        "project session removed after interrupt cleanup = {}",
        !project_session_present_after_interrupt
    ));
    assertions.push(format!(
        "helper sessions removed after interrupt cleanup = {}",
        after_state.helper_sessions.is_empty()
    ));
    assertions.push(format!(
        "helper pane pids removed after interrupt cleanup = {}",
        after_state.helper_pane_pids.is_empty()
    ));
    assertions.push(format!(
        "tracked pre-interrupt helper pids still alive after cleanup = {}",
        leaked_helper_pids.len()
    ));

    let settle = settle_snapshot(harness, "E2E-19");
    let session_exists = project_session_present_after_interrupt;
    let session_count = usize::from(session_exists);
    let pass = session == expected_session
        && auxiliary_open.exit_code == 0
        && interrupt_cleanup_path
        && popup_present_before_interrupt
        && !before_state.helper_sessions.is_empty()
        && !before_state.helper_pane_pids.is_empty()
        && cleanup_observed
        && !project_session_present_after_interrupt
        && after_state.helper_sessions.is_empty()
        && after_state.helper_pane_pids.is_empty()
        && leaked_helper_pids.is_empty()
        && settle.stable;

    CaseEvidence {
        id: String::from("E2E-19"),
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
        slots: None,
        remote_path: None,
        helper_state: Some(HelperLifecycleEvidence {
            before: before_state,
            after: after_state,
            pre_helper_pids_alive_after_teardown: leaked_helper_pids,
        }),
    }
}
