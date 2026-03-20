#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import tempfile
import time
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "ezm-release-evidence-manifest/v1"
GATE_SCHEMA_VERSION = "ezm-release-gate-decision/v1"
DEFAULT_SUITE_ROOT = Path("target") / "e2e-evidence"
DEFAULT_OUTPUT_ROOT = Path("target") / "release-evidence"

SUITE_ORDER = ("foundation", "core-session-orchestration", "cross-platform-smoke", "install-validation")
REQUIRED_RELEASE_SUITES = ("foundation", "core-session-orchestration", "cross-platform-smoke", "install-validation")
REQUIRED_OS = ("linux", "macos")
FULL_REGRESSION_IDS = tuple(f"E2E-{index:02d}" for index in range(20))
REQUIRED_ARTIFACT_PATHS = (
    "artifacts/foundation/summary.json",
    "artifacts/core-session-orchestration/summary.json",
    "artifacts/core-session-orchestration/cases/E2E-12.json",
    "artifacts/core-session-orchestration/cases/E2E-13.json",
    "artifacts/cross-platform-smoke/summary.json",
    "artifacts/cross-platform-smoke/envelope.json",
    "artifacts/cross-platform-smoke/matrix.json",
    "artifacts/cross-platform-smoke/topology.json",
    "artifacts/install-validation/summary.json",
    "artifacts/install-validation/envelope.json",
    "artifacts/install-validation/contract-smoke/help.txt",
    "artifacts/install-validation/contract-smoke/version.txt",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Assemble and verify release evidence bundles.",
    )
    parser.add_argument("--output-root", default=str(DEFAULT_OUTPUT_ROOT), help="Bundle output root directory")
    parser.add_argument("--bundle-id", help="Explicit bundle id (default: generated)")
    parser.add_argument(
        "--foundation-run",
        help="Run id/path (or comma-separated list) for foundation evidence",
    )
    parser.add_argument(
        "--core-run",
        help="Run id/path (or comma-separated list) for core-session-orchestration evidence",
    )
    parser.add_argument(
        "--smoke-run",
        help="Run id/path (or comma-separated list) for cross-platform-smoke evidence",
    )
    parser.add_argument(
        "--install-run",
        help="Run id/path (or comma-separated list) for install-validation evidence",
    )
    parser.add_argument("--dry-run", action="store_true", help="Print planned actions without writing bundle")
    parser.add_argument(
        "--check-reproducible",
        action="store_true",
        help="Rebuild from recorded inputs and verify manifest/artifact reproducibility",
    )
    parser.add_argument(
        "--evaluate-gate",
        action="store_true",
        help="Evaluate release gate rules from an existing manifest",
    )
    parser.add_argument("--manifest", help="Path to an existing release evidence manifest for reproducibility checks")
    parser.add_argument(
        "--decision-output",
        help="Optional output path for gate decision JSON (default: <manifest_dir>/gate-decision.json)",
    )
    return parser.parse_args()


def generated_bundle_id() -> str:
    return f"bundle-{time.time_ns():x}-{os.getpid():x}"


def resolve_single_run_dir(base_dir: Path, supplied: str) -> Path:
    candidate = Path(supplied)
    if candidate.is_dir():
        return candidate.resolve()
    by_id = base_dir / supplied
    if by_id.is_dir():
        return by_id.resolve()
    prefixed = base_dir / f"run-{supplied}"
    if prefixed.is_dir():
        return prefixed.resolve()
    raise SystemExit(f"error: could not resolve run `{supplied}` under {base_dir}")


def parse_supplied_runs(supplied: str) -> list[str]:
    return [token.strip() for token in supplied.split(",") if token.strip()]


def choose_latest_runs_by_platform(base_dir: Path) -> list[Path]:
    runs = [entry.resolve() for entry in base_dir.glob("run-*") if entry.is_dir()]
    if not runs:
        raise SystemExit(f"error: no run directories found under {base_dir}")

    latest_by_platform: dict[str, Path] = {}
    latest_known: Path | None = None
    latest_overall = max(runs, key=lambda path: path.stat().st_mtime)

    for run_dir in runs:
        metadata = parse_run_metadata(run_dir / "summary.json")
        os_raw = metadata.get("os") if isinstance(metadata, dict) else None
        if isinstance(os_raw, str):
            platform = normalize_os_label(os_raw)
            if platform in REQUIRED_OS:
                current = latest_by_platform.get(platform)
                if current is None or run_dir.stat().st_mtime > current.stat().st_mtime:
                    latest_by_platform[platform] = run_dir
                if latest_known is None or run_dir.stat().st_mtime > latest_known.stat().st_mtime:
                    latest_known = run_dir

    selected: list[Path] = []
    for platform in REQUIRED_OS:
        run_dir = latest_by_platform.get(platform)
        if run_dir:
            selected.append(run_dir)

    if selected:
        return selected
    if latest_known:
        return [latest_known]
    return [latest_overall]


def resolve_run_dirs(base_dir: Path, supplied: str | None) -> list[Path]:
    if supplied:
        tokens = parse_supplied_runs(supplied)
        if not tokens:
            raise SystemExit("error: supplied run selection is empty")
        resolved: list[Path] = []
        seen: set[Path] = set()
        for token in tokens:
            run_dir = resolve_single_run_dir(base_dir, token)
            if run_dir not in seen:
                seen.add(run_dir)
                resolved.append(run_dir)
        return resolved

    return choose_latest_runs_by_platform(base_dir)


def discover_suite_inputs(repo_root: Path, args: argparse.Namespace) -> dict[str, list[Path]]:
    suite_root = (repo_root / DEFAULT_SUITE_ROOT).resolve()
    return {
        "foundation": resolve_run_dirs(suite_root / "foundation", args.foundation_run),
        "core-session-orchestration": resolve_run_dirs(
            suite_root / "core-session-orchestration", args.core_run
        ),
        "cross-platform-smoke": resolve_run_dirs(suite_root / "cross-platform-smoke", args.smoke_run),
        "install-validation": resolve_run_dirs(suite_root / "install-validation", args.install_run),
    }


def selected_relative_files(run_dir: Path) -> list[Path]:
    selected: list[Path] = []

    for root_file in ("summary.json", "envelope.json", "topology.json", "matrix.json"):
        candidate = run_dir / root_file
        if candidate.is_file():
            selected.append(Path(root_file))

    for subdir in ("cases", "contract-smoke"):
        root = run_dir / subdir
        if root.is_dir():
            for item in sorted(root.rglob("*")):
                if item.is_file():
                    selected.append(item.relative_to(run_dir))

    if not selected:
        raise SystemExit(f"error: no releasable artifacts detected under {run_dir}")

    return selected


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(64 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()


def rel_to_repo(repo_root: Path, path: Path) -> str:
    try:
        return path.resolve().relative_to(repo_root.resolve()).as_posix()
    except ValueError:
        return path.resolve().as_posix()


def parse_run_metadata(summary_path: Path) -> dict[str, Any]:
    if not summary_path.is_file():
        return {}
    with summary_path.open("r", encoding="utf-8") as handle:
        decoded = json.load(handle)

    metadata = decoded.get("metadata")
    if isinstance(metadata, dict):
        return metadata

    return {}


def run_descriptor(repo_root: Path, run_dir: Path) -> dict[str, Any]:
    summary_metadata = parse_run_metadata(run_dir / "summary.json")
    os_raw = summary_metadata.get("os") if isinstance(summary_metadata, dict) else None
    platform = normalize_os_label(os_raw) if isinstance(os_raw, str) else "unknown"
    return {
        "run_dir": run_dir,
        "run_id": run_dir.name,
        "run_dir_rel": rel_to_repo(repo_root, run_dir),
        "platform": platform,
        "summary_metadata": summary_metadata,
    }


def classify_artifact(rel_path: Path) -> str:
    path_str = rel_path.as_posix()
    if path_str in ("cases/E2E-02.json", "cases/E2E-12.json", "cases/E2E-13.json"):
        return "width-evidence"
    if path_str.endswith("summary.json") or path_str.endswith("envelope.json") or path_str.endswith("matrix.json"):
        return "machine-readable-results"
    if path_str.endswith("topology.json") or path_str.startswith("cases/"):
        return "tmux-snapshots"
    if path_str.startswith("contract-smoke/"):
        return "run-metadata"
    return "machine-readable-results"


def assemble_bundle(
    repo_root: Path,
    output_root: Path,
    bundle_id: str,
    suite_inputs: dict[str, list[Path]],
    dry_run: bool,
) -> tuple[Path, dict[str, Any]]:
    bundle_dir = (output_root / bundle_id).resolve()
    suite_runs: dict[str, list[dict[str, Any]]] = {
        suite: [run_descriptor(repo_root, run_dir) for run_dir in run_dirs]
        for suite, run_dirs in suite_inputs.items()
    }

    run_inputs = {
        "schema_version": "ezm-release-evidence-inputs/v1",
        "bundle_id": bundle_id,
        "suites": {
            suite: {
                "runs": [
                    {
                        "run_dir": descriptor["run_dir_rel"],
                        "run_id": descriptor["run_id"],
                        "platform": descriptor["platform"],
                    }
                    for descriptor in descriptors
                ],
            }
            for suite, descriptors in suite_runs.items()
        },
    }

    for suite, descriptors in suite_runs.items():
        if len(descriptors) == 1:
            run_inputs["suites"][suite]["run_dir"] = descriptors[0]["run_dir_rel"]
            run_inputs["suites"][suite]["run_id"] = descriptors[0]["run_id"]

    if dry_run:
        print(f"bundle_id={bundle_id}")
        print(f"bundle_dir={bundle_dir}")
        for suite in SUITE_ORDER:
            for descriptor in suite_runs[suite]:
                print(
                    "suite."
                    f"{suite}.run_dir={descriptor['run_dir']}"
                    f"; platform={descriptor['platform']}"
                )
        return bundle_dir, run_inputs

    if bundle_dir.exists():
        raise SystemExit(f"error: bundle directory already exists: {bundle_dir}")

    artifacts_dir = bundle_dir / "artifacts"
    artifacts_dir.mkdir(parents=True, exist_ok=False)

    artifact_records: list[dict[str, Any]] = []
    evidence_categories: dict[str, list[str]] = {
        "machine_readable_results": [],
        "tmux_structure_snapshots": [],
        "pane_width_evidence": [],
        "run_metadata": [],
    }
    run_metadata: list[dict[str, Any]] = []

    for suite in SUITE_ORDER:
        descriptors = suite_runs[suite]
        platform_counts: dict[str, int] = {}
        for descriptor in descriptors:
            platform = descriptor["platform"]
            platform_counts[platform] = platform_counts.get(platform, 0) + 1

        for index, descriptor in enumerate(descriptors):
            run_dir = descriptor["run_dir"]
            platform = descriptor["platform"]
            run_id = descriptor["run_id"]
            selected = selected_relative_files(run_dir)

            platform_segment: str | None = None
            if index > 0:
                platform_segment = platform
                if platform_counts[platform] > 1:
                    platform_segment = f"{platform}-{run_id}"

            suite_root = Path("artifacts") / suite
            if platform_segment:
                suite_root = suite_root / platform_segment

            for rel_path in selected:
                source = run_dir / rel_path
                bundle_rel = suite_root / rel_path
                target = bundle_dir / bundle_rel
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(source, target)

                checksum = sha256_file(target)
                record = {
                    "suite": suite,
                    "run_id": run_id,
                    "platform": platform,
                    "category": classify_artifact(rel_path),
                    "path": bundle_rel.as_posix(),
                    "sha256": checksum,
                    "size_bytes": target.stat().st_size,
                    "source_path": rel_to_repo(repo_root, source),
                }
                artifact_records.append(record)

                if record["category"] == "machine-readable-results":
                    evidence_categories["machine_readable_results"].append(record["path"])
                if record["category"] == "tmux-snapshots":
                    evidence_categories["tmux_structure_snapshots"].append(record["path"])
                if record["category"] == "width-evidence":
                    evidence_categories["pane_width_evidence"].append(record["path"])

            summary_metadata = descriptor["summary_metadata"]
            if summary_metadata:
                run_metadata.append(
                    {
                        "suite": suite,
                        "run_id": run_id,
                        "platform": platform,
                        "summary_source": rel_to_repo(repo_root, run_dir / "summary.json"),
                        "commit_sha": summary_metadata.get("commit_sha", "unknown"),
                        "os": summary_metadata.get("os", "unknown"),
                        "shell": summary_metadata.get("shell", "unknown"),
                        "tmux_version": summary_metadata.get("tmux_version", "unknown"),
                        "test_ids": summary_metadata.get("test_ids", []),
                        "pass_total": summary_metadata.get("pass_total", 0),
                        "fail_total": summary_metadata.get("fail_total", 0),
                    }
                )

    run_metadata.sort(key=lambda item: (item["suite"], item.get("platform", "unknown"), item.get("run_id", "")))
    evidence_categories["run_metadata"] = [
        f"{entry['suite']}:{entry.get('platform', 'unknown')}" for entry in run_metadata
    ]

    artifact_records.sort(key=lambda item: (item["suite"], item["path"]))
    for key in evidence_categories:
        evidence_categories[key] = sorted(set(evidence_categories[key]))

    manifest = {
        "schema_version": SCHEMA_VERSION,
        "plan_section": "10",
        "bundle_id": bundle_id,
        "inputs": run_inputs,
        "artifacts": artifact_records,
        "evidence_index": evidence_categories,
        "run_metadata": run_metadata,
    }

    (bundle_dir / "run-inputs.json").write_text(
        json.dumps(run_inputs, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    (bundle_dir / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    return bundle_dir, manifest


def run_reproducibility_check(repo_root: Path, manifest_path: Path) -> int:
    if not manifest_path.is_file():
        raise SystemExit(f"error: manifest not found: {manifest_path}")

    bundle_dir = manifest_path.parent
    with manifest_path.open("r", encoding="utf-8") as handle:
        original_manifest = json.load(handle)

    inputs_path = bundle_dir / "run-inputs.json"
    if not inputs_path.is_file():
        raise SystemExit(f"error: run inputs not found beside manifest: {inputs_path}")
    with inputs_path.open("r", encoding="utf-8") as handle:
        recorded_inputs = json.load(handle)

    suites: dict[str, list[Path]] = {}
    for suite in SUITE_ORDER:
        suite_entry = recorded_inputs.get("suites", {}).get(suite, {})
        if not isinstance(suite_entry, dict):
            raise SystemExit(f"error: recorded inputs missing suite entry for `{suite}`")

        resolved_runs: list[Path] = []
        runs = suite_entry.get("runs")
        if isinstance(runs, list) and runs:
            for run_entry in runs:
                if not isinstance(run_entry, dict):
                    continue
                run_dir_raw = run_entry.get("run_dir")
                if not isinstance(run_dir_raw, str):
                    continue
                run_dir = (repo_root / run_dir_raw).resolve() if not Path(run_dir_raw).is_absolute() else Path(run_dir_raw).resolve()
                if not run_dir.is_dir():
                    raise SystemExit(f"error: recorded run directory missing for suite `{suite}`: {run_dir}")
                resolved_runs.append(run_dir)

        if not resolved_runs:
            run_dir_raw = suite_entry.get("run_dir")
            if not isinstance(run_dir_raw, str):
                raise SystemExit(f"error: recorded inputs missing run_dir for suite `{suite}`")
            run_dir = (repo_root / run_dir_raw).resolve() if not Path(run_dir_raw).is_absolute() else Path(run_dir_raw).resolve()
            if not run_dir.is_dir():
                raise SystemExit(f"error: recorded run directory missing for suite `{suite}`: {run_dir}")
            resolved_runs.append(run_dir)

        suites[suite] = resolved_runs

    bundle_id = recorded_inputs.get("bundle_id")
    if not isinstance(bundle_id, str) or not bundle_id:
        raise SystemExit("error: recorded inputs missing bundle_id")

    with tempfile.TemporaryDirectory(prefix="ezm-release-rebuild-") as temp_dir:
        output_root = Path(temp_dir)
        rebuilt_dir, rebuilt_manifest = assemble_bundle(
            repo_root=repo_root,
            output_root=output_root,
            bundle_id=bundle_id,
            suite_inputs=suites,
            dry_run=False,
        )
        rebuilt_manifest_path = rebuilt_dir / "manifest.json"

        if rebuilt_manifest != original_manifest:
            print("reproducibility check: failed")
            print(f"expected manifest: {manifest_path}")
            print(f"rebuilt manifest: {rebuilt_manifest_path}")
            return 1

    print("reproducibility check: passed")
    print(f"manifest={manifest_path}")
    return 0


def normalize_os_label(raw: str) -> str:
    normalized = raw.strip().lower()
    aliases = {
        "darwin": "macos",
        "mac": "macos",
        "osx": "macos",
        "macos": "macos",
        "linux": "linux",
    }
    return aliases.get(normalized, normalized)


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def make_blocker(code: str, message: str, details: dict[str, Any] | None = None) -> dict[str, Any]:
    blocker: dict[str, Any] = {"code": code, "message": message}
    if details:
        blocker["details"] = details
    return blocker


def evaluate_release_gate(manifest_path: Path) -> dict[str, Any]:
    manifest = load_json(manifest_path)
    bundle_dir = manifest_path.parent
    artifact_records = manifest.get("artifacts")
    run_metadata = manifest.get("run_metadata")
    evidence_index = manifest.get("evidence_index")
    bundle_id = manifest.get("bundle_id", "unknown")

    blockers: list[dict[str, Any]] = []
    checks: list[dict[str, Any]] = []

    if not isinstance(artifact_records, list):
        blockers.append(
            make_blocker(
                "manifest-artifacts-missing",
                "manifest is missing artifacts list required for release gate evaluation",
            )
        )
        artifact_records = []
    if not isinstance(run_metadata, list):
        blockers.append(
            make_blocker(
                "manifest-run-metadata-missing",
                "manifest is missing run_metadata required for release gate evaluation",
            )
        )
        run_metadata = []
    if not isinstance(evidence_index, dict):
        blockers.append(
            make_blocker(
                "manifest-evidence-index-missing",
                "manifest is missing evidence_index required for release gate evaluation",
            )
        )
        evidence_index = {}

    artifact_paths = {
        str(path)
        for path in (
            record.get("path")
            for record in artifact_records
            if isinstance(record, dict)
        )
        if path
    }

    required_artifact_paths = {path for path in REQUIRED_ARTIFACT_PATHS}
    missing_artifact_paths = sorted(required_artifact_paths - artifact_paths)
    if missing_artifact_paths:
        blockers.append(
            make_blocker(
                "required-artifacts-missing",
                "required release artifacts are missing",
                {"missing_paths": missing_artifact_paths},
            )
        )

    required_evidence_categories = {
        "machine_readable_results",
        "tmux_structure_snapshots",
        "pane_width_evidence",
        "run_metadata",
    }
    missing_categories = sorted(
        category
        for category in required_evidence_categories
        if not isinstance(evidence_index.get(category), list) or not evidence_index.get(category)
    )
    if missing_categories:
        blockers.append(
            make_blocker(
                "required-evidence-categories-missing",
                "manifest evidence index is missing required non-empty categories",
                {"missing_categories": missing_categories},
            )
        )

    for e2e_id in ("E2E-12", "E2E-13"):
        case_rel = f"artifacts/core-session-orchestration/cases/{e2e_id}.json"
        case_path = bundle_dir / case_rel
        if not case_path.is_file():
            blockers.append(
                make_blocker(
                    f"{e2e_id.lower()}-artifact-missing",
                    f"required preset case artifact {e2e_id} is missing",
                    {"path": case_rel},
                )
            )
            continue

        case_json = load_json(case_path)
        passed = isinstance(case_json, dict) and bool(case_json.get("pass"))
        if not passed:
            blockers.append(
                make_blocker(
                    f"{e2e_id.lower()}-failed",
                    f"required preset case {e2e_id} is not marked pass",
                    {"path": case_rel},
                )
            )

    smoke_summary_paths = sorted(
        path
        for path in artifact_paths
        if path.startswith("artifacts/cross-platform-smoke/") and path.endswith("summary.json")
    )
    if not smoke_summary_paths and "artifacts/cross-platform-smoke/summary.json" in artifact_paths:
        smoke_summary_paths = ["artifacts/cross-platform-smoke/summary.json"]

    smoke_pass = False
    for rel_path in smoke_summary_paths:
        smoke_summary_path = bundle_dir / rel_path
        if not smoke_summary_path.is_file():
            continue
        smoke_summary = load_json(smoke_summary_path)
        smoke_metadata = smoke_summary.get("metadata") if isinstance(smoke_summary, dict) else None
        if isinstance(smoke_metadata, dict) and smoke_metadata.get("fail_total", 1) == 0:
            smoke_pass = True
            break

    if not smoke_pass:
        blockers.append(
            make_blocker(
                "e2e-14-missing-or-failed",
                "required cross-platform smoke evidence (E2E-14) is missing or failing",
                {"summary_paths": smoke_summary_paths},
            )
        )

    metadata_entries: list[dict[str, Any]] = [
        entry for entry in run_metadata if isinstance(entry, dict)
    ]
    suites_present = {entry.get("suite") for entry in metadata_entries if isinstance(entry.get("suite"), str)}
    missing_suites = sorted(set(REQUIRED_RELEASE_SUITES) - suites_present)
    if missing_suites:
        blockers.append(
            make_blocker(
                "required-suite-metadata-missing",
                "run metadata does not include all required suites",
                {"missing_suites": missing_suites},
            )
        )

    coverage: dict[str, set[str]] = {platform: set() for platform in REQUIRED_OS}
    failed_entries: list[dict[str, Any]] = []
    for entry in metadata_entries:
        os_raw = entry.get("os")
        platform_raw = entry.get("platform")
        suite = entry.get("suite")
        platform = None
        if isinstance(platform_raw, str):
            platform = normalize_os_label(platform_raw)
        elif isinstance(os_raw, str):
            platform = normalize_os_label(os_raw)

        if platform is None:
            continue
        if platform not in coverage:
            continue
        test_ids = entry.get("test_ids")
        fail_total = entry.get("fail_total")
        if isinstance(fail_total, int) and fail_total > 0:
            failed_entries.append(
                {
                    "suite": suite,
                    "os": platform,
                    "fail_total": fail_total,
                }
            )
            continue
        if isinstance(test_ids, list):
            coverage[platform].update(test_id for test_id in test_ids if isinstance(test_id, str))
        if suite == "cross-platform-smoke":
            coverage[platform].add("E2E-14")

    if failed_entries:
        blockers.append(
            make_blocker(
                "suite-failures-present",
                "one or more suite summaries report failing tests",
                {"failing_entries": failed_entries},
            )
        )

    required_regression = set(FULL_REGRESSION_IDS)
    missing_by_os = {
        platform: sorted(required_regression - coverage[platform]) for platform in REQUIRED_OS
    }
    for platform, missing_ids in missing_by_os.items():
        if missing_ids:
            blockers.append(
                make_blocker(
                    f"carry-forward-missing-{platform}",
                    f"full carry-forward regression is incomplete for {platform}",
                    {"missing_test_ids": missing_ids},
                )
            )

    checks.append(
        {
            "id": "gate-required-e2e-and-artifacts",
            "description": "fail on missing E2E-12/13/14 evidence or required artifacts",
            "passed": not any(
                blocker["code"].startswith(prefix)
                for blocker in blockers
                for prefix in (
                    "required-artifacts",
                    "required-evidence-categories",
                    "e2e-12",
                    "e2e-13",
                    "e2e-14",
                    "required-suite-metadata",
                )
            ),
        }
    )
    checks.append(
        {
            "id": "gate-full-carry-forward-linux-macos",
            "description": "require Linux and macOS regression pass coverage for E2E-00..E2E-19",
            "passed": not any(
                blocker["code"].startswith("carry-forward-missing") or blocker["code"] == "suite-failures-present"
                for blocker in blockers
            ),
            "details": {
                platform: sorted(coverage[platform]) for platform in REQUIRED_OS
            },
        }
    )

    passed = not blockers
    return {
        "schema_version": GATE_SCHEMA_VERSION,
        "gate": "release-readiness",
        "bundle_id": bundle_id,
        "manifest_path": str(manifest_path),
        "evaluated_at_unix": int(time.time()),
        "passed": passed,
        "checks": checks,
        "blocking_reasons": blockers,
        "carry_forward_requirement": {
            "platforms": list(REQUIRED_OS),
            "required_test_ids": list(FULL_REGRESSION_IDS),
        },
    }


def run_release_gate_evaluation(manifest_path: Path, decision_output: str | None, dry_run: bool) -> int:
    if not manifest_path.is_file():
        raise SystemExit(f"error: manifest not found: {manifest_path}")

    decision = evaluate_release_gate(manifest_path)
    decision_path = (
        Path(decision_output).resolve()
        if decision_output
        else (manifest_path.parent / "gate-decision.json").resolve()
    )
    decision["decision_path"] = str(decision_path)

    if dry_run:
        print("mode=dry-run")
        print(f"manifest={manifest_path}")
        print(f"decision_path={decision_path}")
        print(json.dumps(decision, indent=2, sort_keys=True))
        return 0 if decision["passed"] else 2

    decision_path.parent.mkdir(parents=True, exist_ok=True)
    decision_path.write_text(
        json.dumps(decision, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )

    status = "passed" if decision["passed"] else "failed"
    print(f"release gate evaluation: {status}")
    print(f"manifest={manifest_path}")
    print(f"decision={decision_path}")
    return 0 if decision["passed"] else 2


def main() -> int:
    args = parse_args()
    repo_root = Path(__file__).resolve().parents[2]

    if args.check_reproducible:
        if not args.manifest:
            raise SystemExit("error: --manifest is required with --check-reproducible")
        return run_reproducibility_check(repo_root, Path(args.manifest).resolve())

    if args.evaluate_gate:
        if not args.manifest:
            raise SystemExit("error: --manifest is required with --evaluate-gate")
        return run_release_gate_evaluation(
            Path(args.manifest).resolve(),
            args.decision_output,
            args.dry_run,
        )

    suite_inputs = discover_suite_inputs(repo_root, args)
    output_root = (repo_root / args.output_root).resolve() if not Path(args.output_root).is_absolute() else Path(args.output_root).resolve()
    if not args.dry_run:
        output_root.mkdir(parents=True, exist_ok=True)

    bundle_id = args.bundle_id or generated_bundle_id()
    bundle_dir, _ = assemble_bundle(
        repo_root=repo_root,
        output_root=output_root,
        bundle_id=bundle_id,
        suite_inputs=suite_inputs,
        dry_run=args.dry_run,
    )

    if args.dry_run:
        print("mode=dry-run")
        return 0

    print("release evidence bundle assembled")
    print(f"bundle_dir={bundle_dir}")
    print(f"manifest={bundle_dir / 'manifest.json'}")
    print(f"run_inputs={bundle_dir / 'run-inputs.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
