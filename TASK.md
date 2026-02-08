# SlideshowMaker

Repository: `slideshow-maker`
Priority: 15 of 15 (user-requested launch order)
Product: SlideshowMaker
Purpose: Simple family photo videos with music and captions
Inspiration: https://smartshow-software.com/

Independent open-source implementation; no affiliation, competitor code or copied assets.

## Requested implementation

Build an approachable slideshow creator: import images and short clips where supported, reorder slides, choose per-slide and total duration, add captions/title cards and an audio track, select a few original tasteful templates/transitions, preview a timeline and export a normal shareable video. Support landscape/portrait layouts, image orientation and fit/fill without accidental clipping.

Implement real timeline/compositing/export code with clear backend support. Prefer Rust and Windows media APIs or a deliberately packaged compatible media engine. Tests can compose synthetic frames and validate timing/output files without opening media or playing sound. Do not open user photo folders, capture screens, play audio, launch applications or install codecs. Keep the initial feature set coherent, provide graceful cancellation/progress and preserve all source media. Use only original or properly licensed template/assets/music, and record media-engine licensing/packaging requirements.

## Packaging and publication target

Windows standalone asset: `SlideshowMaker-vVERSION.exe` (for the initial release, `SlideshowMaker-v0.1.0.exe`). Append architecture only when distributing multiple executable architectures in the same release.
Planned canonical repository: https://github.com/siarheisinelnikau/slideshow-maker (not yet created or published; keep availability/error reporting truthful).

## Working agreement

Follow AGENTS.md. Complete as much real implementation and safe noninteractive verification as possible in this unattended code-only session. Manual UI, device and hardware tests will happen later. Record required external components and their verification status. Keep changes committed locally in meaningful increments.

