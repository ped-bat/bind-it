#!/usr/bin/env bash
# Generate synthetic audio fixtures for manual testing of Bindery.
# Each subfolder is a complete "book" tailored to a specific code path.
#
# Usage: ./scripts/make-fixtures.sh [output-dir]
# Default output: ./test-fixtures/

set -euo pipefail

OUT="${1:-./test-fixtures}"
mkdir -p "$OUT"

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg not found. Install: brew install ffmpeg" >&2
  exit 1
fi

gen() {
  # gen <path> <freq> <duration> <extra ffmpeg args...>
  local path="$1" freq="$2" dur="$3"; shift 3
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i "sine=frequency=${freq}:duration=${dur}:sample_rate=44100" \
    "$@" "$path"
}

# ── Scenario 1: all AAC → Lossless mode remuxes (no warnings) ────────────────
D="$OUT/all-aac"; mkdir -p "$D"
gen "$D/01 Chapter One.m4a"   440 3 -c:a aac -b:a 64k
gen "$D/02 Chapter Two.m4a"   494 3 -c:a aac -b:a 64k
gen "$D/03 Chapter Three.m4a" 523 3 -c:a aac -b:a 64k

# ── Scenario 2: FLAC + WAV → Lossless emits ALAC (compat warning only) ───────
D="$OUT/flac-wav-lossless"; mkdir -p "$D"
gen "$D/01 Intro.flac"  440 3 -c:a flac
gen "$D/02 Middle.wav"  523 3
gen "$D/03 Outro.flac"  659 3 -c:a flac

# ── Scenario 3: MP3 + FLAC → Lossless triggers both warnings ─────────────────
D="$OUT/lossy-source"; mkdir -p "$D"
gen "$D/01 Lossy Part.mp3" 440 3 -c:a libmp3lame -b:a 128k
gen "$D/02 Clean Part.flac" 523 3 -c:a flac

# ── Scenario 4: all MP3 → Compress mode transcodes to AAC ────────────────────
D="$OUT/all-mp3"; mkdir -p "$D"
gen "$D/01 Part One.mp3" 440 3 -c:a libmp3lame -b:a 128k
gen "$D/02 Part Two.mp3" 523 3 -c:a libmp3lame -b:a 128k

# ── Scenario 5: mismatched sample rates within AAC ───────────────────────────
D="$OUT/aac-mismatched-sr"; mkdir -p "$D"
ffmpeg -hide_banner -loglevel error -y \
  -f lavfi -i "sine=frequency=440:duration=3:sample_rate=44100" \
  -c:a aac -b:a 64k "$D/01 Track A.m4a"
ffmpeg -hide_banner -loglevel error -y \
  -f lavfi -i "sine=frequency=523:duration=3:sample_rate=22050" \
  -c:a aac -b:a 64k "$D/02 Track B.m4a"

echo ""
echo "Fixtures written to: $OUT"
echo ""
echo "Drop any subfolder into the app:"
ls -1 "$OUT"
