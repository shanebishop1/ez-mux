"""Discover release inputs and assemble their evidence into a bundle."""

from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Any

from release_evidence import (
    DEFAULT_SUITE_ROOT,
    REQUIRED_NATIVE_PLATFORMS,
    REQUIRED_OS,
    REQUIRED_RELEASE_ASSETS,
    SCHEMA_VERSION,
    SUITE_ORDER,
    load_json,
    normalize_os_label,
    parse_run_metadata,
    rel_to_repo,
    sha256_file,
)


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


def discover_suite_inputs(repo_root: Path, args: Any) -> dict[str, list[Path]]:
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
