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
  rm -rf "$tmp_root"
}
trap cleanup EXIT INT TERM

if [ "$dry_run" -eq 1 ]; then
  echo "EZM_SMOKE_PLATFORM=$platform"
  echo "EZM_SMOKE_NAMESPACE=$namespace"
  echo "TMPDIR=$tmp_root"
  echo "cargo test --test smoke_e2e -- --nocapture"
  exit 0
fi

if [ "$host_os" != "$expected_os" ]; then
  echo "platform mismatch: host '$host_os' does not match smoke target '$platform'" >&2
  exit 65
fi

EZM_SMOKE_PLATFORM="$platform" \
EZM_SMOKE_NAMESPACE="$namespace" \
TMPDIR="$tmp_root" \
cargo test --test smoke_e2e -- --nocapture
