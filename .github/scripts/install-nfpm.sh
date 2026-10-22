#!/usr/bin/env bash
# Install the version pinned in native-packages.yaml on Linux CI runners.
set -euo pipefail
output=${1:-.cache/bin}
mkdir -p "$output"
output=$(realpath "$output")
case "$(uname -m)" in
  x86_64) arch=x86_64 ;;
  aarch64) arch=arm64 ;;
  *) echo 'Unsupported nFPM architecture' >&2; exit 1 ;;
esac
version=2.47.0
archive="nfpm_${version}_Linux_${arch}.tar.gz"
work=$(mktemp -d "$output/nfpm-download.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
base="https://github.com/goreleaser/nfpm/releases/download/v$version"
curl --fail --location --retry 3 "$base/$archive" --output "$work/$archive"
curl --fail --location --retry 3 "$base/checksums.txt" --output "$work/checksums.txt"
(cd "$work"
 awk -v file="$archive" '$2 == file { print }' checksums.txt > selected-checksum.txt
 test "$(wc -l < selected-checksum.txt)" -eq 1
 sha256sum --check --strict selected-checksum.txt)
tar xzf "$work/$archive" -C "$output" nfpm
if [[ -n "${GITHUB_PATH:-}" ]]; then
  echo "$output" >> "$GITHUB_PATH"
fi
"$output/nfpm" --version
