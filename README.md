# SlideshowMaker

A small Windows photo-story editor for turning family photographs, captions and music into a video. Original implementation and templates, with no affiliation to SmartSHOW. Local processing; no accounts, AI services, bundled music, or automatic media uploads.

**Status:** first local implementation, version 0.1.0. Windows x64 is the initial packaging target. Interactive Windows validation is pending. The planned [GitHub repository](https://github.com/siarheisinelnikau/slideshow-maker) and releases have not been published by this work.

## Create a video

1. Choose **Add photos** for JPEG, PNG, WebP or BMP images, or **Add title** for a title card.
2. Select a timeline card. Edit its caption, duration, rotation and **Fit** / **Fill**. Fit preserves the entire image; Fill deliberately crops from the center. JPEG orientation metadata is applied before your rotation.
3. Use the arrows to reorder slides. Choose landscape, portrait or square, one of three original color templates, and cuts, dissolves or fades through the background. Total duration proportionally rescales slide durations; transitions do not subtract from the total.
4. Optionally choose a **PCM 16-bit WAV** soundtrack (mono/stereo, 8–96 kHz, up to 128 MiB), adjust volume and enable repeat. Short tracks otherwise end in silence. The export applies a half-second fade at each end.
5. Scrub or play the **silent** timeline preview. Save an editable `.slideshow.json` project. Project files reference the original media; keep those files available or use **Relink photo** after moving them.
6. Export **AVI** directly, or select your trusted FFmpeg executable in **Settings** to export **MP4**. Choose a new destination filename. Export runs in the background with progress and cancellation.

Project files save atomically. Existing export files are never overwritten, including when cancellation or encoding fails. All media is read only. Save the project before closing or restarting to install an update.

## Video backend and limits

| Capability | 0.1.0 behavior |
| --- | --- |
| Built-in export | AVI 1.0, Motion JPEG, optional interleaved PCM16 audio; no external engine |
| Optional MP4 | Explicitly selected FFmpeg executable; MPEG-4 Part 2 video and AAC audio, `yuv420p`, fast-start metadata |
| Output sizes | Height 360, 720 or 1080; aspect ratio 16:9, 9:16 or 1:1; even pixel dimensions |
| Frame rates | 12, 24 or 30 fps |
| Timeline | Up to 500 slides, 0.5–120 seconds each, one hour total |
| Intermediate size | AVI is limited to 2 GB, including MP4's intermediate AVI |
| Input limits | JPEG/PNG/WebP/BMP; still frame of animated sources; decoder allocation capped at 256 MiB, dimensions at 16,000 per side |
| Composition | Two-photo cache; oriented images scaled to at most 2048 per side; captions use embedded Ubuntu Light |
| Not supported | Video-clip import, HEIC/RAW, MP3, recorded narration, sound playback, 3D effects, pan/zoom, undo/redo, portable project bundles |

AVI can be played by many desktop media players; social and mobile services may require MP4. MPEG-4 Part 2 is not H.264 and is accepted by fewer sharing services. MP4 availability depends on the chosen FFmpeg build. Export temporarily needs disk space for AVI, MP4 when selected, and a final publication copy. Failed and cancelled jobs remove their temporary directory. A forced process termination may leave a temporary directory beside the selected output.

FFmpeg is **not bundled, installed, downloaded or executed during this development session**. Its license depends on how it was built; see [engine and asset notices](docs/THIRD_PARTY.md) before packaging one. No codecs, drivers or services are installed by SlideshowMaker.

## Build and package

Requires Rust 1.95 or newer and the Windows MSVC C++ build tools already installed. Dependencies are locked in `Cargo.lock`. The repository Cargo configuration limits jobs to two and uses a local `target` directory.

```powershell
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
powershell -NoProfile -File scripts/package.ps1
```

Packaging statically links the MSVC runtime and produces `dist/SlideshowMaker-v0.1.0.exe`, its `.sha256`, and license notices. It does not launch the application or publish anything. The app uses the standard Windows desktop graphics environment through eframe/OpenGL; actual driver/DPI and clean-machine compatibility still need manual validation. This session's PowerShell policy blocked running `.ps1` files; the release build was invoked directly through Cargo instead, without changing that policy. See the validation record for the exact commands and results.

For a safe, headless synthetic export (no UI, playback, or network):

```powershell
cargo run --locked --example render_fixture
python scripts/verify-fixture.py .test-output/synthetic.avi
```

The second command uses an **already installed** `opencv-python` decoder; it is optional and never installs packages. Repeated example runs choose a new AVI filename. Generated `.test-output` media is artificial and ignored by Git.

## Settings and updates

**Settings → About & updates** shows the installed version, **Check for updates**, and a persistent **Check for updates on startup** toggle (enabled by default). Disabling it makes no update request at the next launch. Settings are stored under `%APPDATA%/SlideshowMaker/settings.json`. Startup checks use a separate bounded worker and do not block editing. A nonexistent repository/release, rate limit or network failure is reported as an unavailable check, never as “up to date”; startup errors remain in Settings without a popup.

A newer stable Windows x64 release needs `SlideshowMaker-vVERSION.exe` and `SlideshowMaker-vVERSION.exe.sha256` assets on the canonical repository. **Download and install** stages the official executable, checks its declared length, SHA-256 and x64 PE header, then offers **Restart to install verified update**. A hidden PowerShell helper waits for the app to exit, replaces the portable executable, restarts it, and rolls back file-operation/start failures. Project/media/settings paths are preserved. Read-only installations fail without elevation. The portable executable keeps its installed filename during updates. Checksum assets detect corrupt downloads; they are not a separate code-signing trust root.

The updater is implemented but **network and replacement behavior were not executed in this session**. A packaged Windows update/rollback test remains required before publication. See [manual checks](docs/VALIDATION.md).

## Screenshots

Real application and Settings screenshots are pending. This session explicitly prohibited GUI launches and screen capture. No mock screenshot or broken image link is substituted. Capture and add accurate English-captioned screenshots after the authorized interactive validation.

See [architecture](docs/ARCHITECTURE.md), [validation](docs/VALIDATION.md), and [initial release draft](docs/RELEASE_NOTES.md). Source code is MIT licensed; embedded fonts and dependencies retain their licenses.
