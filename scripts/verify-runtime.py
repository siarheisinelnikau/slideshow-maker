"""Bounded native diagnostic; no computer use, simulated input, or user data."""
import json
import subprocess
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
exe = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else root / "dist" / "SlideshowMaker-v0.1.0.exe"
report = root / ".test-output" / "gui-check" / "runtime-gate.json"
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
print(json.dumps(data, indent=2))
if process.returncode or not data["passed"]:
    raise RuntimeError("Rendering gate did not pass; inspect the recorded samples")
print("PASS: bounded native CPU/GDI diagnostic completed and exited")
