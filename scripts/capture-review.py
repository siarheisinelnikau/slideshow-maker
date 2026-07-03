"""Run bounded, offscreen captures from the actual application executable."""
import json
import subprocess
import sys
from pathlib import Path

from PIL import Image

root = Path(__file__).resolve().parent.parent
exe = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else root / "dist" / "SlideshowMaker-v0.1.0.exe"
project = root / ".test-output" / "gui-check" / "sample.slideshow.json"
output = root / "docs" / "screenshots"
scenes = [("editor", "editor.png", "1180x780", "1", True),
          ("settings", "settings.png", "1180x780", "1", True),
          ("export", "export.png", "1180x780", "1", True),
          ("music", "music.png", "1180x780", "1", True),
          ("editor", "empty-compact.png", "900x640", "1.25", False)]
for screen, name, size, scale, populated in scenes:
    target = output / name
    command = [str(exe), "--screenshot", str(target), "--screen", screen, "--size", size, "--scale", scale]
    if populated:
        selected_project = project.with_name("music.slideshow.json") if screen == "music" else project
        command += ["--project", str(selected_project)]
    result = subprocess.run(command, cwd=root, timeout=15, capture_output=True,
                            creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0)
    if result.returncode:
        raise RuntimeError(f"Capture {name} failed ({result.returncode}): {result.stderr.decode(errors='replace')}")
    metadata = json.loads(target.with_suffix(".png.json").read_text())
    with Image.open(target) as image:
        image.load()
        assert list(image.size) == metadata["physical_size"]
        assert image.format == "PNG"
    assert metadata["frames"] <= 8
    print(f"PASS: {name}, {metadata['physical_size']}, {metadata['frames']} frames, process exited")
