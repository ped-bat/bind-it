# Bindery

Bind your audiobook chapters into a single M4B.

Bindery is a native desktop app that merges chapter files (MP3, M4A, M4B) into one
M4B audiobook with chapters, metadata, and cover art — ready for any player.

## Features

- **Lossless remux** — AAC files are concatenated without re-encoding, preserving original quality
- **Parallel transcoding** — MP3 and mixed-codec inputs are transcoded to AAC using all available CPU cores
- **Automatic chapters** — chapter markers are created from each input file
- **Metadata and cover art** — auto-detected from source files, fully editable before export
- **Drag and drop** — add files by dropping them onto the window; reorder by dragging
- **Folder support** — drop a folder to import all audio files inside it
- **Configurable quality** — choose your output bitrate (64–320 kbps)
- **Dark mode** — light and dark themes that follow system preference
- **Accessible** — keyboard navigable with proper focus management
- **Offline and private** — no uploads, no accounts, no tracking

## How It Works

Bindery inspects the codec of every input file and picks the fastest merge path:

1. **Remux** — when all files are AAC (M4A/M4B), they are concatenated directly into the
   output container with no re-encoding. This is near-instant and completely lossless.

2. **Transcode** — when all files are MP3 (or another non-AAC codec), each file is transcoded
   to AAC in parallel across all CPU cores, then concatenated into the final M4B.

3. **Mixed** — when the input contains a mix of AAC and non-AAC files, only the non-AAC files
   are transcoded. AAC files that already match the target sample rate are passed through
   untouched. Everything is then concatenated in the correct order.

In all three paths, chapter metadata and cover art are written into the output file as a final step.

## System Requirements

- **macOS** on Apple Silicon (arm64) — other platforms are planned but not yet tested
- **ffmpeg** — must be installed and available on `PATH` (e.g. `brew install ffmpeg`)

## Development

Prerequisites: [Node.js](https://nodejs.org/), [Rust](https://www.rust-lang.org/tools/install),
and the [Tauri 2 CLI prerequisites](https://tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

The build output (`.dmg`) will be in `src-tauri/target/release/bundle/`.

## Tech Stack

- [Tauri 2](https://tauri.app) — Rust backend, native window shell
- [Svelte 5](https://svelte.dev) — reactive UI
- [SvelteKit](https://svelte.dev/docs/kit) — routing and static adapter
- [Vite](https://vite.dev) — frontend build tooling
- [ffmpeg](https://ffmpeg.org) — audio decoding, encoding, and muxing
- [Rayon](https://docs.rs/rayon) — parallel transcoding across CPU cores

## License

Proprietary. All rights reserved.
