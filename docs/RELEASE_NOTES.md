# SlideshowMaker 0.1.0 — initial release draft

Local preparation only. There is no preceding published release and no release was published by this session.

## Added

- Create photo stories from JPEG, PNG, WebP and BMP images and original title cards.
- Reorder slides, add captions, rotate photos, keep whole photos with Fit or deliberately crop with Fill.
- Choose landscape, portrait or square video and three original color templates, with cuts, dissolves and fades through the background.
- Set slide durations individually or proportionally fit the full story to a chosen total duration.
- Add PCM16 WAV music with volume, optional repetition, and export fades.
- Scrub and play a silent preview, save/reopen editable projects, and relink moved photos.
- Export MJPEG/PCM AVI directly, with progress, cancellation and protection against overwriting existing videos. Optional MP4 export uses a user-selected compatible FFmpeg executable.
- Access the installed version, manual release checks, startup-check opt-out, and checksum-verified portable update download/restart in Settings.
- Use a clearer workspace with Slide, Story and Music controls, a separate export dialog, consistent typography and a single mint accent.
- Run a CPU/GDI Windows interface that waits for input and stops layout, rasterization and presentation in the measured idle and hidden states.

## Fixed during local verification

- Prevent a software-renderer texture overrun when a photo is displayed at a fractional scale.
- Preserve the staged executable during an explicit update restart and discard it during ordinary closing.
- Reset the selected slide in a new project and move the playhead with reordered slides.
- Correct 125% capture scaling, timeline card flow and export-dialog clipping found in real application-generated PNGs.

## Platform and requirements

Planned asset: `SlideshowMaker-v0.1.0.exe` (Windows x64), accompanied by `SlideshowMaker-v0.1.0.exe.sha256` and license notices. AVI needs no media-engine installation. MP4 requires the user's own trusted FFmpeg build with MPEG-4 Part 2 and AAC encoders. FFmpeg is not included.

## Known limitations and release gate

Application-generated editor/Settings/export PNGs and bounded idle/hidden/restore measurements are complete. Interactive Windows workflows, subjective responsiveness, additional target-player compatibility, FFmpeg conversion, and update installation/rollback remain pending. Preview is silent; video-clip import, MP3, HEIC, undo/redo and media bundling are not implemented. AVI and MP4's intermediate AVI are limited to 2 GB. No signing certificate or installer is included.
