#!/usr/bin/env python3
"""Validate the release tag, package version, and checked-out git ref.

All release inputs are read as data.  This script deliberately does not invoke a
shell and only emits values after every consistency check has passed.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
from pathlib import Path
from typing import Any


TAG_PATTERN = re.compile(
    r"^v(?P<version>(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)$"
)


def run_git(repo_root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo_root), *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "unknown git error"
        raise SystemExit(f"error: git {' '.join(args)} failed: {detail}")
    return result.stdout.strip()


def cargo_package_version(repo_root: Path) -> str:
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or "cargo metadata failed"
        raise SystemExit(f"error: {detail}")

    try:
        metadata: dict[str, Any] = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise SystemExit(f"error: cargo metadata returned invalid JSON: {error}") from error

    packages = metadata.get("packages")
    if not isinstance(packages, list):
        raise SystemExit("error: cargo metadata did not return packages")
    matches = [
        package
        for package in packages
        if isinstance(package, dict) and package.get("name") == "ez-mux"
    ]
    if len(matches) != 1 or not isinstance(matches[0].get("version"), str):
        raise SystemExit("error: cargo metadata must contain exactly one ez-mux package")
    return matches[0]["version"]


def validate_release(repo_root: Path, tag: str) -> dict[str, str]:
    match = TAG_PATTERN.fullmatch(tag)
    if match is None:
        raise SystemExit(
            "error: release tag must be an exact semantic version tag such as v1.2.3"
        )

    version = match.group("version")
    package_version = cargo_package_version(repo_root)
    if package_version != version:
        raise SystemExit(
            f"error: release tag {tag} does not match ez-mux package version {package_version}"
        )

    head = run_git(repo_root, "rev-parse", "--verify", "HEAD")
    tag_commit = run_git(repo_root, "rev-parse", "--verify", f"refs/tags/{tag}^{{commit}}")
    if head != tag_commit:
        raise SystemExit(
            f"error: checked-out commit {head} is not the commit targeted by {tag} ({tag_commit})"
        )

    return {"tag": tag, "version": version, "commit_sha": head}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", default=".")
    parser.add_argument("--tag", default=os.environ.get("RELEASE_TAG"))
    parser.add_argument("--json-output", help="Optional path for the validated metadata JSON")
    return parser.parse_args()


def write_github_output(metadata: dict[str, str]) -> None:
    output_path = os.environ.get("GITHUB_OUTPUT")
    if not output_path:
        return
    with Path(output_path).open("a", encoding="utf-8") as output:
        for key in ("tag", "version", "commit_sha"):
            output.write(f"{key}={metadata[key]}\n")


def main() -> int:
    args = parse_args()
    if not args.tag:
        raise SystemExit("error: release tag is required via --tag or RELEASE_TAG")
    metadata = validate_release(Path(args.repo_root).resolve(), args.tag)
    if args.json_output:
        Path(args.json_output).resolve().write_text(
            json.dumps(metadata, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    write_github_output(metadata)
    print(json.dumps(metadata, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
