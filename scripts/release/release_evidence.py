"""Shared release-evidence definitions and filesystem/JSON helpers."""

from __future__ import annotations

import hashlib
import json
import os
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


def generated_bundle_id() -> str:
    return f"bundle-{time.time_ns():x}-{os.getpid():x}"


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


def safe_bundle_path(bundle_dir: Path, path_raw: object) -> Path | None:
    if not isinstance(path_raw, str):
        return None
    path = PurePosixPath(path_raw)
    if path.is_absolute() or ".." in path.parts or path_raw != path.as_posix():
        return None
    return bundle_dir.joinpath(*path.parts)
