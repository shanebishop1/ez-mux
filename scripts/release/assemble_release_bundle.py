#!/usr/bin/env python3
"""Assemble and verify release evidence bundles."""

from __future__ import annotations

import argparse
import tempfile
from pathlib import Path
from typing import Any

from release_collection import assemble_bundle, discover_suite_inputs, resolve_file_inputs
from release_evidence import (
    DEFAULT_OUTPUT_ROOT,
    SUITE_ORDER,
    generated_bundle_id,
    load_json,
)
from release_gate import evaluate_release_gate, run_release_gate_evaluation


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
    parser.add_argument(
        "--native-records",
        help="Directory or comma-separated list of native verification JSON records",
    )
    parser.add_argument(
        "--release-archives",
        help="Directory or comma-separated list of exact publishable release archives",
    )
    parser.add_argument("--release-tag", help="Validated release tag used to identify release archives")
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
    parser.add_argument(
        "--workflow-results",
        help="JSON containing actual needs.*.result values and validated release inputs",
    )
    return parser.parse_args()


def run_reproducibility_check(repo_root: Path, manifest_path: Path) -> int:
    if not manifest_path.is_file():
        raise SystemExit(f"error: manifest not found: {manifest_path}")

    bundle_dir = manifest_path.parent
    original_manifest = load_json(manifest_path)

    inputs_path = bundle_dir / "run-inputs.json"
    if not inputs_path.is_file():
        raise SystemExit(f"error: run inputs not found beside manifest: {inputs_path}")
    recorded_inputs = load_json(inputs_path)

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

    native_records: list[Path] = []
    recorded_native = recorded_inputs.get("native_records")
    if not isinstance(recorded_native, list) or not recorded_native:
        raise SystemExit("error: recorded inputs missing native verification records")
    for entry in recorded_native:
        if not isinstance(entry, dict) or not isinstance(entry.get("source_path"), str):
            raise SystemExit("error: recorded native verification input is invalid")
        raw_path = Path(entry["source_path"])
        path = raw_path.resolve() if raw_path.is_absolute() else (repo_root / raw_path).resolve()
        if not path.is_file():
            raise SystemExit(f"error: recorded native verification record missing: {path}")
        native_records.append(path)

    release_archives: list[Path] = []
    recorded_archives = recorded_inputs.get("release_archives")
    if not isinstance(recorded_archives, list) or not recorded_archives:
        raise SystemExit("error: recorded inputs missing release archives")
    for entry in recorded_archives:
        if not isinstance(entry, dict) or not isinstance(entry.get("source_path"), str):
            raise SystemExit("error: recorded release archive input is invalid")
        raw_path = Path(entry["source_path"])
        path = raw_path.resolve() if raw_path.is_absolute() else (repo_root / raw_path).resolve()
        if not path.is_file():
            raise SystemExit(f"error: recorded release archive missing: {path}")
        release_archives.append(path)

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
            native_records=native_records,
            release_archives=release_archives,
            release_tag=recorded_inputs.get("release_tag"),
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
            Path(args.workflow_results).resolve() if args.workflow_results else None,
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
        native_records=resolve_file_inputs(args.native_records, "native verification records"),
        release_archives=resolve_file_inputs(args.release_archives, "release archives"),
        release_tag=args.release_tag,
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
