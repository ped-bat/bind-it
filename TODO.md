- [X] Remove unecessary code
- [X] Check build errors
- [X] Open folder at the end not working
- [X] Fix dock app icon
- [X] Fields still have a lot of repeated css
- [X] Fix icon used accross screens
- [X] Add splash screen
- [X] During process the description jumps between Transcoding and transcoded
- [X] Fix progress bar shine animation - it's inconsistent rn
- [X] Progress bar seems to do 20-60% on transcoding, 80 to concat 90 to metadata, but these last two are quick
- [X] Remember preferences between sessions
- [X] Fix lossless / compress UI
- [X] Add support for all main audio formats including FLAC and WAV
- [X] Stress test app
- [X] Add lottie animations
- [X] Register domain
- [X] Add domain to siteground
- [X] Add SSL
- [X] Create repo for website on git
- [X] Add repo ftp workflow
- [X] On the chapters file list, when the file size is large the string breaks into two lines, instead the columns should be flexible to adapt for possible longer strings on some columns
- [X] On the output panel when io press the input text box for the file name, the extension box on the right side of the text inpout remains in it's normal state, instead the border should behave accordingly
- [X] If i try to clear the folder or filename text input and leave it empty and press bind it button nothing happens - there should be fields validation on these two, with a red border when empty and if user presses bind it still, it should say please fill "input name"
- [X] Quality panel on the expected output is missing the experected file size with mp3's
- [X] Let's make the default size of the app window larger
- [X] If i try to drag a track that's not the first to the first position it doesn't work
- [X] On the success page after completion let's also show the output format and codec, same rules as shown on the expexted output in the settings page
- [X] On the settings page the expected output string, should be the same font size as the text input fields on the rest of the panel like the string inside filename input field
- [X] When i hover the app on the macbook dock it should say "Bind it" not "bind-it"
- [X] On the menu Bind it > About bind-it, should be About Bind it
- [X] Let's change the version of the app to 1.0
- [X] On the menu Bind it > About Bind it should show: Author: Pedro Batista 
- [X] Add it to new repository on my account
- [ ] Delete bindery repository
- [ ] Add website to the main repository?
- [ ] Convert all audiobooks
- [ ] Create landing page

## Release readiness (from the Aug 2026 audit)

Fixed on the `release-fixes` branch:

- [X] ffmpeg sidecars were Homebrew-linked and broke on any other machine —
      fetch script fixed (its Apple Silicon URL never worked), static builds,
      guard script blocks regressions
- [X] Universal macOS build needs lipo'd sidecars — done in fetch script
- [X] Cmd/Ctrl+Backspace wiped the session while typing — guarded + confirmed
- [X] Windows console windows on every ffmpeg call — CREATE_NO_WINDOW
- [X] Illegal filename characters reached the OS — stripped and validated
- [X] Drop errors broke the conversion state machine
- [X] No drag-over feedback on Windows/Linux — now uses native Tauri events
- [X] "brew install ffmpeg" shown on every platform — per-platform hints
- [X] Dev CLI shipped inside the app bundle — behind a cargo feature
- [X] No ffmpeg GPL notices — THIRD_PARTY_LICENSES.md bundled as a resource
- [X] deb/rpm would collide with the distro's ffmpeg — AppImage only
- [X] No test workflow — ci.yml runs tests, clippy, svelte-check, build
- [X] Working tree uncommitted, README contradicted the product

Still to do — these need machines and accounts, not code:

- [ ] Dry-run the release workflow (`workflow_dispatch`) on all three platforms
- [ ] Verify Developer ID signing + notarization on a clean Mac (the local
      build is ad-hoc signed; Gatekeeper will reject it as-is)
- [ ] Test fresh installation on another mac
- [ ] Test installation on windows
- [ ] Test installation on linux
- [ ] Tag v1.0.0 and publish website + app