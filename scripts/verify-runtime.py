"""Bounded native diagnostic; no computer use, simulated input, or user data."""
import json
import hashlib
import math
import subprocess
import sys
import time
from pathlib import Path

root = Path(__file__).resolve().parent.parent
exe = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else root / "dist" / "SlideshowMaker-v0.2.0.exe"
report = root / ".test-output" / "gui-check" / f"runtime-gate-{time.time_ns()}.json"
report.parent.mkdir(parents=True, exist_ok=True)
process = subprocess.Popen([str(exe), "--diagnostic-report", str(report)], cwd=root,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                           creationflags=subprocess.CREATE_NO_WINDOW if sys.platform == "win32" else 0)
try:
    stdout, stderr = process.communicate(timeout=26)
except subprocess.TimeoutExpired:
    process.kill()  # Only this disposable diagnostic child, never a name match.
    process.communicate()
    raise RuntimeError("The isolated diagnostic exceeded its external deadline")
if not report.exists():
    raise RuntimeError(f"No diagnostic report ({process.returncode}): {stderr.decode(errors='replace')}")
data = json.loads(report.read_text())
assert data["pid"] == process.pid, "Stale report from another process"
data["binary_sha256"] = hashlib.sha256(exe.read_bytes()).hexdigest()
report.write_text(json.dumps(data, indent=2), encoding="utf-8")
(report.parent / "runtime-gate.json").write_text(json.dumps(data, indent=2), encoding="utf-8")
print(f"Report: {report}")
print(json.dumps(data, indent=2))
if process.returncode or not data["passed"]:
    raise RuntimeError("Rendering gate did not pass; inspect the recorded samples")
icons = data["native_icons"]
assert icons["present"], "The native title-bar/taskbar icons are missing"
for key, logical in [("small", 16), ("large", 32)]:
    expected = math.floor(logical * icons["scale"] + 0.5)
    assert icons[key] == [expected, expected], f"Unexpected {key} icon size: {icons}"
print("PASS: bounded native CPU/GDI diagnostic completed and exited")
print(f"PASS: native title-bar icon {icons['small']}, taskbar icon {icons['large']}")
