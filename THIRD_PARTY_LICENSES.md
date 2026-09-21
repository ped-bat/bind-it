# Third-party licenses

Bind it is © Pedro Batista, released under the MIT license (see LICENSE).
It bundles and invokes the following third-party software as separate,
unmodified executables.

## FFmpeg (ffmpeg, ffprobe)

Bind it ships unmodified `ffmpeg` and `ffprobe` binaries and runs them as
separate processes for probing, transcoding, and muxing. FFmpeg is a
trademark of Fabrice Bellard, originator of the FFmpeg project.

The bundled builds are compiled with GPL components enabled and are
licensed under the **GNU General Public License version 3 or later**
(with parts under the LGPL v2.1+). Full license texts:
<https://www.gnu.org/licenses/gpl-3.0.html> ·
<https://www.gnu.org/licenses/lgpl-2.1.html>

FFmpeg source code is available at <https://ffmpeg.org/download.html>.
The exact builds bundled per platform, including their build scripts and
corresponding sources:

| Platform | Build provider |
| --- | --- |
| macOS (Apple Silicon) | <https://www.osxexperts.net> (`ffmpeg71arm.zip`, FFmpeg 7.1) |
| macOS (Intel) | <https://evermeet.cx/ffmpeg/> (`ffmpeg-8.1.zip`, FFmpeg 8.1) |
| Windows x86_64 | <https://github.com/BtbN/FFmpeg-Builds> (`ffmpeg-n8.1-latest-win64-gpl-8.1`) |
| Linux x86_64 | <https://github.com/BtbN/FFmpeg-Builds> (`ffmpeg-n8.1-latest-linux64-gpl-8.1`) |

Run `ffmpeg -version` from the app's own copy (next to the app executable)
to see the exact build string of an installed copy.

In accordance with the GPL, we will provide the complete corresponding
source code of the bundled FFmpeg binaries on request — the links above
carry it, or email me@pedrobatista.pt.

## Fonts

- Inter — SIL Open Font License 1.1 — <https://github.com/rsms/inter>
- Instrument Serif — SIL Open Font License 1.1 — <https://github.com/Instrument/instrument-serif>

## Frameworks

Bind it is built with [Tauri](https://tauri.app) (MIT/Apache-2.0),
[Svelte](https://svelte.dev) (MIT), and
[lottie-web](https://github.com/airbnb/lottie-web) (MIT).
