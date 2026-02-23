"""Independently decode a synthetic AVI without opening a window or audio device.

Requires an already installed opencv-python. Does not download dependencies.
Usage: python scripts/verify-fixture.py .test-output/synthetic.avi
"""
import struct
import sys
from pathlib import Path

import cv2

path = Path(sys.argv[1])
data = path.read_bytes()
assert data[:4] == b"RIFF" and data[8:12] == b"AVI "
assert struct.unpack_from("<I", data, 4)[0] == len(data) - 8


def chunks(start, end):
    while start + 8 <= end:
        kind, length = struct.unpack_from("<4sI", data, start)
        payload = start + 8
        assert payload + length <= end, "Truncated chunk"
        yield kind, payload, length
        start = payload + length + (length & 1)
    assert start == end, "Misaligned chunk list"


top = list(chunks(12, len(data)))
movi = next((pos, size) for kind, pos, size in top if kind == b"LIST" and data[pos:pos + 4] == b"movi")
media = list(chunks(movi[0] + 4, sum(movi)))
video = [data[pos:pos + size] for kind, pos, size in media if kind == b"00dc"]
audio = b"".join(data[pos:pos + size] for kind, pos, size in media if kind == b"01wb")
assert len(video) == 30, f"Expected 30 frames, got {len(video)}"
assert len(audio) == 40000, f"Expected 2.5s PCM16 mono at 8 kHz, got {len(audio)} bytes"
assert any(audio[2000:10000]), "Soundtrack is unexpectedly silent"
assert audio[:2] == b"\0\0", "Soundtrack should start with a fade"

capture = cv2.VideoCapture(str(path), cv2.CAP_FFMPEG)
assert capture.isOpened(), "Independent FFmpeg decoder could not open AVI"
assert abs(capture.get(cv2.CAP_PROP_FPS) - 12.0) < 0.01
count = 0
while True:
    ok, frame = capture.read()
    if not ok:
        break
    assert frame.shape == (360, 640, 3), frame.shape
    count += 1
capture.release()
assert count == 30, f"Only {count} frames decoded"
print(f"PASS: {count} decoded MJPEG frames, 640x360 at 12 fps; 2.5s PCM16 soundtrack; valid RIFF lengths")
