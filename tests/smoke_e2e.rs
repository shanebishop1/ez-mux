mod support;

#[path = "core_session_e2e/core_support.rs"]
#[allow(dead_code)]
mod core_support;
#[path = "core_session_e2e/scenario_e2e_01.rs"]
mod scenario_e2e_01;
#[path = "core_session_e2e/scenario_e2e_04.rs"]
mod scenario_e2e_04;
#[path = "core_session_e2e/scenario_e2e_06.rs"]
mod scenario_e2e_06;
#[path = "core_session_e2e/scenario_e2e_11.rs"]
mod scenario_e2e_11;

use core_support::{
    CaseEvidence, RunMetadata, SuiteEvidence, read_commit_sha, write_case_artifacts, write_json,
};
use serde::Serialize;
use support::foundation_harness::FoundationHarness;

#[derive(Serialize)]
struct SmokeOutputEnvelope {
    run_id: String,
    commit_sha: String,
    os: String,
    tmux_version: String,
    test_ids: Vec<String>,
    pass_total: usize,
    fail_total: usize,
}

#[derive(Serialize)]
struct SmokeTopologyCaseEvidence {
    id: String,
    attempts: u32,
    poll_interval_ms: u64,
    timeout_ms: u64,
    stable: bool,
    sessions: Vec<String>,
    windows: Vec<String>,
    panes: Vec<String>,
}

#[derive(Serialize)]
struct SmokeTopologyRunEvidence {
    run_id: String,
    cases: Vec<SmokeTopologyCaseEvidence>,
}

const SMOKE_IDS: [&str; 4] = ["E2E-01", "E2E-04", "E2E-06", "E2E-11"];
const SMOKE_PROFILE: &str = "core-flows-v1";

#[derive(Clone, Copy)]
enum SmokePlatform {
    Linux,
    Macos,
}

#[derive(Serialize)]
struct MatrixEvidence {
    profile: String,
    platform: String,
    test_ids: Vec<String>,
}

#[test]
fn cross_platform_smoke_suite() {
    let platform = selected_platform();
    let harness = FoundationHarness::new_for_suite("cross-platform-smoke")
        .unwrap_or_else(|error| panic!("harness setup failed: {error}"));

    let cases = run_matrix(&harness, platform);
    write_case_artifacts(&harness.artifact_dir.join("cases"), &cases)
        .unwrap_or_else(|error| panic!("failed writing case evidence artifacts: {error}"));

    let matrix = MatrixEvidence {
        profile: SMOKE_PROFILE.to_string(),
        platform: platform.label().to_string(),
        test_ids: SMOKE_IDS.iter().map(|id| (*id).to_string()).collect(),
    };
    write_json(&harness.artifact_dir.join("matrix.json"), &matrix)
        .unwrap_or_else(|error| panic!("failed writing matrix evidence: {error}"));

    let pass_total = cases.iter().filter(|case| case.pass).count();
    let fail_total = cases.len() - pass_total;
    let commit_sha = read_commit_sha(harness.project_root());
    let os = std::env::consts::OS.to_owned();
    let tmux_version = harness
        .tmux_version()
        .unwrap_or_else(|error| format!("unknown ({error})"));
    let test_ids = SMOKE_IDS
        .iter()
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();

    let summary = SuiteEvidence {
        metadata: RunMetadata {
            run_id: harness.run_id.clone(),
            commit_sha: commit_sha.clone(),
            os: os.clone(),
            shell: harness.shell.clone(),
            tmux_version: tmux_version.clone(),
            artifact_dir: harness.artifact_dir.display().to_string(),
            test_ids: test_ids.clone(),
            pass_total,
            fail_total,
        },
        cases,
    };

    let envelope = SmokeOutputEnvelope {
        run_id: harness.run_id.clone(),
        commit_sha,
        os,
        tmux_version,
        test_ids,
        pass_total,
        fail_total,
    };
    write_json(&harness.artifact_dir.join("envelope.json"), &envelope)
        .unwrap_or_else(|error| panic!("failed writing smoke envelope: {error}"));

    let topology = SmokeTopologyRunEvidence {
        run_id: harness.run_id.clone(),
        cases: summary
            .cases
            .iter()
            .map(|case| SmokeTopologyCaseEvidence {
                id: case.id.clone(),
                attempts: case.settle.attempts,
                poll_interval_ms: case.settle.poll_interval_ms,
                timeout_ms: case.settle.timeout_ms,
                stable: case.settle.stable,
                sessions: normalize_snapshot_lines(&case.settle.sessions),
                windows: normalize_snapshot_lines(&case.settle.windows),
                panes: normalize_snapshot_lines(&case.settle.panes),
            })
            .collect(),
    };
    write_json(&harness.artifact_dir.join("topology.json"), &topology)
        .unwrap_or_else(|error| panic!("failed writing smoke topology evidence: {error}"));

    write_json(&harness.artifact_dir.join("summary.json"), &summary)
        .unwrap_or_else(|error| panic!("failed writing summary evidence: {error}"));

    assert_eq!(
        summary.metadata.fail_total, 0,
        "cross-platform smoke suite contains failures; inspect summary artifact"
    );
}

fn normalize_snapshot_lines(snapshot: &str) -> Vec<String> {
    let mut lines = snapshot
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines.sort();
    lines
}

fn run_matrix(harness: &FoundationHarness, platform: SmokePlatform) -> Vec<CaseEvidence> {
    let run_order = matrix_for_platform(platform);
    run_order.into_iter().map(|run| run(harness)).collect()
}

fn matrix_for_platform(platform: SmokePlatform) -> Vec<fn(&FoundationHarness) -> CaseEvidence> {
    match platform {
        SmokePlatform::Linux | SmokePlatform::Macos => vec![
            scenario_e2e_01::run,
            scenario_e2e_04::run,
            scenario_e2e_06::run,
            scenario_e2e_11::run,
        ],
    }
}

fn selected_platform() -> SmokePlatform {
    match std::env::var("EZM_SMOKE_PLATFORM") {
        Ok(value) => SmokePlatform::from_label(value.as_str())
            .unwrap_or_else(|| panic!("unsupported EZM_SMOKE_PLATFORM value: {value}")),
        Err(_) => SmokePlatform::from_host_os(),
    }
}

impl SmokePlatform {
    fn from_label(value: &str) -> Option<Self> {
        match value {
            "linux" => Some(Self::Linux),
            "macos" => Some(Self::Macos),
            _ => None,
        }
    }

    fn from_host_os() -> Self {
        match std::env::consts::OS {
            "linux" => Self::Linux,
            "macos" => Self::Macos,
            os => panic!("unsupported host OS for smoke suite: {os}"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Macos => "macos",
        }
    }
}
