"""Inspect the executable's own hidden native window icons, then exit."""
import hashlib
import json
import math
import subprocess
import sys
import time
from pathlib import Path

root=Path(__file__).resolve().parent.parent
exe=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else root/'dist/SlideshowMaker-v0.1.0.exe'
report=root/'.test-output'/f'native-icons-{time.time_ns()}.json'
report.parent.mkdir(exist_ok=True)
result=subprocess.run([str(exe),'--verify-native-icons',str(report)],cwd=root,timeout=7,capture_output=True,creationflags=subprocess.CREATE_NO_WINDOW if sys.platform=='win32' else 0)
if result.returncode:
    raise RuntimeError(f"Native icon probe failed: {result.stderr.decode(errors='replace')}")
data=json.loads(report.read_text())
assert data['hidden_window'] and data['ui_frames']==0
icons=data['native_icons']
assert icons['present']
for name,logical in [('small',16),('large',32)]:
    size=math.floor(logical*icons['scale']+0.5)
    assert icons[name]==[size,size], icons
data['binary_sha256']=hashlib.sha256(exe.read_bytes()).hexdigest()
report.write_text(json.dumps(data,indent=2),encoding='utf-8')
(root/'.test-output/native-icons.json').write_text(json.dumps(data,indent=2),encoding='utf-8')
print(json.dumps(data,indent=2))
print('PASS: native title-bar/taskbar icons; hidden probe exited without rendering')
