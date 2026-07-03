"""Create original, synthetic postcard images for local GUI verification."""
import math
import json
import struct
import wave
from pathlib import Path

from PIL import Image, ImageDraw

root = Path(__file__).resolve().parent.parent / ".test-output" / "gui-check"
root.mkdir(parents=True, exist_ok=True)
for name, size in [("mountain-lake.png", (1280, 800)), ("forest-sunrise.png", (800, 1200))]:
    w, h = size
    image = Image.new("RGB", size)
    draw = ImageDraw.Draw(image)
    for y in range(h):
        t = y / h
        draw.line((0, y, w, y), fill=(int(236 - 139 * t), int(192 - 66 * t), int(142 + 15 * t)))
    draw.ellipse((w * .68, h * .12, w * .83, h * .12 + w * .15), fill="#fff0c3")
    draw.polygon([(0, h * .60), (w * .25, h * .28), (w * .50, h * .61), (w * .73, h * .33), (w, h * .63)], fill="#728f98")
    draw.polygon([(0, h * .67), (w * .18, h * .47), (w * .45, h * .76), (w * .8, h * .46), (w, h * .71)], fill="#346574")
    draw.rectangle((0, h * .72, w, h), fill="#255965")
    for y in range(int(h * .75), h, max(1, h // 55)):
        inset = int((math.sin(y * .17) + 1) * w * .12)
        draw.line((inset, y, w - inset, y), fill="#427780", width=2)
    for x, scale in [(w * .07, .20), (w * .16, .13), (w * .91, .25)]:
        bottom = h * .93
        draw.rectangle((x - w * .006, bottom - h * scale, x + w * .006, bottom), fill="#173f45")
        for k in range(3):
            top = bottom - h * scale + k * h * scale * .17
            draw.polygon([(x, top), (x - w * scale * .25, top + h * scale * .57), (x + w * scale * .25, top + h * scale * .57)], fill="#193f47")
    image.save(root / name)
with wave.open(str(root / "gentle-test-tone.wav"), "wb") as audio:
    audio.setparams((1, 2, 8000, 0, "NONE", "not compressed"))
    audio.writeframes(b"".join(struct.pack("<h", int(math.sin(n * 440 * math.tau / 8000) * 3000)) for n in range(16000)))
slides = [
    {"media": {"Photo": str(root / "mountain-lake.png")}, "caption": "The places we remember", "seconds": 4.0, "fit": "Fit", "rotation": 0},
    {"media": {"Photo": str(root / "forest-sunrise.png")}, "caption": "Slow mornings, good company", "seconds": 4.0, "fit": "Fit", "rotation": 0},
    {"media": "Title", "caption": "Somewhere together", "seconds": 3.0, "fit": "Fit", "rotation": 0},
]
project = {"format_version": 1, "name": "A weekend to remember", "slides": slides,
           "layout": "Landscape", "theme": "Midnight", "transition": "Dissolve", "transition_seconds": 0.7,
           "audio": str(root / "gentle-test-tone.wav"), "audio_volume": 0.7, "audio_loop": True, "fps": 24, "height": 720}
(root / "sample.slideshow.json").write_text(json.dumps(project, indent=2), encoding="utf-8")
project["audio"] = str(Path(__file__).resolve().parent.parent / "tests/fixtures/audio/tone.mp3")
(root / "music.slideshow.json").write_text(json.dumps(project, indent=2), encoding="utf-8")
(root / "invalid-project.json").write_text('{"unrelated": true}', encoding="utf-8")
print(root)
