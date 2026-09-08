#!/usr/bin/env python3
"""Reject release verification evidence that does not prove a successful release."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any

from assemble_release_bundle import evaluate_release_gate


REQUIRED_JOBS = (
    "validate-ref",
    "quality-gate",
    "locked-tests",
    "session-runtime-integration",
    "msrv",
    "e2e",
    "build",
    "native-release",
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def read_json(path: Path) -> Any:
    try:
        with path.open("r", encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"error: invalid verification JSON {path}: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--verification", required=True, type=Path)
    parser.add_argument("--evidence-manifest", required=True, type=Path)
    parser.add_argument("--gate-decision", required=True, type=Path)
    parser.add_argument("--workflow-results", required=True, type=Path)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--commit-sha", required=True)
    args = parser.parse_args()

    verification = read_json(args.verification)
    manifest = read_json(args.evidence_manifest)
    gate = read_json(args.gate_decision)
    workflow = read_json(args.workflow_results)
    jobs = workflow.get("jobs") if isinstance(workflow, dict) else None
    if not isinstance(jobs, dict):
        raise SystemExit("error: workflow evidence has no jobs result map")
    if any(jobs.get(job) != "success" for job in REQUIRED_JOBS):
        raise SystemExit("error: npm publication rejected because a required workflow job failed")
    if not isinstance(gate, dict) or gate.get("passed") is not True:
        raise SystemExit("error: npm publication rejected because the release evidence gate failed")
    if not isinstance(verification, dict) or verification.get("status") != "passed":
        raise SystemExit("error: npm publication rejected because verification status is not passed")
    evidence = verification.get("evidence")
    if not isinstance(manifest, dict) or not isinstance(evidence, dict):
        raise SystemExit("error: npm publication rejected because evaluated evidence bundle is missing")
    manifest_sha256 = sha256_file(args.evidence_manifest)
    if evidence.get("manifest_sha256") != manifest_sha256 or gate.get("manifest_sha256") != manifest_sha256:
        raise SystemExit("error: npm publication rejected because evaluated evidence manifest hash changed")
    if evidence.get("bundle_id") != manifest.get("bundle_id") or gate.get("bundle_id") != manifest.get("bundle_id"):
        raise SystemExit("error: npm publication rejected because evidence does not refer to the evaluated bundle")
    bundle_evaluation = evaluate_release_gate(args.evidence_manifest)
    if bundle_evaluation.get("passed") is not True:
        raise SystemExit("error: npm publication rejected because evaluated bundle contents changed")
    inputs = verification.get("inputs")
    if not isinstance(inputs, dict) or inputs != {
        "tag": args.tag,
        "version": args.version,
        "commit_sha": args.commit_sha,
    }:
        raise SystemExit("error: npm publication rejected because verification inputs do not match release ref")
    checks = verification.get("checks")
    if not isinstance(checks, list) or not checks or any(
        not isinstance(check, dict) or check.get("status") != "passed" for check in checks
    ):
        raise SystemExit("error: npm publication rejected because verification contains a failed check")
    print("release verification evidence accepted for npm publication")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
