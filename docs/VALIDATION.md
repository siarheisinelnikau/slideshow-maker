# Validation record and manual checklist

This session is restricted to code, compilation and bounded headless tests on generated data. No application windows, native dialogs, real photo folders, audio devices, GUI screenshots, updater installation, or FFmpeg processes were opened. All Cargo invocations use at most two jobs and repository-local target output.

## Automated checks

Results from this session (2026-10-31):

| Check | Result |
| --- | --- |
| Offline dependency resolution and Cargo lockfile | Passed |
| `cargo check --offline --all-targets` | Passed |
| Formatting | Passed |
| Clippy on all targets with `-D warnings` | Passed |
| Rust tests on all targets | 9 passed, 0 failed |
| Synthetic 2.5-second video export | Passed; generated 30 MJPEG frames plus PCM16 soundtrack |
| Independent OpenCV/FFmpeg decoding | All 30 frames decoded at 640×360, 12 fps; RIFF lengths and 40,000 PCM bytes validated |
| Documentation relative links | Passed; no missing local targets |
| PowerShell package/helper syntax parsing | Passed without executing either script |
| `powershell -NoProfile -NonInteractive -File scripts/package.ps1` | Blocked by `AuthorizationManager check failed`; execution policy was not modified |
| Direct Windows x64 release build with static MSVC runtime | Passed; optimized build completed in 7m 03s |
| Packaged asset PE/checksum inspection | Passed; Windows x64 GUI executable, matching SHA-256, no imported MSVC redistributable DLL |
| GitHub CI workflow | Prepared locally; not run remotely |
| GUI, playback, FFmpeg executable, update network/install | Deferred by session restrictions or unavailable engine |

The first two storage/export tests failed when using the system temporary directory: this managed environment denied temporary-file rename there (`Access is denied`, OS error 5). Fixtures were moved to a temporary subdirectory of this repository's ignored `.test-output`; all tests then passed. The environment also emitted an incremental-cache hard-link warning and used copies; it did not prevent builds or tests.

Reproduce with:

```powershell
cargo fmt --all -- --check
cargo check --offline --locked --all-targets
cargo test --offline --locked --all-targets
cargo clippy --offline --locked --all-targets -- -D warnings
cargo run --offline --locked --example render_fixture
python scripts/verify-fixture.py .test-output/synthetic.avi
powershell -NoProfile -File scripts/package.ps1
python scripts/verify-release.py dist/SlideshowMaker-v0.1.0.exe
```

The direct release-build command used after the blocked script invocation is:

```powershell
cargo --config 'target.x86_64-pc-windows-msvc.rustflags=["-C","target-feature=+crt-static"]' build --offline --locked --release --target x86_64-pc-windows-msvc --jobs 2 --target-dir target
```

Packaging used direct repository-local file copies and `Get-FileHash` after the blocked script invocation; no execution-policy setting was changed. The application executable was not launched. The final local asset is `dist/SlideshowMaker-v0.1.0.exe`, 10,868,736 bytes. Its SHA-256 is:

```text
830a5abe6a336c9364fee6407c5d346071412256754d777b38d3e3f00e8719f3
```

The `.exe.sha256`, MIT license, font notices, and dependency declaration inventory are beside it in `dist`. Build/output directories are intentionally ignored by Git; source, lockfile, scripts, and this validation record are committed. No repository, PR or release was published.

The Rust tests exercise timing/rescaling, validation, Fit/Fill compositing and transitions, project round trips, AVI RIFF/index structure, per-frame audio sample counts, decodable JPEG frames, cancellation/no-clobber, semantic release handling, and checksum corruption. The fixture builds a 2.5-second 640×360, 12 fps slideshow with two generated photos, a title, transitions and a generated PCM16 tone. The Python verifier uses an independent installed OpenCV/FFmpeg decoder to decode every frame and separately parses the PCM chunks without playing them.

## Deferred manual checks

- Launch the packaged EXE on supported Windows x64 machines, including a clean machine, and verify OpenGL availability, scaling at 100/150/200% DPI, smaller windows, text and control layout.
- Add representative JPEGs with EXIF rotation, PNG alpha, WebP and BMP. Compare preview and exported orientation, Fit edges, deliberate Fill crop, rotations, colors and captions. Replace/move a source and test relinking/error recovery.
- Import, select, reorder and delete slides. Adjust total and individual durations. Exercise silent playback, seeking during a render, last-frame behavior and all transitions in portrait/landscape/square.
- Save/reopen/save-as projects, including Unicode and long Windows paths, missing/readonly media, invalid JSON, unsupported versions and unsaved-close warnings. Keep independent copies of valuable media.
- Export AVI to target players and inspect video/audio duration, timing, volume, loop boundaries and fades. Cancel during rendering, FFmpeg conversion and finalization. Verify failed output is absent and existing files remain byte-identical.
- Select a trusted compatible FFmpeg executable, export MP4, and verify MPEG-4/AAC decoding in intended sharing destinations. Test missing binary, bad binary, absent encoder, low disk space, write denial and the 2 GB intermediate limit.
- Measure visible idle CPU, playback responsiveness and background export with release builds on representative hardware; no CPU/latency claim is made from compilation alone.
- Check Settings startup-update persistence: enabled starts a nonblocking public check; disabled makes zero startup requests; manual checking remains available. Test unpublished repository/404, offline, timeout, API rate limit, older/equal/prerelease/incompatible assets.
- On a disposable portable installation only, test checksum failure, truncated/wrong-architecture update, unwritable executable folder, explicit download progress, dirty-project restart prevention, successful restart replacement, and file-operation rollback. Confirm projects/settings/source hashes are preserved and no helper remains. Check `.update-error.txt` on a failed helper operation.
- Capture real main-window and Settings screenshots and add them to README with valid relative links and English captions.
- Audit target-specific transitive licenses and font notices, test the final asset and checksum, and review the release draft before any separately authorized publication.
