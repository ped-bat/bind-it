# Bind it

Bind your audio file chapters into a single M4B.

Bind it is a native desktop app that merges chapter files (MP3, M4A, M4B, AAC,
WAV, FLAC, WMA) into one audio file with chapters, metadata, and cover art —
ready for any player. Output is `.m4b`, or `.mp3` when the source set is
uniform MP3 and you keep the original codec.

## Features

- **Lossless remux** — AAC files are concatenated without re-encoding, preserving original quality
- **Parallel transcoding** — MP3 and mixed-codec inputs are transcoded to AAC using all available CPU cores
- **Automatic chapters** — chapter markers are created from each input file
- **Metadata and cover art** — auto-detected from source files, fully editable before export
- **Drag and drop** — add files by dropping them onto the window with drag and drop to reorder chapters
- **Folder support** — drop a folder to import all audio files inside it
- **Configurable quality** — choose your output bitrate (64–320 kbps)
- **Offline and private** — no uploads, no accounts, no tracking

## How It Works

Bind it inspects the codec of every input file and picks the fastest merge path:

1. **Remux** — when all files are AAC (M4A/M4B), they are concatenated directly into the
   output container with no re-encoding. This is near-instant and completely lossless.

2. **Transcode** — when all files are MP3 (or another non-AAC codec), each file is transcoded
   to AAC in parallel across all CPU cores, then concatenated into the final M4B.

3. **Mixed** — when the input contains a mix of AAC and non-AAC files, only the non-AAC files
   are transcoded. AAC files that already match the target sample rate are passed through
   untouched. Everything is then concatenated in the correct order.

In all three paths, chapter metadata and cover art are written into the output file as a final step.

## System Requirements

- **macOS** 12+ on Apple Silicon, 11+ on Intel; **Windows** 10/11 (x86_64); or
  **Linux** x86_64 (AppImage) with glibc 2.35 or newer — Ubuntu 22.04+,
  Debian 12+, Fedora 36+, Linux Mint 21+
- Nothing else — `ffmpeg` and `ffprobe` ship inside the app

## Development

Prerequisites: [Node.js](https://nodejs.org/), [Rust](https://www.rust-lang.org/tools/install),
and the [Tauri 2 CLI prerequisites](https://tauri.app/start/prerequisites/).

```bash
npm install
./scripts/fetch-binaries.sh   # download ffmpeg/ffprobe sidecars for your host
npm run tauri dev
```

The sidecars are not committed to the repository. `fetch-binaries.sh`
downloads static builds and then runs `scripts/check-binaries.sh`, which
rejects dynamically-linked binaries — a Homebrew ffmpeg copied into
`src-tauri/binaries/` runs fine locally but fails on every machine without
the same libraries installed.

## Build

```bash
FETCH_ALL=1 ./scripts/fetch-binaries.sh   # all target triples + universal macOS
npm run tauri build
```

Bundles land in `src-tauri/target/release/bundle/` (`.dmg`/`.app` on macOS,
`.msi`/`.exe` on Windows, `.AppImage` on Linux). Tagging `v*` runs
`.github/workflows/release.yml`, which builds all three platforms, signs and
notarizes the macOS build, and uploads installers to a draft release.

The batch CLI (`bind-it-cli`) is a development tool. It lives in
`src-tauri/examples/` rather than `src/bin/`, because Tauri's bundler copies
every declared binary target into the shipped app:

```bash
cargo run --release --example bind-it-cli -- <INPUT_DIR> [OPTIONS]
```

## Testing

```bash
cd src-tauri && cargo test --lib   # end-to-end merge tests on synthetic fixtures
npm run check                      # svelte-check
```

## Tech Stack

- [Tauri 2](https://tauri.app) — Rust backend, native window shell
- [Svelte 5](https://svelte.dev) — reactive UI
- [SvelteKit](https://svelte.dev/docs/kit) — routing and static adapter
- [Vite](https://vite.dev) — frontend build tooling
- [ffmpeg](https://ffmpeg.org) — audio decoding, encoding, and muxing
- [Rayon](https://docs.rs/rayon) — parallel transcoding across CPU cores

## License

MIT — see [LICENSE](LICENSE). Copyright © 2026 Pedro Batista.

Bundled `ffmpeg`/`ffprobe` binaries are GPL-licensed and distributed under
their own terms — see [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
