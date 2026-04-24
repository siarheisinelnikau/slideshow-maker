"""Inspect a packaged Windows executable and checksum without executing it."""
import hashlib
import struct
import sys
from pathlib import Path

path = Path(sys.argv[1])
data = path.read_bytes()
checksum = Path(str(path) + ".sha256").read_text(encoding="ascii").split()[0]
assert hashlib.sha256(data).hexdigest() == checksum.lower(), "Checksum mismatch"
assert data[:2] == b"MZ", "Missing DOS header"
pe = struct.unpack_from("<I", data, 60)[0]
assert data[pe:pe + 4] == b"PE\0\0", "Missing PE header"
machine, sections = struct.unpack_from("<HH", data, pe + 4)
assert machine == 0x8664, f"Expected Windows x64, got {machine:x}"
optional_size = struct.unpack_from("<H", data, pe + 20)[0]
optional = pe + 24
assert struct.unpack_from("<H", data, optional)[0] == 0x20B, "Expected PE32+"
assert struct.unpack_from("<H", data, optional + 68)[0] == 2, "Expected GUI subsystem"
section_table = optional + optional_size


def offset(rva):
    for number in range(sections):
        section = section_table + number * 40
        virtual_size, address, size, position = struct.unpack_from("<IIII", data, section + 8)
        if address <= rva < address + max(size, virtual_size):
            return position + rva - address
    raise ValueError(f"RVA {rva:x} is outside all sections")


imports_rva = struct.unpack_from("<I", data, optional + 120)[0]
imports = []
if imports_rva:
    entry = offset(imports_rva)
    while any(data[entry:entry + 20]):
        name_rva = struct.unpack_from("<I", data, entry + 12)[0]
        name_at = offset(name_rva)
        imports.append(data[name_at:data.index(b"\0", name_at)].decode("ascii"))
        entry += 20
assert not any(name.lower().startswith(("vcruntime", "msvcp")) for name in imports), imports
assert not any(name.lower().startswith(("opengl", "d3d", "dxgi")) for name in imports), imports
print(f"PASS: Windows x64 GUI PE, {len(data):,} bytes, matching SHA-256; no imported MSVC redistributable or OpenGL/Direct3D DLL")
print("Imported DLLs: " + ", ".join(imports))
