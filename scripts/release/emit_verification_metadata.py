#!/usr/bin/env python3
"""Emit release verification metadata from the workflow's actual gate inputs."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import Any


CHECKS = (
    ("validate-ref", "validated release tag, package version, and commit"),
    ("quality-gate", "format, strict Clippy, and runtime structure audit"),
    ("locked-tests", "complete cargo test --locked surface"),
    ("session-runtime-integration", "session_runtime_integration locked integration test"),
    ("msrv", "locked MSRV compatibility checks"),
    ("e2e", "independent Linux and macOS foundation/core/smoke/layout E2E matrix"),
    ("build", "publishable release archive builds"),
    ("native-release", "native verification of publishable archives"),
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> Any:
    try:
        with path.open("r", encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, json.JSONDecodeError) as error:
        raise SystemExit(f"error: could not read JSON {path}: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--evidence-manifest", required=True, type=Path)
    parser.add_argument("--gate-decision", required=True, type=Path)
    parser.add_argument("--workflow-results", required=True, type=Path)
    args = parser.parse_args()

    tag = os.environ["RELEASE_TAG"]
    version = os.environ["RELEASE_VERSION"]
    commit_sha = os.environ["RELEASE_SHA"]
    run_url = os.environ["RELEASE_RUN_URL"]
    workflow_results = load_json(args.workflow_results)
    workflow_jobs = workflow_results.get("jobs") if isinstance(workflow_results, dict) else None
    workflow_inputs = workflow_results.get("inputs") if isinstance(workflow_results, dict) else None
    gate_decision = load_json(args.gate_decision)
    evidence_manifest = load_json(args.evidence_manifest) if args.evidence_manifest.is_file() else None

    if not isinstance(workflow_jobs, dict) or not isinstance(workflow_inputs, dict):
        raise SystemExit("error: workflow results must contain jobs and inputs objects")

    input_values = {"tag": tag, "version": version, "commit_sha": commit_sha}
    input_match = all(workflow_inputs.get(key) == value for key, value in input_values.items())
    gate_passed = gate_decision.get("passed") is True if isinstance(gate_decision, dict) else False

    checks = []
    for job, description in CHECKS:
        result = workflow_jobs.get(job)
        checks.append(
            {
                "name": description,
                "workflow_job": job,
                "workflow_result": result if isinstance(result, str) else "missing",
                "status": "passed" if result == "success" else "failed",
            }
        )
    checks.append(
        {
            "name": "release evidence gate evaluation",
            "workflow_job": "evidence-gate",
            "workflow_result": "success" if gate_passed else "failed",
            "status": "passed" if gate_passed else "failed",
        }
    )

    all_jobs_passed = all(check["status"] == "passed" for check in checks[:-1])
    evidence_passed = input_match and all_jobs_passed and gate_passed and isinstance(evidence_manifest, dict)
    manifest_sha256 = sha256_file(args.evidence_manifest) if args.evidence_manifest.is_file() else None
    artifacts = [
        f"ezm-{tag}-linux-x64.tar.gz",
        f"ezm-{tag}-linux-arm64.tar.gz",
        f"ezm-{tag}-macos-x64.tar.gz",
        f"ezm-{tag}-macos-arm64.tar.gz",
        f"ezm-{tag}-checksums.txt",
        f"ezm-{tag}-sbom.spdx.json",
        f"ezm-{tag}-sbom-status.txt",
        f"ezm-{tag}-verification.json",
        f"ezm-{tag}-release-evidence.tar.gz",
    ]
    payload = {
        "schema_version": "ezm-release-verification/v3",
        "release_tag": tag,
        "version": version,
        "commit_sha": commit_sha,
        "workflow_run": {"url": run_url},
        "inputs": input_values,
        "checks": checks,
        "release_artifacts": artifacts,
        "evidence": {
            "manifest": str(args.evidence_manifest),
            "manifest_sha256": manifest_sha256,
            "bundle_id": evidence_manifest.get("bundle_id") if isinstance(evidence_manifest, dict) else None,
            "gate_decision": str(args.gate_decision),
            "workflow_results": str(args.workflow_results),
        },
        "status": "passed" if evidence_passed else "failed",
    }
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(output)
    return 0 if evidence_passed else 2


if __name__ == "__main__":
    raise SystemExit(main())
