#!/usr/bin/env bash
# Guards against shipping non-distributable ffmpeg sidecars.
#
# The failure mode this exists for: copying a Homebrew (or otherwise
# dynamically-linked) ffmpeg into src-tauri/binaries/. It runs fine on the
# build machine, then dies with a dyld error on every user machine that
# doesn't have the same libraries installed. Every sidecar must be a
# self-contained static build.
#
# Checks, per binary present in src-tauri/binaries/:
#   1. Size sanity — static ffmpeg builds are tens of MB; anything under
#      10 MB is almost certainly a dynamic stub.
#   2. (Mach-O, macOS host only) otool -L must not reference Homebrew,
#      MacPorts, or /usr/local library paths.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN_DIR="$ROOT/src-tauri/binaries"
MIN_BYTES=$((10 * 1024 * 1024))

fail=0
found=0

file_size() {
  if stat --version >/dev/null 2>&1; then
    stat -c%s "$1"   # GNU
  else
    stat -f%z "$1"   # BSD/macOS
  fi
}

for f in "$BIN_DIR"/ffmpeg-* "$BIN_DIR"/ffprobe-*; do
  [[ -f "$f" ]] || continue
  found=1
  name="$(basename "$f")"

  size="$(file_size "$f")"
  if (( size < MIN_BYTES )); then
    echo "✗ $name is only $((size / 1024)) KB — static ffmpeg builds are tens of MB." >&2
    echo "  This looks like a dynamically-linked stub that will break on user machines." >&2
    fail=1
    continue
  fi

  if [[ "$name" == *apple-darwin* ]] && command -v otool >/dev/null 2>&1; then
    bad_refs="$(otool -L "$f" 2>/dev/null | grep -E '/opt/homebrew|/opt/local|/usr/local/(Cellar|opt|lib)' || true)"
    if [[ -n "$bad_refs" ]]; then
      echo "✗ $name links against non-system libraries:" >&2
      echo "$bad_refs" >&2
      echo "  Re-run scripts/fetch-binaries.sh to get a static build." >&2
      fail=1
      continue
    fi
  fi

  echo "✓ $name ($((size / 1024 / 1024)) MB, self-contained)"
done

if (( found == 0 )); then
  echo "✗ No sidecar binaries found in $BIN_DIR — run scripts/fetch-binaries.sh first." >&2
  exit 1
fi

exit "$fail"
