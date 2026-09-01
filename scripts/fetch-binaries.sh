#!/usr/bin/env bash
# Downloads ffmpeg + ffprobe for the current Rust host target (or all targets
# via FETCH_ALL=1) and drops them in src-tauri/binaries/ with the
# tauri-sidecar naming convention: <name>-<triple>[.exe].
#
# Usage:
#   ./scripts/fetch-binaries.sh             # current host triple only
#   FETCH_ALL=1 ./scripts/fetch-binaries.sh # all supported triples
#
# Supported triples:
#   aarch64-apple-darwin        (macOS Apple Silicon)
#   x86_64-apple-darwin         (macOS Intel)
#   x86_64-unknown-linux-gnu    (Linux x86_64)
#   aarch64-unknown-linux-gnu   (Linux ARM64)
#   x86_64-pc-windows-msvc      (Windows x86_64)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT_DIR="$ROOT/src-tauri/binaries"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

mkdir -p "$OUT_DIR"

host_triple() {
  if command -v rustc >/dev/null 2>&1; then
    rustc -vV | awk '/host:/ {print $2}'
  else
    case "$(uname -sm)" in
      "Darwin arm64")  echo "aarch64-apple-darwin" ;;
      "Darwin x86_64") echo "x86_64-apple-darwin" ;;
      "Linux x86_64")  echo "x86_64-unknown-linux-gnu" ;;
      "Linux aarch64") echo "aarch64-unknown-linux-gnu" ;;
      *) echo "unknown"; return 1 ;;
    esac
  fi
}

# Install a fetched binary, replacing any previous file even if it was
# checked out read-only.
install_bin() {
  local src="$1" dest="$2"
  rm -f "$dest"
  cp "$src" "$dest"
  chmod +x "$dest"
}

# Extract a zip without assuming `unzip` exists. The GitHub Windows runner
# ships neither a guaranteed unzip nor (since 2024) 7-Zip, so fall back to
# PowerShell and then Python before giving up.
extract_zip() {
  local archive="$1" dest="$2"
  mkdir -p "$dest"
  if command -v unzip >/dev/null 2>&1; then
    unzip -q "$archive" -d "$dest"
  elif command -v pwsh >/dev/null 2>&1 || command -v powershell >/dev/null 2>&1; then
    local ps; ps="$(command -v pwsh || command -v powershell)"
    "$ps" -NoProfile -Command \
      "Expand-Archive -LiteralPath '$archive' -DestinationPath '$dest' -Force"
  elif command -v python3 >/dev/null 2>&1; then
    python3 -c "import sys,zipfile; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])" "$archive" "$dest"
  else
    echo "No way to extract $archive (need unzip, PowerShell, or python3)" >&2
    return 1
  fi
}

fetch_btbn() {
  # BtbN builds: single archive contains ffmpeg + ffprobe under bin/
  local triple="$1" url="$2" archive="$3"
  echo "→ $triple"
  local ext_suffix=""
  [[ "$triple" == *windows* ]] && ext_suffix=".exe"
  local archive_path="$TMP_DIR/$archive"
  curl -fsSL -o "$archive_path" "$url"
  local extract_dir="$TMP_DIR/extract-$triple"
  mkdir -p "$extract_dir"
  case "$archive" in
    *.zip)    extract_zip "$archive_path" "$extract_dir" ;;
    *.tar.xz) tar -xf "$archive_path" -C "$extract_dir" ;;
    *) echo "Unknown archive: $archive"; return 1 ;;
  esac
  local ffmpeg_src ffprobe_src
  ffmpeg_src="$(find "$extract_dir" -type f -name "ffmpeg$ext_suffix" | head -1)"
  ffprobe_src="$(find "$extract_dir" -type f -name "ffprobe$ext_suffix" | head -1)"
  [[ -z "$ffmpeg_src" || -z "$ffprobe_src" ]] && { echo "Binaries not found in $archive"; return 1; }
  install_bin "$ffmpeg_src"  "$OUT_DIR/ffmpeg-$triple$ext_suffix"
  install_bin "$ffprobe_src" "$OUT_DIR/ffprobe-$triple$ext_suffix"
}

fetch_evermeet() {
  # evermeet.cx ships ffmpeg and ffprobe as separate zips (x86_64-apple-darwin)
  local triple="$1"
  echo "→ $triple"
  for bin in ffmpeg ffprobe; do
    local zip_path="$TMP_DIR/$bin-$triple.zip"
    curl -fsSL -o "$zip_path" "https://evermeet.cx/ffmpeg/getrelease/$bin/zip"
    local extract_dir="$TMP_DIR/extract-$bin-$triple"
    mkdir -p "$extract_dir"
    extract_zip "$zip_path" "$extract_dir"
    install_bin "$extract_dir/$bin" "$OUT_DIR/$bin-$triple"
  done
}

fetch_osxexperts() {
  # osxexperts.net: ffmpeg + ffprobe zipped individually for arm64
  local triple="$1"
  echo "→ $triple"
  for bin in ffmpeg ffprobe; do
    local zip_path="$TMP_DIR/$bin-$triple.zip"
    curl -fsSL -o "$zip_path" "https://www.osxexperts.net/${bin}71arm.zip"
    local extract_dir="$TMP_DIR/extract-$bin-$triple"
    mkdir -p "$extract_dir"
    extract_zip "$zip_path" "$extract_dir"
    local src
    src="$(find "$extract_dir" -type f -name "$bin" | head -1)"
    [[ -z "$src" ]] && { echo "$bin not found in archive"; return 1; }
    install_bin "$src" "$OUT_DIR/$bin-$triple"
  done
}

fetch_for_triple() {
  local triple="$1"
  case "$triple" in
    aarch64-apple-darwin)
      fetch_osxexperts "$triple" ;;
    x86_64-apple-darwin)
      fetch_evermeet "$triple" ;;
    x86_64-unknown-linux-gnu)
      fetch_btbn "$triple" \
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz" \
        "ffmpeg-linux64.tar.xz" ;;
    aarch64-unknown-linux-gnu)
      fetch_btbn "$triple" \
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linuxarm64-gpl.tar.xz" \
        "ffmpeg-linuxarm64.tar.xz" ;;
    x86_64-pc-windows-msvc)
      fetch_btbn "$triple" \
        "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip" \
        "ffmpeg-win64.zip" ;;
    *)
      echo "Unsupported triple: $triple" >&2
      return 1 ;;
  esac
}

# Tauri's `--target universal-apple-darwin` looks for sidecars suffixed
# `-universal-apple-darwin`; they don't exist upstream, so lipo the two
# per-arch downloads together whenever both are present (macOS only).
make_universal_macos() {
  command -v lipo >/dev/null 2>&1 || return 0
  for bin in ffmpeg ffprobe; do
    local arm="$OUT_DIR/$bin-aarch64-apple-darwin"
    local x86="$OUT_DIR/$bin-x86_64-apple-darwin"
    if [[ -f "$arm" && -f "$x86" ]]; then
      echo "→ lipo $bin-universal-apple-darwin"
      rm -f "$OUT_DIR/$bin-universal-apple-darwin"
      lipo -create "$arm" "$x86" -output "$OUT_DIR/$bin-universal-apple-darwin"
      chmod +x "$OUT_DIR/$bin-universal-apple-darwin"
    fi
  done
}

if [[ "${FETCH_ALL:-0}" == "1" ]]; then
  for t in \
    aarch64-apple-darwin \
    x86_64-apple-darwin \
    x86_64-unknown-linux-gnu \
    aarch64-unknown-linux-gnu \
    x86_64-pc-windows-msvc
  do
    fetch_for_triple "$t"
  done
  make_universal_macos
else
  HOST="$(host_triple)"
  [[ "$HOST" == "unknown" ]] && { echo "Could not detect host triple; pass FETCH_ALL=1 or install rustc"; exit 1; }
  fetch_for_triple "$HOST"
fi

echo
echo "✓ Done. Binaries in: $OUT_DIR"
ls -la "$OUT_DIR"

# Fail fast if anything fetched is not distributable.
"$ROOT/scripts/check-binaries.sh"
