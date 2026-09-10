#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import tempfile
import time
from pathlib import Path, PurePosixPath
from typing import Any


SCHEMA_VERSION = "ezm-release-evidence-manifest/v1"
GATE_SCHEMA_VERSION = "ezm-release-gate-decision/v1"
DEFAULT_SUITE_ROOT = Path("target") / "e2e-evidence"
DEFAULT_OUTPUT_ROOT = Path("target") / "release-evidence"

SUITE_ORDER = ("foundation", "core-session-orchestration", "cross-platform-smoke", "install-validation")
REQUIRED_RELEASE_SUITES = ("foundation", "core-session-orchestration", "cross-platform-smoke", "install-validation")
REQUIRED_OS = ("linux", "macos")
FOUNDATION_CASE_IDS = ("E2E-00", "E2E-15", "E2E-17", "E2E-18", "E2E-19", "E2E-20")
CORE_SESSION_CASE_IDS = (
    "E2E-01",
    "E2E-02",
    "E2E-03",
    "E2E-04",
    "E2E-05",
    "E2E-06",
    "E2E-07",
    "E2E-08",
    "E2E-09",
    "E2E-10",
    "E2E-11",
    "E2E-12",
    "E2E-13",
    "E2E-16",
    "E2E-19",
    "E2E-20",
    "E2E-21",
)
SMOKE_CASE_IDS = ("E2E-01", "E2E-04", "E2E-06", "E2E-11")
EXPECTED_CASE_IDS_BY_SUITE = {
    "foundation": FOUNDATION_CASE_IDS,
    "core-session-orchestration": CORE_SESSION_CASE_IDS,
    "cross-platform-smoke": SMOKE_CASE_IDS,
}
FULL_REGRESSION_IDS = tuple(
    sorted(
        {case_id for case_ids in EXPECTED_CASE_IDS_BY_SUITE.values() for case_id in case_ids},
        key=lambda case_id: int(case_id.removeprefix("E2E-")),
    )
)
REQUIRED_WORKFLOW_JOBS = (
    "validate-ref",
    "quality-gate",
    "locked-tests",
    "session-runtime-integration",
    "msrv",
    "e2e",
    "build",
    "native-release",
)
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
REQUIRED_NATIVE_PLATFORMS = ("linux", "macos")
REQUIRED_RELEASE_ASSETS = ("linux-x64", "linux-arm64", "macos-x64", "macos-arm64")


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


def resolve_file_inputs(supplied: str | None, description: str) -> list[Path]:
    if not supplied:
        return []

    resolved: list[Path] = []
    seen: set[Path] = set()
    for token in parse_supplied_runs(supplied):
        candidate = Path(token)
        if candidate.is_dir():
            pattern = "*.json" if description == "native verification records" else "*.tar.gz"
            candidates = sorted(item for item in candidate.glob(pattern) if item.is_file())
            if not candidates:
                raise SystemExit(f"error: no {description} found under {candidate}")
        elif candidate.is_file():
            candidates = [candidate]
        else:
            raise SystemExit(f"error: {description} path does not exist: {candidate}")
        for item in candidates:
            path = item.resolve()
            if path not in seen:
                seen.add(path)
                resolved.append(path)
    if not resolved:
        raise SystemExit(f"error: supplied {description} selection is empty")
    return resolved


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


def native_record_platform(path: Path) -> str:
    record = load_json(path)
    if not isinstance(record, dict):
        raise SystemExit(f"error: native verification record is not a JSON object: {path}")
    platform = record.get("platform")
    if not isinstance(platform, str):
        raise SystemExit(f"error: native verification record has no platform: {path}")
    platform = normalize_os_label(platform)
    if platform not in REQUIRED_NATIVE_PLATFORMS:
        raise SystemExit(f"error: unsupported native verification platform in {path}: {platform}")
    if record.get("status") != "passed":
        raise SystemExit(f"error: native verification record is not passed: {path}")
    return platform


def release_asset_name(path: Path) -> str:
    name = path.name
    if not name.endswith(".tar.gz"):
        raise SystemExit(f"error: release archive has unexpected name: {path}")
    for asset in REQUIRED_RELEASE_ASSETS:
        if name.endswith(f"-{asset}.tar.gz"):
            return asset
    raise SystemExit(f"error: release archive does not identify a supported asset: {path}")


def assemble_bundle(
    repo_root: Path,
    output_root: Path,
    bundle_id: str,
    suite_inputs: dict[str, list[Path]],
    dry_run: bool,
    native_records: list[Path] | None = None,
    release_archives: list[Path] | None = None,
    release_tag: str | None = None,
) -> tuple[Path, dict[str, Any]]:
    native_records = native_records or []
    release_archives = release_archives or []
    native_records = sorted(native_records, key=lambda path: (native_record_platform(path), path.name))
    release_archives = sorted(release_archives, key=lambda path: (release_asset_name(path), path.name))
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
        "native_records": [],
        "release_archives": [],
    }

    for suite, descriptors in suite_runs.items():
        if len(descriptors) == 1:
            run_inputs["suites"][suite]["run_dir"] = descriptors[0]["run_dir_rel"]
            run_inputs["suites"][suite]["run_id"] = descriptors[0]["run_id"]

    for record_path in native_records:
        run_inputs["native_records"].append(
            {
                "source_path": rel_to_repo(repo_root, record_path),
                "platform": native_record_platform(record_path),
            }
        )
    for archive_path in release_archives:
        if release_tag and not archive_path.name.startswith(f"ezm-{release_tag}-"):
            raise SystemExit(
                f"error: release archive {archive_path.name} does not match validated tag {release_tag}"
            )
        run_inputs["release_archives"].append(
            {
                "source_path": rel_to_repo(repo_root, archive_path),
                "asset": release_asset_name(archive_path),
            }
        )
    if release_tag:
        run_inputs["release_tag"] = release_tag

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
        "install_validation_summaries": [],
        "native_verification": [],
        "release_assets": [],
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
                    if suite == "install-validation" and rel_path.as_posix() == "summary.json":
                        evidence_categories["install_validation_summaries"].append(record["path"])
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

    native_manifest_entries: list[dict[str, Any]] = []
    native_platforms: set[str] = set()
    for record_path in native_records:
        platform = native_record_platform(record_path)
        if platform in native_platforms:
            raise SystemExit(f"error: more than one native verification record for {platform}")
        native_platforms.add(platform)
        bundle_rel = Path("artifacts") / "native-verification" / f"{platform}.json"
        target = bundle_dir / bundle_rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(record_path, target)
        checksum = sha256_file(target)
        record = {
            "suite": "native-release",
            "run_id": f"native-{platform}",
            "platform": platform,
            "category": "native-verification",
            "path": bundle_rel.as_posix(),
            "sha256": checksum,
            "size_bytes": target.stat().st_size,
            "source_path": rel_to_repo(repo_root, record_path),
        }
        artifact_records.append(record)
        evidence_categories["native_verification"].append(record["path"])
        native_manifest_entries.append(
            {
                "platform": platform,
                "path": record["path"],
                "sha256": checksum,
                "source_path": record["source_path"],
            }
        )

    release_asset_entries: list[dict[str, Any]] = []
    release_assets_seen: set[str] = set()
    for archive_path in release_archives:
        if release_tag and not archive_path.name.startswith(f"ezm-{release_tag}-"):
            raise SystemExit(
                f"error: release archive {archive_path.name} does not match validated tag {release_tag}"
            )
        asset = release_asset_name(archive_path)
        if asset in release_assets_seen:
            raise SystemExit(f"error: more than one release archive for asset {asset}")
        release_assets_seen.add(asset)
        bundle_rel = Path("artifacts") / "release-assets" / archive_path.name
        target = bundle_dir / bundle_rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(archive_path, target)
        checksum = sha256_file(target)
        record = {
            "suite": "release-assets",
            "run_id": f"release-{asset}",
            "platform": "macos" if asset.startswith("macos-") else "linux",
            "asset": asset,
            "category": "release-assets",
            "path": bundle_rel.as_posix(),
            "sha256": checksum,
            "size_bytes": target.stat().st_size,
            "source_path": rel_to_repo(repo_root, archive_path),
        }
        artifact_records.append(record)
        evidence_categories["release_assets"].append(record["path"])
        release_asset_entries.append(
            {
                "asset": asset,
                "path": record["path"],
                "sha256": checksum,
                "source_path": record["source_path"],
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
        "native_verification": native_manifest_entries,
        "release_assets": release_asset_entries,
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


def validate_manifest_artifacts(
    bundle_dir: Path, artifact_records: list[Any]
) -> list[dict[str, Any]]:
    blockers: list[dict[str, Any]] = []
    seen_paths: set[str] = set()
    for index, record in enumerate(artifact_records):
        if not isinstance(record, dict):
            blockers.append(
                make_blocker(
                    "manifest-artifact-record-invalid",
                    "every manifest artifact record must be an object",
                    {"index": index},
                )
            )
            continue
        path_raw = record.get("path")
        checksum = record.get("sha256")
        size_bytes = record.get("size_bytes")
        if not isinstance(path_raw, str) or not path_raw:
            blockers.append(
                make_blocker(
                    "manifest-artifact-record-invalid",
                    "every artifact record must contain a path",
                    {"index": index},
                )
            )
            continue
        path = PurePosixPath(path_raw)
        if path.is_absolute() or ".." in path.parts or path_raw != path.as_posix():
            blockers.append(
                make_blocker(
                    "manifest-artifact-path-unsafe",
                    "artifact record paths must be safe bundle-relative paths",
                    {"path": path_raw},
                )
            )
            continue
        if path_raw in seen_paths:
            blockers.append(
                make_blocker(
                    "manifest-artifact-duplicate",
                    "artifact record paths must be unique",
                    {"path": path_raw},
                )
            )
            continue
        seen_paths.add(path_raw)
        if not isinstance(checksum, str) or re.fullmatch(r"[0-9a-f]{64}", checksum) is None:
            blockers.append(
                make_blocker(
                    "manifest-artifact-hash-invalid",
                    "artifact records must contain lowercase SHA-256 hashes",
                    {"path": path_raw},
                )
            )
            continue
        if not isinstance(size_bytes, int) or size_bytes < 0:
            blockers.append(
                make_blocker(
                    "manifest-artifact-size-invalid",
                    "artifact records must contain a non-negative byte size",
                    {"path": path_raw},
                )
            )
            continue

        artifact_path = bundle_dir.joinpath(*path.parts)
        if not artifact_path.is_file():
            blockers.append(
                make_blocker(
                    "manifest-artifact-file-missing",
                    "every manifest artifact must exist in the evaluated bundle",
                    {"path": path_raw},
                )
            )
            continue
        actual_size = artifact_path.stat().st_size
        actual_checksum = sha256_file(artifact_path)
        if actual_size != size_bytes or actual_checksum != checksum:
            blockers.append(
                make_blocker(
                    "manifest-artifact-hash-mismatch",
                    "evaluated bundle artifact content does not match its manifest record",
                    {
                        "path": path_raw,
                        "expected_sha256": checksum,
                        "actual_sha256": actual_checksum,
                        "expected_size_bytes": size_bytes,
                        "actual_size_bytes": actual_size,
                    },
                )
            )
    return blockers


def manifest_records_by_category(
    artifact_records: list[Any], category: str
) -> list[dict[str, Any]]:
    return [
        record
        for record in artifact_records
        if isinstance(record, dict) and record.get("category") == category
    ]


def safe_bundle_path(bundle_dir: Path, path_raw: object) -> Path | None:
    if not isinstance(path_raw, str):
        return None
    path = PurePosixPath(path_raw)
    if path.is_absolute() or ".." in path.parts or path_raw != path.as_posix():
        return None
    return bundle_dir.joinpath(*path.parts)


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
