# SlideshowMaker v0.2.0

This is the first published release of SlideshowMaker.

## Initial capabilities

- Build photo stories from images, title cards and captions, with saved editable projects, slide timing, framing, transitions and story templates.
- Add a local soundtrack, inspect a silent visual preview and export a new AVI with progress and cancellation.
- Use MP4 only with an explicitly selected trusted FFmpeg executable; FFmpeg is not bundled or downloaded, and original media/export destinations are preserved.

## Localization and appearance

SlideshowMaker now offers six interface languages and a choice of light or dark appearance.

## Added

- Choose English, Russian, French, German, Spanish or Italian in Settings. Language names appear in their native spelling.
- Switch Light / Dark in the same Appearance section. Language and appearance apply without restarting and save automatically.
- Read translated editor controls, help, export options, confirmation dialogs, native file-picker filters, progress and update messages. Existing status messages follow the current language.
- Start new stories and title cards with localized editable text. Existing story names, captions, media paths and settings are preserved.

## Improved

- Wrap longer translated actions and provide wider timeline cards and scrollable Settings.
- Apply the selected palette to surfaces, text, inputs, selections, preview surroundings and dialogs.
- Validate catalog completeness, message variables, safe insertion of file paths, and migration of older settings with focused tests.

## Downloads and verification

Windows x64 portable executable: `SlideshowMaker-v0.2.0.exe`, accompanied by `SlideshowMaker-v0.2.0.exe.sha256` and license notices. AVI export is built in. Optional MP4 export still requires a user-selected trusted FFmpeg executable.

Verified locally: formatting, all-target compilation, strict Clippy, 25 distinct library/editor/catalog tests, an optimized Windows x64 build with the static MSVC runtime, SHA-256/PE checks, and all eleven embedded application-icon sizes. The executable is 9,572,864 bytes and imports no OpenGL/Direct3D/DXGI or MSVC redistributable DLL.

Current source-only instructions prohibit launching the application; revised light/dark and translated screenshots, native interaction and new runtime idle/hidden measurements remain unverified. The README labels retained 0.1.0 captures and performance evidence as historical. Existing media-format, silent-preview and update-installation limitations remain documented there.
