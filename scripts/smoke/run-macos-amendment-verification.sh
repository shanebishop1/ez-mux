#!/usr/bin/env sh
set -eu

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"

if ! command -v tmux >/dev/null 2>&1; then
  if [ -x "/opt/homebrew/bin/tmux" ]; then
    PATH="/opt/homebrew/bin:$PATH"
    export PATH
  elif [ -x "/usr/local/bin/tmux" ]; then
    PATH="/usr/local/bin:$PATH"
    export PATH
  fi
fi

dry_run=0
if [ "$#" -gt 0 ]; then
  if [ "$1" = "--dry-run" ]; then
    dry_run=1
    shift
  else
    echo "unknown argument: $1" >&2
    exit 64
  fi
fi

if [ "$#" -gt 0 ]; then
  echo "unexpected extra arguments" >&2
  exit 64
fi

max_cargo_jobs="${EZM_AMENDMENT_MAX_CARGO_JOBS:-2}"
test_threads="${EZM_AMENDMENT_TEST_THREADS:-1}"

case "$max_cargo_jobs" in
  ''|*[!0-9]*)
    echo "EZM_AMENDMENT_MAX_CARGO_JOBS must be a positive integer" >&2
    exit 64
    ;;
esac

case "$test_threads" in
  ''|*[!0-9]*)
    echo "EZM_AMENDMENT_TEST_THREADS must be a positive integer" >&2
    exit 64
    ;;
esac

if [ "$max_cargo_jobs" -lt 1 ] || [ "$test_threads" -lt 1 ]; then
  echo "EZM_AMENDMENT_MAX_CARGO_JOBS and EZM_AMENDMENT_TEST_THREADS must be >= 1" >&2
  exit 64
fi

host_os="$(uname -s)"
if [ "$dry_run" -eq 0 ] && [ "$host_os" != "Darwin" ]; then
  echo "platform mismatch: host '$host_os' does not match macOS amendment target" >&2
  exit 65
fi

if command -v python3 >/dev/null 2>&1; then
  python_bin="python3"
elif command -v python >/dev/null 2>&1; then
  python_bin="python"
else
  echo "python3 (or python) is required for macOS amendment verification" >&2
  exit 69
fi

run_id="run-$(date +%s)-$$"
artifact_root="$repo_root/target/e2e-evidence/focus5-macos-amendment/$run_id"
log_dir="$artifact_root/logs"
mkdir -p "$log_dir"

tmp_root="$(mktemp -d "/tmp/ezm-amendment-macos-XXXXXX")"
namespace="focus5-amendment-macos-$(date +%s)-$$"

cleanup() {
  if command -v tmux >/dev/null 2>&1; then
    tmux -L "$namespace" kill-server >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp_root"
}
trap cleanup EXIT INT TERM

resource_snapshot() {
  phase="$1"
  checkpoint="$artifact_root/resource-$phase.txt"
  {
    printf '%s\n' "resource checkpoint ($phase)"
    if command -v ps >/dev/null 2>&1; then
      ps -Ao pid,ppid,pcpu,rss,comm,args | awk 'NR == 1 || $5 ~ /cargo|rustc|tmux|opencode|ezm/'
    else
      printf '%s\n' "ps unavailable"
    fi
  } > "$checkpoint"
}

find_run_dir_after_marker() {
  suite_dir="$1"
  marker_path="$2"
  "$python_bin" - "$suite_dir" "$marker_path" <<'PY'
import pathlib
import sys

suite_dir = pathlib.Path(sys.argv[1])
marker = pathlib.Path(sys.argv[2])
if not suite_dir.exists():
    print("")
    raise SystemExit(0)

marker_mtime = marker.stat().st_mtime
candidates = []
for path in suite_dir.glob("run-*"):
    if not path.is_dir():
        continue
    try:
        mtime = path.stat().st_mtime
    except OSError:
        continue
    if mtime >= marker_mtime:
        candidates.append((mtime, str(path)))

if not candidates:
    print("")
else:
    candidates.sort(reverse=True)
    print(candidates[0][1])
PY
}

if [ "$dry_run" -eq 1 ]; then
  echo "EZM_AMENDMENT_ARTIFACT_ROOT=$artifact_root"
  echo "EZM_AMENDMENT_MAX_CARGO_JOBS=$max_cargo_jobs"
  echo "EZM_AMENDMENT_TEST_THREADS=$test_threads"
  echo "EZM_AMENDMENT_TMPDIR=$tmp_root"
  echo "cargo test --test core_session_e2e -- --nocapture"
  echo "cargo test --test foundation_e2e -- --nocapture"
  echo "sh scripts/smoke/run-macos-smoke.sh"
  exit 0
fi

run_command() {
  command_name="$1"
  suite_key="$2"
  command_text="$3"

  marker="$tmp_root/$command_name.marker"
  : > "$marker"

  resource_snapshot "before-$command_name"

  status=0
  (
    cd "$repo_root"
    EZM_AMENDMENT_NAMESPACE="$namespace" \
    TMPDIR="$tmp_root" \
    CARGO_BUILD_JOBS="$max_cargo_jobs" \
    RUST_TEST_THREADS="$test_threads" \
    sh -c "$command_text"
  ) > "$log_dir/$command_name.log" 2>&1 || status=$?

  resource_snapshot "after-$command_name"

  run_dir="$(find_run_dir_after_marker "$repo_root/target/e2e-evidence/$suite_key" "$marker")"
  printf '%s\n' "$status" > "$artifact_root/$command_name.exit_code"
  printf '%s\n' "$run_dir" > "$artifact_root/$command_name.run_dir"
}

run_command "core_session" "core-session-orchestration" "cargo test --test core_session_e2e -- --nocapture"
run_command "foundation" "foundation" "cargo test --test foundation_e2e -- --nocapture"
run_command "smoke" "cross-platform-smoke" "sh scripts/smoke/run-macos-smoke.sh"

"$python_bin" - "$artifact_root" <<'PY'
import json
import pathlib
import sys

artifact_root = pathlib.Path(sys.argv[1])
log_dir = artifact_root / "logs"

def read_text(path: pathlib.Path) -> str:
    if not path.exists():
        return ""
    return path.read_text(encoding="utf-8").strip()

def read_json(path: pathlib.Path):
    if not path.exists() or not path.is_file():
        return None
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return None

def case_pass(path: pathlib.Path):
    data = read_json(path)
    if not isinstance(data, dict):
        return None
    value = data.get("pass")
    if isinstance(value, bool):
        return value
    return None

def smoke_profile_pass(summary_path: pathlib.Path):
    data = read_json(summary_path)
    if not isinstance(data, dict):
        return None
    metadata = data.get("metadata")
    if not isinstance(metadata, dict):
        return None
    fail_total = metadata.get("fail_total")
    pass_total = metadata.get("pass_total")
    if not isinstance(fail_total, int) or not isinstance(pass_total, int):
        return None
    return fail_total == 0 and pass_total >= 1

core_run_dir_text = read_text(artifact_root / "core_session.run_dir")
foundation_run_dir_text = read_text(artifact_root / "foundation.run_dir")
smoke_run_dir_text = read_text(artifact_root / "smoke.run_dir")

core_run_dir = pathlib.Path(core_run_dir_text) if core_run_dir_text else None
foundation_run_dir = pathlib.Path(foundation_run_dir_text) if foundation_run_dir_text else None
smoke_run_dir = pathlib.Path(smoke_run_dir_text) if smoke_run_dir_text else None

core_exit = int(read_text(artifact_root / "core_session.exit_code") or "1")
foundation_exit = int(read_text(artifact_root / "foundation.exit_code") or "1")
smoke_exit = int(read_text(artifact_root / "smoke.exit_code") or "1")

core_summary = core_run_dir / "summary.json" if core_run_dir else None
foundation_summary = foundation_run_dir / "summary.json" if foundation_run_dir else None
smoke_summary = smoke_run_dir / "summary.json" if smoke_run_dir else None

checklist = [
    (
        "E2E-01",
        (core_run_dir / "cases" / "E2E-01.json") if core_run_dir else None,
        case_pass(core_run_dir / "cases" / "E2E-01.json") if core_run_dir else None,
    ),
    (
        "E2E-02",
        (core_run_dir / "cases" / "E2E-02.json") if core_run_dir else None,
        case_pass(core_run_dir / "cases" / "E2E-02.json") if core_run_dir else None,
    ),
    (
        "E2E-03",
        (core_run_dir / "cases" / "E2E-03.json") if core_run_dir else None,
        case_pass(core_run_dir / "cases" / "E2E-03.json") if core_run_dir else None,
    ),
    (
        "E2E-04",
        (core_run_dir / "cases" / "E2E-04.json") if core_run_dir else None,
        case_pass(core_run_dir / "cases" / "E2E-04.json") if core_run_dir else None,
    ),
    (
        "E2E-06",
        (core_run_dir / "cases" / "E2E-06.json") if core_run_dir else None,
        case_pass(core_run_dir / "cases" / "E2E-06.json") if core_run_dir else None,
    ),
    ("E2E-14", smoke_summary, smoke_profile_pass(smoke_summary) if smoke_summary else None),
    (
        "E2E-17",
        (foundation_run_dir / "cases" / "E2E-17.json") if foundation_run_dir else None,
        case_pass(foundation_run_dir / "cases" / "E2E-17.json") if foundation_run_dir else None,
    ),
    (
        "E2E-18",
        (foundation_run_dir / "cases" / "E2E-18.json") if foundation_run_dir else None,
        case_pass(foundation_run_dir / "cases" / "E2E-18.json") if foundation_run_dir else None,
    ),
]

checklist_rows = []
pass_total = 0
fail_total = 0
for e2e_id, artifact, passed in checklist:
    artifact_path = str(artifact) if artifact else ""
    status = "pass" if passed is True else "fail"
    if passed is True:
        pass_total += 1
    else:
        fail_total += 1
    checklist_rows.append(
        {
            "id": e2e_id,
            "status": status,
            "pass": passed,
            "artifact_path": artifact_path,
        }
    )

commands = [
    {
        "name": "core_session",
        "command": "cargo test --test core_session_e2e -- --nocapture",
        "exit_code": core_exit,
        "log_path": str(log_dir / "core_session.log"),
    },
    {
        "name": "foundation",
        "command": "cargo test --test foundation_e2e -- --nocapture",
        "exit_code": foundation_exit,
        "log_path": str(log_dir / "foundation.log"),
    },
    {
        "name": "smoke",
        "command": "sh scripts/smoke/run-macos-smoke.sh",
        "exit_code": smoke_exit,
        "log_path": str(log_dir / "smoke.log"),
    },
]

command_manifest = {
    "suite": "focus5-macos-amendment",
    "platform": "macos",
    "commands": commands,
    "resource_checkpoints": [
        str(artifact_root / "resource-before-core_session.txt"),
        str(artifact_root / "resource-after-core_session.txt"),
        str(artifact_root / "resource-before-foundation.txt"),
        str(artifact_root / "resource-after-foundation.txt"),
        str(artifact_root / "resource-before-smoke.txt"),
        str(artifact_root / "resource-after-smoke.txt"),
    ],
        "artifact_roots": {
        "core_session_run_dir": str(core_run_dir) if core_run_dir else "",
        "foundation_run_dir": str(foundation_run_dir) if foundation_run_dir else "",
        "smoke_run_dir": str(smoke_run_dir) if smoke_run_dir else "",
    },
}

checklist_json = {
    "suite": "focus5-macos-amendment",
    "platform": "macos",
    "impacted_ids": checklist_rows,
    "pass_total": pass_total,
    "fail_total": fail_total,
}

overall_pass = (
    pass_total == 8
    and fail_total == 0
    and foundation_exit == 0
    and smoke_exit == 0
)

(artifact_root / "command-manifest.json").write_text(
    json.dumps(command_manifest, indent=2) + "\n", encoding="utf-8"
)
(artifact_root / "checklist.json").write_text(
    json.dumps(checklist_json, indent=2) + "\n", encoding="utf-8"
)

env_lines = [
    f"EZM_AMENDMENT_ARTIFACT_ROOT={artifact_root}",
    f"EZM_AMENDMENT_COMMAND_MANIFEST_JSON={artifact_root / 'command-manifest.json'}",
    f"EZM_AMENDMENT_CHECKLIST_JSON={artifact_root / 'checklist.json'}",
    f"EZM_AMENDMENT_CORE_SUMMARY_JSON={str(core_summary) if core_summary else ''}",
    f"EZM_AMENDMENT_FOUNDATION_SUMMARY_JSON={str(foundation_summary) if foundation_summary else ''}",
    f"EZM_AMENDMENT_SMOKE_SUMMARY_JSON={str(smoke_summary) if smoke_summary else ''}",
    f"EZM_AMENDMENT_CLOSURE_HANDOFF_MD={artifact_root / 'closure-handoff.md'}",
]
(artifact_root / "paths.env").write_text("\n".join(env_lines) + "\n", encoding="utf-8")

lines = []
lines.append("# Focus5 macOS amendment closure handoff")
lines.append("")
lines.append("## Run metadata")
lines.append(f"- Platform: macOS")
lines.append(f"- Verification pack root: `{artifact_root}`")
lines.append(f"- Overall status: {'PASS' if overall_pass else 'FAIL'}")
lines.append("")
lines.append("## Commands")
for command in commands:
    lines.append(
        f"- `{command['command']}` -> exit `{command['exit_code']}`, log `{command['log_path']}`"
    )
lines.append("")
lines.append("## Machine-readable artifact paths")
lines.append(f"- Core summary: `{core_summary if core_summary else ''}`")
lines.append(f"- Foundation summary: `{foundation_summary if foundation_summary else ''}`")
lines.append(f"- macOS smoke summary: `{smoke_summary if smoke_summary else ''}`")
lines.append(f"- Command manifest: `{artifact_root / 'command-manifest.json'}`")
lines.append(f"- Checklist JSON: `{artifact_root / 'checklist.json'}`")
lines.append("")
lines.append("## Impacted ID checklist")
for row in checklist_rows:
    lines.append(
        f"- {row['id']}: {row['status'].upper()} (artifact: `{row['artifact_path']}`)"
    )
lines.append("")
lines.append("## Closure decision")
lines.append(
    "- Close `T-1.6` when all impacted IDs are PASS and non-impacted suite lanes do not introduce blockers."
)
lines.append(
    "- Note: `core_session` may exit non-zero if out-of-scope IDs fail, but closure remains valid when `E2E-01/02/03/04/06` case artifacts are PASS."
)

(artifact_root / "closure-handoff.md").write_text("\n".join(lines) + "\n", encoding="utf-8")

for line in env_lines:
    print(line)
PY
