"""Evaluate workflow, archive, manifest, and E2E release-readiness gates."""

from __future__ import annotations

import json
import re
import time
from pathlib import Path
from typing import Any

from release_e2e import (
    _is_release_commit_sha,
    _read_json_object,
    validate_e2e_case_evidence,
)
from release_evidence import (
    FULL_REGRESSION_IDS,
    GATE_SCHEMA_VERSION,
    REQUIRED_ARTIFACT_PATHS,
    REQUIRED_NATIVE_PLATFORMS,
    REQUIRED_OS,
    REQUIRED_RELEASE_ASSETS,
    REQUIRED_RELEASE_SUITES,
    REQUIRED_WORKFLOW_JOBS,
    SMOKE_CASE_IDS,
    load_json,
    make_blocker,
    normalize_os_label,
    safe_bundle_path,
    sha256_file,
)
from release_manifest import manifest_records_by_category, validate_manifest_artifacts


def evaluate_release_gate(manifest_path: Path, workflow_results_path: Path | None = None) -> dict[str, Any]:
    manifest = load_json(manifest_path)
    bundle_dir = manifest_path.parent
    artifact_records = manifest.get("artifacts")
    run_metadata = manifest.get("run_metadata")
    evidence_index = manifest.get("evidence_index")
    bundle_id = manifest.get("bundle_id", "unknown")

    blockers: list[dict[str, Any]] = []
    checks: list[dict[str, Any]] = []
    validated_commit_sha: str | None = None

    if workflow_results_path is not None:
        if not workflow_results_path.is_file():
            blockers.append(
                make_blocker(
                    "workflow-results-missing",
                    "actual workflow needs results are required for release evidence",
                    {"path": str(workflow_results_path)},
                )
            )
        else:
            workflow_results = load_json(workflow_results_path)
            workflow_jobs = workflow_results.get("jobs") if isinstance(workflow_results, dict) else None
            workflow_inputs = workflow_results.get("inputs") if isinstance(workflow_results, dict) else None
            if not isinstance(workflow_jobs, dict):
                blockers.append(
                    make_blocker(
                        "workflow-job-results-missing",
                        "workflow results are missing the jobs map",
                    )
                )
                workflow_jobs = {}
            if not isinstance(workflow_inputs, dict):
                blockers.append(
                    make_blocker(
                        "workflow-inputs-missing",
                        "workflow results are missing validated release inputs",
                    )
                )
                workflow_inputs = {}
            missing_inputs = sorted(
                key
                for key in ("tag", "version", "commit_sha")
                if (
                    not isinstance(workflow_inputs.get(key), str)
                    or not workflow_inputs[key]
                    or (key == "commit_sha" and not _is_release_commit_sha(workflow_inputs[key]))
                )
            )
            if missing_inputs:
                blockers.append(
                    make_blocker(
                        "workflow-inputs-invalid",
                        "workflow results contain incomplete validated release inputs",
                        {"missing_inputs": missing_inputs},
                    )
                )
            if _is_release_commit_sha(workflow_inputs.get("commit_sha")):
                validated_commit_sha = workflow_inputs["commit_sha"]
            for job in REQUIRED_WORKFLOW_JOBS:
                result = workflow_jobs.get(job)
                if result != "success":
                    blockers.append(
                        make_blocker(
                            "workflow-job-not-success",
                            f"required workflow job `{job}` did not succeed",
                            {"job": job, "result": result if isinstance(result, str) else "missing"},
                        )
                    )
            checks.append(
                {
                    "id": "gate-workflow-needs-results",
                    "description": "require every release prerequisite job to report success",
                    "passed": not any(
                        blocker["code"] in {
                            "workflow-results-missing",
                            "workflow-job-results-missing",
                            "workflow-inputs-missing",
                            "workflow-inputs-invalid",
                            "workflow-job-not-success",
                        }
                        for blocker in blockers
                    ),
                    "jobs": {job: workflow_jobs.get(job, "missing") for job in REQUIRED_WORKFLOW_JOBS},
                    "inputs": workflow_inputs,
                }
            )

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

    blockers.extend(validate_manifest_artifacts(bundle_dir, artifact_records))

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
        "install_validation_summaries",
        "native_verification",
        "release_assets",
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

    native_artifacts = manifest_records_by_category(artifact_records, "native-verification")
    for category, records in (
        ("native_verification", native_artifacts),
        (
            "install_validation_summaries",
            [
                record
                for record in artifact_records
                if isinstance(record, dict)
                and record.get("suite") == "install-validation"
                and isinstance(record.get("path"), str)
                and record["path"].endswith("/summary.json")
            ],
        ),
        ("release_assets", manifest_records_by_category(artifact_records, "release-assets")),
    ):
        indexed_paths = evidence_index.get(category)
        record_paths = {record.get("path") for record in records}
        if (
            not isinstance(indexed_paths, list)
            or not all(isinstance(path, str) for path in indexed_paths)
            or set(indexed_paths) != record_paths
        ):
            blockers.append(
                make_blocker(
                    "evidence-index-mismatch",
                    "the evidence index must enumerate every hashed record in its category exactly",
                    {"category": category},
                )
            )
    native_by_platform: dict[str, dict[str, Any]] = {}
    for record in native_artifacts:
        platform_raw = record.get("platform")
        platform = normalize_os_label(platform_raw) if isinstance(platform_raw, str) else "unknown"
        if platform in native_by_platform:
            blockers.append(
                make_blocker(
                    "native-verification-duplicate",
                    "the evaluated manifest must contain exactly one native verification record per platform",
                    {"platform": platform},
                )
            )
        native_by_platform[platform] = record

        path_raw = record.get("path")
        if not isinstance(path_raw, str):
            continue
        native_path = safe_bundle_path(bundle_dir, path_raw)
        if native_path is None or not native_path.is_file():
            continue
        try:
            native_json = load_json(native_path)
        except (OSError, json.JSONDecodeError):
            native_json = None
        if not isinstance(native_json, dict):
            blockers.append(
                make_blocker(
                    "native-verification-invalid",
                    "native verification records must contain JSON objects",
                    {"path": path_raw},
                )
            )
            continue
        if native_json.get("status") != "passed" or normalize_os_label(str(native_json.get("platform", ""))) != platform:
            blockers.append(
                make_blocker(
                    "native-verification-failed",
                    "native verification records must be passed and match their manifest platform",
                    {"path": path_raw, "platform": platform},
                )
            )
        archive = native_json.get("archive")
        if not isinstance(archive, dict) or re.fullmatch(
            r"[0-9a-f]{64}", str(archive.get("archive_sha256", ""))
        ) is None:
            blockers.append(
                make_blocker(
                    "native-verification-identity-missing",
                    "native verification records must hash the exact verified release archive",
                    {"path": path_raw},
                )
            )

    missing_native = sorted(set(REQUIRED_NATIVE_PLATFORMS) - set(native_by_platform))
    if missing_native:
        blockers.append(
            make_blocker(
                "native-verification-records-missing",
                "the evaluated manifest must enumerate native verification records for Linux and macOS",
                {"missing_platforms": missing_native},
            )
        )
    declared_native = manifest.get("native_verification")
    declared_native_keys = {
        (entry.get("platform"), entry.get("path"), entry.get("sha256"))
        for entry in declared_native
        if isinstance(entry, dict)
    } if isinstance(declared_native, list) else set()
    actual_native_keys = {
        (record.get("platform"), record.get("path"), record.get("sha256"))
        for record in native_artifacts
    }
    if declared_native_keys != actual_native_keys or not isinstance(declared_native, list):
        blockers.append(
            make_blocker(
                "native-verification-index-mismatch",
                "native verification index must enumerate the hashed artifact records exactly",
            )
        )

    install_summary_artifacts = [
        record
        for record in artifact_records
        if isinstance(record, dict)
        and record.get("suite") == "install-validation"
        and isinstance(record.get("path"), str)
        and record["path"].endswith("/summary.json")
    ]
    install_by_platform: dict[str, dict[str, Any]] = {}
    for record in install_summary_artifacts:
        platform_raw = record.get("platform")
        platform = normalize_os_label(platform_raw) if isinstance(platform_raw, str) else "unknown"
        if platform in install_by_platform:
            blockers.append(
                make_blocker(
                    "install-summary-duplicate",
                    "the evaluated manifest must contain exactly one install summary per platform",
                    {"platform": platform},
                )
            )
        install_by_platform[platform] = record
        summary_path = safe_bundle_path(bundle_dir, record["path"])
        if summary_path is None or not summary_path.is_file():
            continue
        try:
            summary = load_json(summary_path)
        except (OSError, json.JSONDecodeError):
            summary = None
        metadata = summary.get("metadata") if isinstance(summary, dict) else None
        if (
            not isinstance(summary, dict)
            or summary.get("status") != "passed"
            or normalize_os_label(str(summary.get("platform", ""))) != platform
            or not isinstance(metadata, dict)
            or metadata.get("os") != platform
            or metadata.get("test_ids") != ["E2E-00"]
            or metadata.get("pass_total") != 1
            or metadata.get("fail_total") != 0
        ):
            blockers.append(
                make_blocker(
                    "install-summary-failed",
                    "install-validation summaries must be passed, positive, and match their manifest platform",
                    {"path": record["path"], "platform": platform},
                )
            )
    missing_install = sorted(set(REQUIRED_NATIVE_PLATFORMS) - set(install_by_platform))
    if missing_install:
        blockers.append(
            make_blocker(
                "install-summaries-missing",
                "the evaluated manifest must enumerate install summaries for Linux and macOS",
                {"missing_platforms": missing_install},
            )
        )

    release_asset_artifacts = manifest_records_by_category(artifact_records, "release-assets")
    release_assets = {record.get("asset") for record in release_asset_artifacts}
    missing_assets = sorted(set(REQUIRED_RELEASE_ASSETS) - release_assets)
    if missing_assets:
        blockers.append(
            make_blocker(
                "release-assets-missing",
                "the evaluated bundle must contain every exact publishable release archive",
                {"missing_assets": missing_assets},
            )
        )
    release_asset_by_name = {
        Path(str(record.get("path"))).name: record for record in release_asset_artifacts
    }
    for platform, native_record in native_by_platform.items():
        native_path_raw = native_record.get("path")
        if not isinstance(native_path_raw, str):
            continue
        native_json_path = safe_bundle_path(bundle_dir, native_path_raw)
        if native_json_path is None or not native_json_path.is_file():
            continue
        try:
            native_json = load_json(native_json_path)
        except (OSError, json.JSONDecodeError):
            continue
        archive = native_json.get("archive") if isinstance(native_json, dict) else None
        archive_name = Path(str(archive.get("path"))).name if isinstance(archive, dict) else ""
        archive_hash = archive.get("archive_sha256") if isinstance(archive, dict) else None
        asset_record = release_asset_by_name.get(archive_name)
        if not isinstance(asset_record, dict) or asset_record.get("sha256") != archive_hash:
            blockers.append(
                make_blocker(
                    "native-release-archive-mismatch",
                    "native verification must identify the exact archive in the evaluated bundle",
                    {"platform": platform, "archive": archive_name},
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

        case_json = _read_json_object(case_path)
        passed = case_json is not None and case_json.get("pass") is True
        if not passed:
            blockers.append(
                make_blocker(
                    f"{e2e_id.lower()}-failed",
                    f"required preset case {e2e_id} is not marked pass",
                    {"path": case_rel},
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

    e2e_blockers, coverage, failed_entries = validate_e2e_case_evidence(
        bundle_dir,
        artifact_records,
        metadata_entries,
        validated_commit_sha,
    )
    blockers.extend(e2e_blockers)

    smoke_pass = all(
        set(SMOKE_CASE_IDS).issubset(coverage[platform]) for platform in REQUIRED_OS
    )
    if not smoke_pass:
        blockers.append(
            make_blocker(
                "e2e-smoke-missing-or-failed",
                "required cross-platform smoke case evidence is missing or failing",
                {"required_case_ids": list(SMOKE_CASE_IDS)},
            )
        )

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
            "description": "fail on missing E2E case evidence or required artifacts",
            "passed": not any(
                blocker["code"].startswith(prefix)
                for blocker in blockers
                for prefix in (
                    "required-artifacts",
                    "required-evidence-categories",
                    "e2e-12",
                    "e2e-13",
                    "e2e-smoke",
                    "e2e-summary",
                    "e2e-case",
                    "required-suite-metadata",
                )
            ),
        }
    )
    checks.append(
        {
            "id": "gate-full-carry-forward-linux-macos",
            "description": "require Linux and macOS regression pass coverage for every current E2E scenario",
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
        "manifest_sha256": sha256_file(manifest_path),
        "evaluated_at_unix": int(time.time()),
        "passed": passed,
        "checks": checks,
        "blocking_reasons": blockers,
        "carry_forward_requirement": {
            "platforms": list(REQUIRED_OS),
            "required_test_ids": list(FULL_REGRESSION_IDS),
        },
    }


def run_release_gate_evaluation(
    manifest_path: Path,
    decision_output: str | None,
    dry_run: bool,
    workflow_results_path: Path | None = None,
) -> int:
    if not manifest_path.is_file():
        raise SystemExit(f"error: manifest not found: {manifest_path}")

    decision = evaluate_release_gate(manifest_path, workflow_results_path)
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
