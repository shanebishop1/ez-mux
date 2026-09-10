"""Validate hashed artifact records and manifest category indexes."""

from __future__ import annotations

import re
from pathlib import Path, PurePosixPath
from typing import Any

from release_evidence import make_blocker, sha256_file


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
