"""Generate the original photo-and-play icon from shared vector geometry."""
from pathlib import Path
from PIL import Image, ImageDraw

root = Path(__file__).resolve().parent.parent
assets = root / "assets"
assets.mkdir(exist_ok=True)
sizes = [16, 20, 24, 32, 40, 48, 64, 96, 128, 192, 256]
geometry = [
    ("rect", (12, 12, 244, 244), 48, "#0f161e"),
    ("rect", (63, 44, 219, 174), 18, "#357d70"),
    ("rect", (33, 64, 200, 205), 16, "#79e0bf"),
    ("rect", (46, 77, 187, 179), 8, "#254958"),
    ("circle", (140, 90, 160, 110), 0, "#79e0bf"),
    ("polygon", [(46, 163), (85, 120), (119, 153), (146, 130), (187, 171), (187, 179), (46, 179)], 0, "#79e0bf"),
    ("circle", (151, 151, 239, 239), 0, "#0f161e"),
    ("circle", (158, 158, 232, 232), 0, "#79e0bf"),
    ("polygon", [(183, 175), (211, 195), (183, 215)], 0, "#0f161e"),
]
scale = 4
image = Image.new("RGBA", (256 * scale, 256 * scale))
draw = ImageDraw.Draw(image)
svg = ['<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">',
       '  <title>SlideshowMaker: photo cards and a play button</title>']
for kind, points, radius, fill in geometry:
    if kind == "polygon":
        draw.polygon([(x * scale, y * scale) for x, y in points], fill=fill)
        svg.append(f'  <polygon points="{" ".join(f"{x},{y}" for x, y in points)}" fill="{fill}"/>')
    else:
        x0, y0, x1, y1 = points
        bounds = tuple(value * scale for value in points)
        if kind == "rect":
            draw.rounded_rectangle(bounds, radius=radius * scale, fill=fill)
            svg.append(f'  <rect x="{x0}" y="{y0}" width="{x1-x0}" height="{y1-y0}" rx="{radius}" fill="{fill}"/>')
        else:
            draw.ellipse(bounds, fill=fill)
            svg.append(f'  <circle cx="{(x0+x1)/2:g}" cy="{(y0+y1)/2:g}" r="{(x1-x0)/2:g}" fill="{fill}"/>')
svg.append('</svg>')
(assets / "SlideshowMaker.svg").write_text("\n".join(svg) + "\n", encoding="utf-8")
image = image.resize((256, 256), Image.Resampling.LANCZOS)
image.save(assets / "SlideshowMaker.png")
image.save(assets / "SlideshowMaker.ico", format="ICO", sizes=[(size, size) for size in sizes], bitmap_format="bmp")
with Image.open(assets / "SlideshowMaker.ico") as icon:
    assert icon.ico.sizes() == {(size, size) for size in sizes}
    for size in sizes:
        frame = icon.ico.getimage((size, size)).convert("RGBA")
        assert frame.getextrema()[3] == (0, 255), f"Invalid transparency at {size}px"
sheet = Image.new("RGB", (560, 164), "#eff3f6")
canvas = ImageDraw.Draw(sheet)
for row, background in enumerate(["#eff3f6", "#0f161e"]):
    canvas.rectangle((0, row * 82, 560, (row + 1) * 82), fill=background)
    for col, size in enumerate([16, 24, 32, 48, 64]):
        frame = image.resize((size, size), Image.Resampling.LANCZOS)
        sheet.paste(frame, (col * 112 + (112 - size) // 2, row * 82 + (82 - size) // 2), frame)
sheet.save(assets / "icon-preview.png")
print(f"Generated original SVG/PNG and {len(sizes)} transparent ICO sizes")
