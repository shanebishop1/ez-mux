#!/usr/bin/env python3
"""Package one release executable without interpolating release input into shell."""

from __future__ import annotations

import argparse
import re
import tarfile
from pathlib import Path


TAG_PATTERN = re.compile(
    r"^v(?P<version>(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)$"
)
ASSET_PATTERN = re.compile(r"^[a-z0-9]+-[a-z0-9]+$")


def package_archive(binary: Path, tag: str, version: str, asset: str, output_dir: Path) -> Path:
    match = TAG_PATTERN.fullmatch(tag)
    if match is None or match.group("version") != version:
        raise SystemExit("error: tag and version must be a matching semantic release version")
    if not ASSET_PATTERN.fullmatch(asset):
        raise SystemExit("error: asset must contain only lowercase platform-architecture characters")
    if not binary.is_file() or not binary.stat().st_mode & 0o111:
        raise SystemExit(f"error: release binary is missing or not executable: {binary}")

    output_dir.mkdir(parents=True, exist_ok=True)
    archive = output_dir / f"ezm-{tag}-{asset}.tar.gz"
    with tarfile.open(archive, "w:gz") as bundle:
        bundle.add(binary, arcname="ezm", recursive=False)
    print(archive)
    return archive


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--asset", required=True)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    package_archive(args.binary.resolve(), args.tag, args.version, args.asset, args.output_dir.resolve())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
