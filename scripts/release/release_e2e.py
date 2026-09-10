"""Validate generated E2E case evidence in an assembled release bundle."""

from __future__ import annotations

import json
from pathlib import Path, PurePosixPath
from typing import Any

from release_evidence import (
    EXPECTED_CASE_IDS_BY_SUITE,
    REQUIRED_OS,
    load_json,
    make_blocker,
    normalize_os_label,
    safe_bundle_path,
)


def _is_json_int(value: object) -> bool:
    return type(value) is int


def _is_release_commit_sha(value: object) -> bool:
    return isinstance(value, str) and bool(value.strip()) and value != "unknown"


def _case_artifact_id(path_raw: object) -> str | None:
    if not isinstance(path_raw, str):
        return None
    path = PurePosixPath(path_raw)
    try:
        cases_index = path.parts.index("cases")
    except ValueError:
        return None
    if len(path.parts) != cases_index + 2 or path.suffix != ".json":
        return None
    return path.stem


def _read_json_object(path: Path) -> dict[str, Any] | None:
    try:
        decoded = load_json(path)
    except (OSError, json.JSONDecodeError):
        return None
    return decoded if isinstance(decoded, dict) else None


def validate_e2e_case_evidence(
    bundle_dir: Path,
    artifact_records: list[Any],
    run_metadata: list[Any],
    expected_commit_sha: str | None = None,
) -> tuple[list[dict[str, Any]], dict[str, set[str]], list[dict[str, Any]]]:
    """Validate the generated E2E schema instead of trusting summary metadata.

    Foundation, core, and smoke intentionally cover different case sets.  Each
    set is required once per release platform, while full carry-forward
    coverage is the union of the sets that the checked-in suites actually run.
    """

    blockers: list[dict[str, Any]] = []
    coverage: dict[str, set[str]] = {platform: set() for platform in REQUIRED_OS}
    failed_entries: list[dict[str, Any]] = []
    seen_runs: set[tuple[str, str, str]] = set()

    for suite, expected_case_ids in EXPECTED_CASE_IDS_BY_SUITE.items():
        expected_ids = set(expected_case_ids)
        for platform in REQUIRED_OS:
            entries = [
                entry
                for entry in run_metadata
                if isinstance(entry, dict)
                and entry.get("suite") == suite
                and normalize_os_label(str(entry.get("platform", ""))) == platform
            ]
            if not entries:
                blockers.append(
                    make_blocker(
                        "e2e-summary-missing",
                        "each required E2E suite must have a summary for every required OS",
                        {"suite": suite, "platform": platform},
                    )
                )
                continue

            for entry in entries:
                run_id = entry.get("run_id")
                if not isinstance(run_id, str) or not run_id:
                    blockers.append(
                        make_blocker(
                            "e2e-run-metadata-invalid",
                            "E2E run metadata must identify a non-empty run",
                            {"suite": suite, "platform": platform},
                        )
                    )
                    continue
                run_key = (suite, platform, str(run_id))
                if run_key in seen_runs:
                    blockers.append(
                        make_blocker(
                            "e2e-summary-duplicate",
                            "the evaluated manifest must not repeat an E2E run",
                            {"suite": suite, "platform": platform, "run_id": run_id},
                        )
                    )
                    continue
                seen_runs.add(run_key)

                summary_records = [
                    record
                    for record in artifact_records
                    if isinstance(record, dict)
                    and record.get("suite") == suite
                    and record.get("run_id") == run_id
                    and normalize_os_label(str(record.get("platform", ""))) == platform
                    and isinstance(record.get("path"), str)
                    and PurePosixPath(record["path"]).name == "summary.json"
                ]
                if len(summary_records) != 1:
                    blockers.append(
                        make_blocker(
                            "e2e-summary-artifact-missing",
                            "each E2E run must have exactly one hashed summary artifact",
                            {
                                "suite": suite,
                                "platform": platform,
                                "run_id": run_id,
                                "summary_count": len(summary_records),
                            },
                        )
                    )
                    continue

                summary_record = summary_records[0]
                summary_path = safe_bundle_path(bundle_dir, summary_record.get("path"))
                summary = _read_json_object(summary_path) if summary_path else None
                metadata = summary.get("metadata") if isinstance(summary, dict) else None
                cases = summary.get("cases") if isinstance(summary, dict) else None
                if not isinstance(metadata, dict) or not isinstance(cases, list):
                    blockers.append(
                        make_blocker(
                            "e2e-summary-schema-invalid",
                            "E2E summaries must contain metadata and a cases list",
                            {"path": summary_record.get("path"), "suite": suite, "platform": platform},
                        )
                    )
                    continue

                actual_summary_os = metadata.get("os")
                if (
                    not isinstance(actual_summary_os, str)
                    or normalize_os_label(actual_summary_os) != platform
                    or normalize_os_label(str(entry.get("os", ""))) != platform
                ):
                    blockers.append(
                        make_blocker(
                            "e2e-summary-platform-mismatch",
                            "E2E summary, manifest metadata, and required platform must agree",
                            {"path": summary_record.get("path"), "suite": suite, "platform": platform},
                        )
                    )
                if metadata.get("run_id") != run_id:
                    blockers.append(
                        make_blocker(
                            "e2e-summary-run-id-mismatch",
                            "E2E summary metadata must identify the run represented by its artifact",
                            {"path": summary_record.get("path"), "suite": suite, "platform": platform, "run_id": run_id},
                        )
                    )
                summary_commit_sha = metadata.get("commit_sha")
                entry_commit_sha = entry.get("commit_sha")
                if not _is_release_commit_sha(summary_commit_sha):
                    blockers.append(
                        make_blocker(
                            "e2e-summary-commit-sha-missing",
                            "E2E summary metadata must contain a release commit SHA",
                            {"path": summary_record.get("path"), "suite": suite, "platform": platform},
                        )
                    )
                if not _is_release_commit_sha(entry_commit_sha):
                    blockers.append(
                        make_blocker(
                            "e2e-run-commit-sha-missing",
                            "E2E run metadata must contain a release commit SHA",
                            {"suite": suite, "platform": platform, "run_id": run_id},
                        )
                    )
                if _is_release_commit_sha(summary_commit_sha) and _is_release_commit_sha(entry_commit_sha) and summary_commit_sha != entry_commit_sha:
                    blockers.append(
                        make_blocker(
                            "e2e-commit-sha-mismatch",
                            "E2E summary and run metadata commit SHAs must agree",
                            {"suite": suite, "platform": platform, "run_id": run_id},
                        )
                    )
                if (
                    expected_commit_sha is not None
                    and _is_release_commit_sha(summary_commit_sha)
                    and summary_commit_sha != expected_commit_sha
                ):
                    blockers.append(
                        make_blocker(
                            "e2e-commit-sha-mismatch",
                            "E2E summary commit SHA must match the validated workflow commit SHA",
                            {"suite": suite, "platform": platform, "run_id": run_id, "source": "summary"},
                        )
                    )
                if (
                    expected_commit_sha is not None
                    and _is_release_commit_sha(entry_commit_sha)
                    and entry_commit_sha != expected_commit_sha
                ):
                    blockers.append(
                        make_blocker(
                            "e2e-commit-sha-mismatch",
                            "E2E run metadata commit SHA must match the validated workflow commit SHA",
                            {"suite": suite, "platform": platform, "run_id": run_id, "source": "run_metadata"},
                        )
                    )

                metadata_ids = metadata.get("test_ids")
                metadata_id_set = (
                    set(metadata_ids)
                    if isinstance(metadata_ids, list) and all(isinstance(item, str) for item in metadata_ids)
                    else set()
                )
                if (
                    not isinstance(metadata_ids, list)
                    or len(metadata_ids) != len(metadata_id_set)
                    or metadata_id_set != expected_ids
                ):
                    blockers.append(
                        make_blocker(
                            "e2e-summary-scenario-ids-invalid",
                            "E2E summary metadata must enumerate the cases run by its suite exactly",
                            {
                                "path": summary_record.get("path"),
                                "suite": suite,
                                "missing_test_ids": sorted(expected_ids - metadata_id_set),
                                "unexpected_test_ids": sorted(metadata_id_set - expected_ids),
                            },
                        )
                    )

                summary_cases: dict[str, dict[str, Any]] = {}
                invalid_case_entries = 0
                for case in cases:
                    if not isinstance(case, dict) or not isinstance(case.get("id"), str) or not isinstance(case.get("pass"), bool):
                        invalid_case_entries += 1
                        continue
                    case_id = case["id"]
                    if case_id in summary_cases:
                        invalid_case_entries += 1
                        continue
                    summary_cases[case_id] = case

                if (
                    invalid_case_entries
                    or set(summary_cases) != expected_ids
                    or any(case_id not in expected_ids for case_id in summary_cases)
                ):
                    blockers.append(
                        make_blocker(
                            "e2e-summary-cases-incomplete",
                            "E2E summary cases must contain every expected case exactly once",
                            {
                                "path": summary_record.get("path"),
                                "suite": suite,
                                "missing_case_ids": sorted(expected_ids - set(summary_cases)),
                                "unexpected_case_ids": sorted(set(summary_cases) - expected_ids),
                                "invalid_case_entries": invalid_case_entries,
                            },
                        )
                    )

                pass_total = metadata.get("pass_total")
                fail_total = metadata.get("fail_total")
                actual_pass_total = sum(case.get("pass") is True for case in summary_cases.values())
                actual_fail_total = sum(case.get("pass") is False for case in summary_cases.values())
                if (
                    not _is_json_int(pass_total)
                    or not _is_json_int(fail_total)
                    or pass_total <= 0
                    or fail_total < 0
                    or pass_total != actual_pass_total
                    or fail_total != actual_fail_total
                    or pass_total + fail_total != len(cases)
                    or pass_total != len(expected_ids)
                    or fail_total != 0
                ):
                    blockers.append(
                        make_blocker(
                            "e2e-summary-counts-invalid",
                            "E2E summary counts must be positive, consistent with cases, and entirely passing",
                            {
                                "path": summary_record.get("path"),
                                "suite": suite,
                                "expected_pass_total": len(expected_ids),
                                "pass_total": pass_total,
                                "fail_total": fail_total,
                                "case_count": len(cases),
                            },
                        )
                    )

                case_records = [
                    record
                    for record in artifact_records
                    if isinstance(record, dict)
                    and record.get("suite") == suite
                    and record.get("run_id") == run_id
                    and normalize_os_label(str(record.get("platform", ""))) == platform
                    and _case_artifact_id(record.get("path")) is not None
                ]
                case_records_by_id = {
                    _case_artifact_id(record.get("path")): record
                    for record in case_records
                    if _case_artifact_id(record.get("path")) is not None
                }
                case_record_ids = set(case_records_by_id)
                if case_record_ids != expected_ids:
                    blockers.append(
                        make_blocker(
                            "e2e-case-artifacts-incomplete",
                            "hashed E2E case artifacts must enumerate every expected case exactly",
                            {
                                "suite": suite,
                                "platform": platform,
                                "run_id": run_id,
                                "missing_case_ids": sorted(expected_ids - case_record_ids),
                                "unexpected_case_ids": sorted(case_record_ids - expected_ids),
                            },
                        )
                    )

                run_failed = False
                for case_id in expected_case_ids:
                    record = case_records_by_id.get(case_id)
                    case_path = safe_bundle_path(bundle_dir, record.get("path")) if record else None
                    case_json = _read_json_object(case_path) if case_path else None
                    summary_case = summary_cases.get(case_id)
                    if (
                        case_json is None
                        or not isinstance(case_json.get("id"), str)
                        or case_json.get("id") != case_id
                        or not isinstance(case_json.get("pass"), bool)
                        or summary_case is None
                        or case_json != summary_case
                    ):
                        blockers.append(
                            make_blocker(
                                "e2e-case-artifact-invalid",
                                "each expected E2E case artifact must be readable and agree with its summary case",
                                {
                                    "suite": suite,
                                    "platform": platform,
                                    "run_id": run_id,
                                    "case_id": case_id,
                                    "path": record.get("path") if record else None,
                                },
                            )
                        )
                        run_failed = True
                        continue
                    if case_json["pass"]:
                        coverage[platform].add(case_id)
                    else:
                        run_failed = True

                if run_failed or fail_total != 0:
                    failed_entries.append(
                        {"suite": suite, "os": platform, "run_id": run_id, "fail_total": fail_total}
                    )

                for field in ("commit_sha", "test_ids", "pass_total", "fail_total"):
                    if entry.get(field) != metadata.get(field):
                        blockers.append(
                            make_blocker(
                                "e2e-manifest-summary-mismatch",
                                "manifest run metadata must match the generated E2E summary metadata",
                                {"suite": suite, "platform": platform, "run_id": run_id, "field": field},
                            )
                        )

    return blockers, coverage, failed_entries
