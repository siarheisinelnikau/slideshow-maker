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

# Check the real PE icon resources, including every scale variant's payload.
resource_rva = struct.unpack_from("<I", data, optional + 128)[0]
assert resource_rva, "The EXE has no Windows resources"
resource_base = offset(resource_rva)


def resource_directory(relative):
    directory = resource_base + relative
    named, numbered = struct.unpack_from("<HH", data, directory + 12)
    entries = {}
    for number in range(named + numbered):
        name, child = struct.unpack_from("<II", data, directory + 16 + number * 8)
        if name & 0x80000000:
            continue
        if child & 0x80000000:
            entries[name] = resource_directory(child & 0x7FFFFFFF)
        else:
            rva, length = struct.unpack_from("<II", data, resource_base + child)
            start = offset(rva)
            entries[name] = data[start:start + length]
    return entries


resources = resource_directory(0)
assert 14 in resources and 1 in resources[14], "Missing primary icon group"
group = next(iter(resources[14][1].values()))
reserved, kind, count = struct.unpack_from("<HHH", group)
assert (reserved, kind) == (0, 1)
source = (Path(__file__).resolve().parent.parent / "assets" / "SlideshowMaker.ico").read_bytes()
assert struct.unpack_from("<HHH", source) == (0, 1, count)
icon_sizes = set()
for index in range(count):
    width, height, colors, pad, planes, bits, size, identifier = struct.unpack_from("<BBBBHHIH", group, 6 + index * 14)
    assert width == height and planes == 1 and bits == 32
    icon_sizes.add(width or 256)
    payload = next(iter(resources[3][identifier].values()))
    assert len(payload) == size
    source_size, source_start = struct.unpack_from("<II", source, 6 + index * 16 + 8)
    assert payload == source[source_start:source_start + source_size], "Embedded icon differs from the original asset"
assert icon_sizes == {16, 20, 24, 32, 40, 48, 64, 96, 128, 192, 256}, icon_sizes
print(f"PASS: Windows x64 GUI PE, {len(data):,} bytes, matching SHA-256; no imported MSVC redistributable or OpenGL/Direct3D DLL")
print(f"PASS: embedded primary application icon has {count} matching 32-bit frames: {sorted(icon_sizes)}")
print("Imported DLLs: " + ", ".join(imports))
