"""Record Cargo-resolved dependency license declarations without network access."""
import json
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--offline", "--locked", "--format-version", "1"], cwd=root,
))
lines = [
    "# Resolved dependency license declarations", "",
    "Generated from locked Cargo metadata. Includes platform-specific and build dependencies;",
    "not every listed package ships in the Windows executable. Declarations are an inventory,",
    "not a completed legal audit. Preserve upstream license texts and notices when required.", "",
    "| Package | Version | Declared license | Upstream |",
    "| --- | --- | --- | --- |",
]
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if package["name"] == "slideshow-maker":
        continue
    license_text = (package.get("license") or "Review upstream license file").replace("|", "\\|")
    repository = package.get("repository")
    upstream = f"[source]({repository})" if repository else "See registry package"
    lines.append(f"| {package['name']} | {package['version']} | {license_text} | {upstream} |")
(root / "docs" / "DEPENDENCY_LICENSES.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"Recorded {len(metadata['packages']) - 1} dependency license declarations.")
