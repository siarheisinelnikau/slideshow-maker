# Validation record

## Current Windows build — 2026-10-31

The initial implementation was code-only. The follow-up first attempted computer use, which the user stopped with Escape. Under the latest instruction **no computer use was invoked**: static design review uses bounded application-generated PNGs, while a separate native diagnostic exercises only its own window lifecycle. All data is synthetic. No audio was played, updates installed, user media opened, system settings changed, or remote Git operations performed.

| Check | Result |
| --- | --- |
| Formatting | Passed |
| Clippy on all targets with `-D warnings` | Passed |
| Rust tests | 14 passed: 9 core, 5 desktop/UI/raster regressions |
| Optimized Windows x64 build with static MSVC runtime | Passed |
| Executable/asset verification | Passed: x64 GUI PE, matching SHA-256, no imported OpenGL/Direct3D/DXGI or MSVC redistributable DLL |
| Real editor, Settings and export client-area captures | Passed at 1180×780, 100% scale |
| Empty editor capture at minimum logical size | Passed at 900×640, 125% scale (1125×800 PNG) |
| Capture process lifecycle | All four children rendered three settled frames and exited successfully, under an external 15-second limit |
| Native visible-idle / hidden / restored-idle gate | Passed on the prior CPU/GDI build; baseline report preserved below |
| Embedded icon resources | Passed; all eleven 32-bit 16–256 px images match the ICO source |
| Native window/title-bar and taskbar icons | Passed in a hidden, render-free probe; 16×16 and 32×32 at 100% scale |
| Normal interactive input, native file dialogs and subjective responsiveness | Not verified; computer use is currently excluded |
| MP4 engine, update installation/rollback, other-machine/DPI checks | Pending |

The current asset is `dist/SlideshowMaker-v0.1.0.exe`, 9,164,288 bytes. Its SHA-256 is:

```text
86117459632099cc8eb3ab2b8604be5bb600e35608a4f625ac0dbd784781948d
```

`dist` contains its checksum and license notices and is ignored by Git. Sources, lockfile, scripts, screenshots and the raw current runtime report are committed. No release was published.

## Rendering/load baseline before the icon-resource repair

The following report identifies the earlier measured executable and its own hash. It is not a new runtime-performance measurement of the icon-bearing build. A follow-up full runtime diagnostic ended before every phase completed; no replacement performance pass is claimed. The narrowly scoped hidden native-icon probe below completed successfully.

Verified baseline process: PID 4968, `E:\work\github\slideshow-maker\dist\SlideshowMaker-v0.1.0.exe`, optimized Windows x64 build. The pipeline is egui CPU rasterization → softbuffer Win32 GDI bitmap presentation. Pinned native source uses `CreateDIBSection` and `BitBlt`; the target executable has no graphics-device API imports. This does not disable Windows desktop composition.

The native diagnostic uses an isolated empty editor with no network or settings writes. It lets startup settle for two seconds, samples visible idle for five seconds, hides the test window for five seconds, restores it, lets it settle for two seconds, samples restored idle for five seconds, writes JSON and exits. An external supervisor applies a 26-second timeout to only that disposable child. No pointer/key injection or desktop capture occurs.

| State | Sample | UI layouts | Rasterizations | Present calls | CPU, one core | CPU, machine |
| --- | --- | --- | --- | --- | --- | --- |
| visible_idle | 5.001s | 0 | 0 | 0 | 0.00% | 0.00% |
| hidden | 5.000s | 0 | 0 | 0 | 0.00% | 0.00% |
| restored_idle | 5.000s | 0 | 0 | 0 | 0.00% | 0.00% |

The completed run made 4 total UI layouts, 2 rasterizations and 3 presentations across startup and restore; **none occurred in the three settled samples**. Restore did present a valid retained bitmap. CPU values are at `GetProcessTimes` counter resolution and do not claim mathematically zero activity. GPU telemetry and perceptual input latency are unavailable; no claim of fully verified responsiveness or absence of all compositor/driver stalls is made. See [raw runtime evidence](validation/runtime-gate.json).

Earlier runs received real cursor events (including one with 640 movements in five seconds). They were rejected as idle samples rather than treated as passes. This motivated scene comparison: hit-testing still runs on useful input, but unchanged shapes, textures, viewport and scale skip tessellation, rasterization and presentation. Native expose/restore can still blit the retained bitmap when required. No artificial sleep, priority change or FPS cap was introduced.

## Application icon repair

The original executable had no primary icon group and the native window/taskbar had no explicit icon. The repair adds original SVG/PNG artwork, a standard transparent ICO with eleven 32-bit sizes (16, 20, 24, 32, 40, 48, 64, 96, 128, 192 and 256 px), Windows icon/version resources, and DPI-aware native small/large icon loading from the same embedded resource.

`scripts/verify-release.py` parses the actual packaged PE resource tree and compares every RT_ICON payload with the source ICO. `scripts/verify-native-icons.py` starts the real executable in a separate hidden probe with a three-second internal/seven-second external limit. It reads the probe's own WM_GETICON handles, validates their bitmap dimensions and releases the inspection bitmaps; there are zero UI layouts/rasterizations/presentations and no computer use. The process exited successfully with a 16×16 title-bar icon and 32×32 taskbar icon at 100% scale. See [native icon evidence](validation/native-icons.json) for the matching executable hash and PID. Explorer/taskbar visual inspection and other actual desktop DPI settings were not exercised.

The small artwork was visually reviewed on light and dark backgrounds in [the icon preview](../assets/icon-preview.png). Regenerate assets with `python scripts/generate-icon.py`; verify the packaged result with `python scripts/verify-native-icons.py dist/SlideshowMaker-v0.1.0.exe`. No Windows icon-cache or system setting was changed.

## Static visual review

The distributable supports a developer-only offscreen capture path, parsed before ordinary startup. It uses the same UI construction, fonts, design tokens, compositor preview and CPU rasterizer, with no native window, network request, normal settings read/write or audio playback. The image is the application's real **client area**, not a mockup, desktop screenshot, title bar or native dialog. Each capture has an internal eight-frame/eight-second limit and an external 15-second limit.

Reviewed PNGs: [editor](screenshots/editor.png), [Settings](screenshots/settings.png), [export](screenshots/export.png), [compact empty state at 125%](screenshots/empty-compact.png). The adjacent `.png.json` files record dimensions, scale, frame count and elapsed time. All four images were decoded and visually inspected with a local image viewer.

Corrections found through these captures: timeline cards inheriting horizontal flow, unsupported arrow glyphs, double-applied scale at 125%, empty-state cursor advancement, export-dialog clipping, and an upstream rectangle rasterizer texture overrun at fractional photo scales. The shared renderer now uses the safe triangle path while keeping caching/other optimizations. A regression verifies the scaled image center and last edge pixel.

The UI now separates Slide, Story and Music work, puts export choices in one deliberate dialog, keeps New/Open/Save-as in Project, and uses a coherent slate/mint palette, readable type sizes and spacing. This is an editing workspace; a tray-only interface would not serve its central task.

## Reproduction

```powershell
cargo fmt --all -- --check
cargo test --offline --locked --all-targets
cargo clippy --offline --locked --all-targets -- -D warnings
cargo --config 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]' build --offline --locked --release --target x86_64-pc-windows-msvc --jobs 2 --target-dir target
python scripts/create-gui-fixtures.py
python scripts/capture-review.py dist/SlideshowMaker-v0.1.0.exe
python scripts/verify-runtime.py dist/SlideshowMaker-v0.1.0.exe
python scripts/verify-release.py dist/SlideshowMaker-v0.1.0.exe
```

Fixture creation/capture validation uses an already installed Pillow; it never installs dependencies. The runtime supervisor uses only Python's standard library. All Cargo builds use at most two jobs and a repository-local target directory.

## Earlier headless export evidence

The built-in exporter produced a 2.5-second 640×360, 12 fps AVI using two generated images, a title, transitions and a generated PCM16 tone. Independent OpenCV/FFmpeg decoding read all 30 MJPEG frames; a separate RIFF parser validated container lengths and 40,000 PCM audio bytes, without playback. Reproduce with `cargo run --locked --example render_fixture` and `python scripts/verify-fixture.py .test-output/synthetic.avi`. Repeated fixture runs preserve existing AVI files.

The initial system-temp storage/export tests hit an OS error 5 rename restriction. Moving fixtures into ignored `.test-output` resolved it. Incremental-cache hard-link warnings caused Cargo to copy files; checks still passed. PowerShell policy blocked direct execution of `scripts/package.ps1`; its syntax was parsed without execution, and packaging was performed by direct Cargo build/file copy/checksum commands without changing policy. GitHub CI is prepared but has not run remotely.

## Remaining interactive/release checks

- Exercise native import/open/save dialogs, caption entry, keyboard navigation, selection/reordering/deletion, seeking/playback and unsaved-close prompts after authorization and a clean rendering gate. Verify subjective responsiveness with the user; static PNGs and idle samples do not establish it.
- Check target-machine DPI changes, maximize/minimize/restore and clean-machine behavior. The current bounded native check covers hide/show and retained-frame restoration, not every window-manager case.
- Test representative EXIF orientations, transparency, long/Unicode paths, moved or read-only media and recovery in the native UI.
- Open AVI in intended players and verify audible music timing, fades and loop boundaries. Cancel during full UI exports and low-disk/write-denial failures. Core no-clobber/cancellation and file-format behavior are covered by tests.
- Supply a trusted compatible FFmpeg engine and verify real MP4 output and expected encoder errors. No engine was installed or invoked here.
- Verify persistent startup-update opt-out, manual API failures and rate limits, and signed/checksummed official release assets. On a disposable installation, exercise explicit download, restart replacement and rollback; preserve source hashes and settings.
- Audit final-target transitive notices and review the initial release draft before any separately authorized publication.
