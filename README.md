# Bindery

Bind your audiobook chapters into a single M4B.

Drop in your chapter files (MP3, M4A, M4B), and Bindery merges them into one audiobook with chapters, metadata, and cover art — ready for any player.

<!-- ![Bindery screenshot](docs/screenshot.png) -->

## Features

- **Drag and drop** — add files, reorder chapters, done
- **Smart encoding** — remuxes AAC sources without re-encoding, only transcodes MP3s
- **Chapter support** — preserves or creates chapter markers automatically
- **Metadata & cover art** — auto-fills from your files, fully editable
- **Configurable quality** — choose your bitrate (64–320 kbps)
- **Cross-platform** — macOS, Windows, Linux
- **Offline** — no uploads, no accounts, no tracking

## Tech stack

- [Tauri 2](https://tauri.app) — Rust backend, native shell
- [Svelte 5](https://svelte.dev) — reactive UI
- [Vite](https://vite.dev) — frontend build
- [ffmpeg](https://ffmpeg.org) — audio processing (bundled)

## Development

```bash
npm install
npm run tauri dev
```

## Build

```bash
npm run tauri build
```

The build output (`.dmg`, `.msi`, `.AppImage`) will be in `src-tauri/target/release/bundle/`.

## License

Proprietary. All rights reserved.
