#!/usr/bin/env python3
"""Verify a native release binary and archive, including their version output."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import stat
import subprocess
import tarfile
import tempfile
from pathlib import Path, PurePosixPath
from typing import Any


VERSION_PATTERN = re.compile(
    r"^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)


def run_version(binary: Path, expected_version: str) -> str:
    result = subprocess.run(
        [str(binary), "--version"],
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    output = (result.stdout + result.stderr).strip()
    expected = f"ezm {expected_version}"
    if result.returncode != 0 or expected not in output:
        raise SystemExit(
            f"error: {binary} --version failed validation (exit={result.returncode}, output={output!r})"
        )
    return output


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(64 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_archive_member(member: tarfile.TarInfo) -> None:
    name = PurePosixPath(member.name)
    if name.is_absolute() or ".." in name.parts or member.name != "ezm":
        raise SystemExit(f"error: archive contains unsafe or unexpected member: {member.name!r}")
    if not member.isfile() or not member.mode & stat.S_IXUSR:
        raise SystemExit("error: archive member ezm is not an executable regular file")


def inspect_archive_structure(archive: Path) -> dict[str, Any]:
    with tarfile.open(archive, "r:gz") as bundle:
        members = bundle.getmembers()
        if len(members) != 1:
            raise SystemExit(f"error: archive must contain exactly one member, found {len(members)}")
        member = members[0]
        safe_archive_member(member)
        return {
            "path": str(archive),
            "archive_sha256": sha256_file(archive),
            "member": member.name,
            "executable": bool(member.mode & stat.S_IXUSR),
        }


def extract_archive(archive: Path, destination: Path) -> tuple[Path, dict[str, Any]]:
    structure = inspect_archive_structure(archive)
    with tarfile.open(archive, "r:gz") as bundle:
        member = bundle.getmembers()[0]
        destination.mkdir(parents=True, exist_ok=True)
        extracted = destination / "ezm"
        source = bundle.extractfile(member)
        if source is None:
            raise SystemExit("error: archive executable could not be read")
        extracted.write_bytes(source.read())
        extracted.chmod(member.mode & 0o777)
    return extracted, {**structure, "member_sha256": sha256_file(extracted)}


def verify_archive(
    archive: Path,
    expected_version: str,
    expected_binary_sha256: str | None = None,
    run_binary: bool = True,
) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="ezm-release-archive-") as temp_dir:
        extracted, structure = extract_archive(archive, Path(temp_dir))
        member_sha256 = structure["member_sha256"]
        if expected_binary_sha256 is not None and member_sha256 != expected_binary_sha256:
            raise SystemExit(
                "error: release binary and archive member differ "
                f"(binary={expected_binary_sha256}, archive={member_sha256})"
            )
        result = {**structure}
        if run_binary:
            result["version_output"] = run_version(extracted, expected_version)
        return result


def verify_release(binary: Path, archive: Path, expected_version: str, platform: str) -> dict[str, Any]:
    if VERSION_PATTERN.fullmatch(expected_version) is None:
        raise SystemExit(f"error: invalid expected release version: {expected_version!r}")
    if not binary.is_file() or not binary.stat().st_mode & 0o111:
        raise SystemExit(f"error: native release binary is missing or not executable: {binary}")
    binary_sha256 = sha256_file(binary)
    binary_version = run_version(binary, expected_version)
    archive_result = verify_archive(archive, expected_version, binary_sha256)
    return {
        "schema_version": "ezm-native-release-verification/v1",
        "platform": platform,
        "expected_version": expected_version,
        "binary": {"path": str(binary), "sha256": binary_sha256, "version_output": binary_version},
        "archive": archive_result,
        "status": "passed",
    }


def verify_release_archive(archive: Path, expected_version: str, platform: str) -> dict[str, Any]:
    archive_result = verify_archive(archive, expected_version)
    return {
        "schema_version": "ezm-native-release-verification/v1",
        "platform": platform,
        "expected_version": expected_version,
        "binary": {
            "source": "archive member ezm",
            "sha256": archive_result["member_sha256"],
            "version_output": archive_result["version_output"],
        },
        "archive": archive_result,
        "status": "passed",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--expected-version")
    parser.add_argument("--platform", required=True, choices=("linux", "macos"))
    parser.add_argument("--output", type=Path)
    parser.add_argument(
        "--extract-to",
        type=Path,
        help="Safely extract the single ezm archive member to this directory",
    )
    parser.add_argument(
        "--archive-only",
        action="store_true",
        help="Check archive contents and executable permissions without running the binary",
    )
    args = parser.parse_args()
    if args.expected_version is not None and VERSION_PATTERN.fullmatch(args.expected_version) is None:
        raise SystemExit(f"error: invalid expected release version: {args.expected_version!r}")
    if args.extract_to is not None:
        extracted, result = extract_archive(args.archive.resolve(), args.extract_to.resolve())
        if args.expected_version is not None:
            run_version(extracted, args.expected_version)
        print(json.dumps({**result, "extracted": str(extracted)}, sort_keys=True))
        return 0
    if args.archive_only:
        if args.expected_version is None:
            raise SystemExit("error: --expected-version is required with --archive-only")
        result = {
            "schema_version": "ezm-archive-verification/v1",
            "platform": args.platform,
            "expected_version": args.expected_version,
            "archive": verify_archive(args.archive.resolve(), args.expected_version, run_binary=False),
            "status": "passed",
        }
    else:
        if args.expected_version is None:
            raise SystemExit("error: --expected-version is required unless --extract-to is used")
        if args.output is None:
            raise SystemExit("error: --output is required unless --archive-only is used")
        if args.binary is None:
            result = verify_release_archive(args.archive.resolve(), args.expected_version, args.platform)
        else:
            result = verify_release(
                args.binary.resolve(), args.archive.resolve(), args.expected_version, args.platform
            )
    if args.output is not None:
        args.output.resolve().parent.mkdir(parents=True, exist_ok=True)
        args.output.resolve().write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
