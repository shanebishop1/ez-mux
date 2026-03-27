#!/usr/bin/env python3

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

TARGET_MAX = 250
WARNING_MIN = 401
HARD_STOP_MIN = 601


@dataclass(frozen=True)
class FileAudit:
    path: str
    line_count: int


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def is_test_file(path: Path) -> bool:
    if "tests" in path.parts:
        return True

    name = path.name
    stem = path.stem
    return (
        name == "tests.rs"
        or stem.endswith("_test")
        or stem.endswith("_tests")
    )


def collect_runtime_files(src_root: Path) -> list[Path]:
    files = [
        candidate
        for candidate in src_root.rglob("*.rs")
        if candidate.is_file() and not is_test_file(candidate.relative_to(src_root))
    ]
    return sorted(files, key=lambda file_path: file_path.as_posix())


def count_lines(path: Path) -> int:
    with path.open("r", encoding="utf-8") as handle:
        return sum(1 for _ in handle)


def classify(line_count: int) -> str:
    if line_count >= HARD_STOP_MIN:
        return "FAIL"
    if line_count >= WARNING_MIN:
        return "WARN"
    if line_count > TARGET_MAX:
        return "INFO"
    return "OK"


def audit_files(src_root: Path, files: list[Path]) -> list[FileAudit]:
    return [
        FileAudit(path=file_path.relative_to(src_root.parent).as_posix(), line_count=count_lines(file_path))
        for file_path in files
    ]


def main() -> int:
    root = repo_root()
    src_root = root / "src"

    if not src_root.is_dir():
        raise SystemExit(f"error: src root not found: {src_root}")

    runtime_files = collect_runtime_files(src_root)
    if not runtime_files:
        raise SystemExit("error: no runtime source files found in src/")

    audits = audit_files(src_root, runtime_files)

    print("Runtime source file size audit (docs/plan.md §8)")
    print("Scope: src/**/*.rs excluding test modules/files")
    print("Thresholds: target <=250, warning >400, hard stop >600")
    print("")
    print(f"{'STATUS':<6} {'LINES':>5}  PATH")
    print(f"{'-' * 6} {'-' * 5}  {'-' * 4}")

    counts = {"OK": 0, "INFO": 0, "WARN": 0, "FAIL": 0}
    for audit in audits:
        status = classify(audit.line_count)
        counts[status] += 1
        print(f"{status:<6} {audit.line_count:>5}  {audit.path}")

    total_files = len(audits)
    print("")
    print(f"Files audited: {total_files}")
    print(
        "Summary: "
        f"OK={counts['OK']}, "
        f"INFO={counts['INFO']} (>250), "
        f"WARN={counts['WARN']} (>400), "
        f"FAIL={counts['FAIL']} (>600)"
    )

    if counts["FAIL"] > 0:
        print("result: FAIL (hard-stop threshold exceeded)")
        return 1

    if counts["WARN"] > 0:
        print("result: PASS with warnings")
    else:
        print("result: PASS")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
