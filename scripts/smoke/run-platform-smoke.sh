#!/usr/bin/env sh
set -eu

if [ "$#" -lt 1 ]; then
  echo "usage: sh scripts/smoke/run-platform-smoke.sh <linux|macos> [--dry-run]" >&2
  exit 64
fi

platform="$1"
shift

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

case "$platform" in
  linux|macos) ;;
  *)
    echo "unsupported platform '$platform' (expected linux or macos)" >&2
    exit 64
    ;;
esac

max_cargo_jobs="${EZM_SMOKE_MAX_CARGO_JOBS:-2}"
test_threads="${EZM_SMOKE_TEST_THREADS:-1}"

case "$max_cargo_jobs" in
  ''|*[!0-9]*)
    echo "EZM_SMOKE_MAX_CARGO_JOBS must be a positive integer" >&2
    exit 64
    ;;
esac

case "$test_threads" in
  ''|*[!0-9]*)
    echo "EZM_SMOKE_TEST_THREADS must be a positive integer" >&2
    exit 64
    ;;
esac

if [ "$max_cargo_jobs" -lt 1 ] || [ "$test_threads" -lt 1 ]; then
  echo "EZM_SMOKE_MAX_CARGO_JOBS and EZM_SMOKE_TEST_THREADS must be >= 1" >&2
  exit 64
fi

host_os="$(uname -s)"
expected_os=""
case "$platform" in
  linux) expected_os="Linux" ;;
  macos) expected_os="Darwin" ;;
esac

tmp_base="${TMPDIR:-/tmp}"
if [ "$platform" = "macos" ]; then
  tmp_base="/tmp"
fi

tmp_root="$(mktemp -d "$tmp_base/ezm-smoke-${platform}-XXXXXX")"
namespace="smoke-${platform}-$(date +%s)-$$"

cleanup() {
  if command -v tmux >/dev/null 2>&1; then
    tmux -L "$namespace" kill-server >/dev/null 2>&1 || true
  fi
  rm -rf "$tmp_root"
}
trap cleanup EXIT INT TERM

resource_snapshot() {
  phase="$1"
  printf '%s\n' "resource checkpoint ($phase):"
  if command -v ps >/dev/null 2>&1; then
    ps -eo pid,ppid,pcpu,rss,comm,args | grep -E 'PID|cargo|rustc|tmux|opencode|ezm' || true
  else
    printf '%s\n' "ps unavailable"
  fi
}

if [ "$dry_run" -eq 1 ]; then
  echo "EZM_SMOKE_PLATFORM=$platform"
  echo "EZM_SMOKE_NAMESPACE=$namespace"
  echo "TMPDIR=$tmp_root"
  echo "EZM_SMOKE_MAX_CARGO_JOBS=$max_cargo_jobs"
  echo "EZM_SMOKE_TEST_THREADS=$test_threads"
  echo "cargo test --test smoke_e2e -- --nocapture --test-threads $test_threads"
  exit 0
fi

if [ "$host_os" != "$expected_os" ]; then
  echo "platform mismatch: host '$host_os' does not match smoke target '$platform'" >&2
  exit 65
fi

resource_snapshot "before-smoke"

EZM_SMOKE_PLATFORM="$platform" \
EZM_SMOKE_NAMESPACE="$namespace" \
TMPDIR="$tmp_root" \
CARGO_BUILD_JOBS="$max_cargo_jobs" \
RUST_TEST_THREADS="$test_threads" \
cargo test --test smoke_e2e -- --nocapture --test-threads "$test_threads"

resource_snapshot "after-smoke"
