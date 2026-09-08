#!/usr/bin/env sh
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <linux|macos> <release-archive>" >&2
  exit 64
fi

platform="$1"
archive="$2"

case "$platform" in
  linux|macos) ;;
  *) echo "unsupported platform: $platform" >&2; exit 64 ;;
esac

: "${RELEASE_VERSION:?RELEASE_VERSION must contain the validated package version}"
: "${RELEASE_TAG:?RELEASE_TAG must contain the validated release tag}"

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
verification_path="$repo_root/dist/native-verification-$platform.json"

python3 "$script_dir/verify_release_artifact.py" \
  --archive "$archive" \
  --expected-version "$RELEASE_VERSION" \
  --platform "$platform" \
  --output "$verification_path"

sh "$repo_root/scripts/install/run-platform-install-validation.sh" \
  "$platform" \
  --candidate-package "$archive" \
  --expected-version "$RELEASE_VERSION"

echo "native release verification passed for $RELEASE_TAG ($platform)"
