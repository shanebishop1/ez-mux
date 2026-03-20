#!/usr/bin/env sh
set -eu

if [ "$#" -lt 1 ]; then
  echo "usage: sh scripts/install/run-platform-install-validation.sh <linux|macos> [--candidate-bin <path> | --candidate-package <path>] [--dry-run]" >&2
  exit 64
fi

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"

platform="$1"
shift

candidate_kind=""
candidate_path=""
dry_run=0

while [ "$#" -gt 0 ]; do
  case "$1" in
    --candidate-bin)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --candidate-bin" >&2
        exit 64
      fi
      candidate_kind="binary"
      candidate_path="$2"
      shift 2
      ;;
    --candidate-package)
      if [ "$#" -lt 2 ]; then
        echo "missing value for --candidate-package" >&2
        exit 64
      fi
      candidate_kind="package"
      candidate_path="$2"
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 64
      ;;
  esac
done

case "$platform" in
  linux|macos) ;;
  *)
    echo "unsupported platform '$platform' (expected linux or macos)" >&2
    exit 64
    ;;
esac

if [ -z "$candidate_kind" ]; then
  if [ "$dry_run" -eq 1 ]; then
    candidate_kind="binary"
    candidate_path="<candidate-path>"
  else
    echo "candidate artifact is required (use --candidate-bin or --candidate-package)" >&2
    exit 64
  fi
fi

host_os="$(uname -s)"
expected_os=""
case "$platform" in
  linux) expected_os="Linux" ;;
  macos) expected_os="Darwin" ;;
esac

tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/ezm-install-${platform}-XXXXXX")"
install_root="$tmp_root/install-root"
install_bin_dir="$install_root/bin"
staging_dir="$tmp_root/staging"

run_id="run-$(date +%s)-$$"
artifact_dir="$repo_root/target/e2e-evidence/install-validation/$run_id"
contract_dir="$artifact_dir/contract-smoke"
summary_path="$artifact_dir/summary.json"
envelope_path="$artifact_dir/envelope.json"
help_output_path="$contract_dir/help.txt"
version_output_path="$contract_dir/version.txt"

help_contains_usage=false
version_contains_ezm=false
installed_ezm=""
validation_status="failed"
commit_sha="unknown"
shell_path="${SHELL:-unknown}"
tmux_version="unknown"
test_ids_json='["E2E-00"]'
pass_total=0
fail_total=1

json_escape() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

write_summary() {
  mkdir -p "$artifact_dir"

  run_id_json="$(json_escape "$run_id")"
  platform_json="$(json_escape "$platform")"
  candidate_kind_json="$(json_escape "$candidate_kind")"
  candidate_path_json="$(json_escape "$candidate_path")"
  install_root_json="$(json_escape "$install_root")"
  installed_ezm_json="$(json_escape "$installed_ezm")"
  help_output_path_json="$(json_escape "$help_output_path")"
  version_output_path_json="$(json_escape "$version_output_path")"
  validation_status_json="$(json_escape "$validation_status")"
  commit_sha_json="$(json_escape "$commit_sha")"
  shell_path_json="$(json_escape "$shell_path")"
  tmux_version_json="$(json_escape "$tmux_version")"

  cat > "$summary_path" <<EOF
{
  "run_id": "$run_id_json",
  "suite": "install-validation",
  "metadata": {
    "commit_sha": "$commit_sha_json",
    "os": "$platform_json",
    "shell": "$shell_path_json",
    "tmux_version": "$tmux_version_json",
    "test_ids": $test_ids_json,
    "pass_total": $pass_total,
    "fail_total": $fail_total
  },
  "platform": "$platform_json",
  "candidate_kind": "$candidate_kind_json",
  "candidate_path": "$candidate_path_json",
  "install_root": "$install_root_json",
  "installed_ezm": "$installed_ezm_json",
  "contract_smoke": {
    "help_output_path": "$help_output_path_json",
    "version_output_path": "$version_output_path_json",
    "help_contains_usage": $help_contains_usage,
    "version_contains_ezm": $version_contains_ezm
  },
  "status": "$validation_status_json"
}
EOF

  cat > "$envelope_path" <<EOF
{
  "run_id": "$run_id_json",
  "suite": "install-validation",
  "metadata": {
    "commit_sha": "$commit_sha_json",
    "os": "$platform_json",
    "shell": "$shell_path_json",
    "tmux_version": "$tmux_version_json",
    "test_ids": $test_ids_json,
    "pass_total": $pass_total,
    "fail_total": $fail_total
  },
  "platform": "$platform_json",
  "candidate_kind": "$candidate_kind_json",
  "candidate_path": "$candidate_path_json",
  "install_root": "$install_root_json",
  "installed_ezm": "$installed_ezm_json",
  "contract_smoke": {
    "help_output_path": "$help_output_path_json",
    "version_output_path": "$version_output_path_json",
    "help_contains_usage": $help_contains_usage,
    "version_contains_ezm": $version_contains_ezm
  },
  "status": "$validation_status_json"
}
EOF
}

print_machine_readable_paths() {
  echo "EZM_EVIDENCE_ARTIFACT_DIR=$artifact_dir"
  echo "EZM_EVIDENCE_SUMMARY_JSON=$summary_path"
  echo "EZM_EVIDENCE_ENVELOPE_JSON=$envelope_path"
  echo "EZM_EVIDENCE_HELP_OUTPUT=$help_output_path"
  echo "EZM_EVIDENCE_VERSION_OUTPUT=$version_output_path"
}

cleanup() {
  rm -rf "$tmp_root"
}
trap cleanup EXIT INT TERM

if [ "$dry_run" -eq 1 ]; then
  print_machine_readable_paths
  echo "EZM_INSTALL_PLATFORM=$platform"
  echo "EZM_CANDIDATE_KIND=$candidate_kind"
  echo "EZM_CANDIDATE_PATH=$candidate_path"
  echo "EZM_INSTALL_ROOT=$install_root"
  case "$candidate_kind" in
    binary)
      echo "install -m 755 $candidate_path $install_bin_dir/ezm"
      ;;
    package)
      echo "tar -xf $candidate_path -C $staging_dir"
      echo "install extracted ezm binary into $install_bin_dir/ezm"
      ;;
  esac
  echo "PATH=$install_bin_dir:\$PATH ezm --help"
  echo "PATH=$install_bin_dir:\$PATH ezm --version"
  exit 0
fi

if [ "$host_os" != "$expected_os" ]; then
  echo "platform mismatch: host '$host_os' does not match install target '$platform'" >&2
  exit 65
fi

if commit_sha_out="$(git -C "$repo_root" rev-parse HEAD 2>/dev/null)"; then
  commit_sha="$commit_sha_out"
fi

if tmux_version_out="$(tmux -V 2>/dev/null)"; then
  tmux_version="$tmux_version_out"
fi

if [ ! -f "$candidate_path" ]; then
  echo "candidate artifact does not exist: $candidate_path" >&2
  exit 66
fi

mkdir -p "$install_bin_dir" "$staging_dir"
mkdir -p "$contract_dir"

case "$candidate_kind" in
  binary)
    install -m 755 "$candidate_path" "$install_bin_dir/ezm"
    ;;
  package)
    tar -xf "$candidate_path" -C "$staging_dir"

    extracted_binary=""
    if [ -x "$staging_dir/ezm" ]; then
      extracted_binary="$staging_dir/ezm"
    elif [ -x "$staging_dir/bin/ezm" ]; then
      extracted_binary="$staging_dir/bin/ezm"
    else
      for path in "$staging_dir"/*/ezm "$staging_dir"/*/bin/ezm; do
        if [ -x "$path" ]; then
          extracted_binary="$path"
          break
        fi
      done
    fi

    if [ -z "$extracted_binary" ]; then
      echo "no executable ezm found in package: $candidate_path" >&2
      exit 67
    fi

    install -m 755 "$extracted_binary" "$install_bin_dir/ezm"
    ;;
  *)
    echo "internal error: unsupported candidate kind '$candidate_kind'" >&2
    exit 70
    ;;
esac

installed_ezm="$(PATH="$install_bin_dir:$PATH" command -v ezm)"
if [ "$installed_ezm" != "$install_bin_dir/ezm" ]; then
  echo "unexpected ezm resolution after install: $installed_ezm" >&2
  exit 68
fi

help_output="$(PATH="$install_bin_dir:$PATH" ezm --help)"
version_output="$(PATH="$install_bin_dir:$PATH" ezm --version)"

printf '%s\n' "$help_output" > "$help_output_path"
printf '%s\n' "$version_output" > "$version_output_path"

case "$help_output" in
  *Usage:*) help_contains_usage=true ;;
  *)
    validation_status="failed"
    write_summary
    print_machine_readable_paths
    echo "post-install check failed: --help output missing Usage banner" >&2
    exit 69
    ;;
esac

case "$version_output" in
  *ezm*) version_contains_ezm=true ;;
  *)
    validation_status="failed"
    write_summary
    print_machine_readable_paths
    echo "post-install check failed: --version output missing ezm token" >&2
    exit 69
    ;;
esac

validation_status="passed"
pass_total=1
fail_total=0
write_summary

echo "install validation passed"
echo "installed_ezm=$installed_ezm"
echo "version_output=$version_output"
print_machine_readable_paths
