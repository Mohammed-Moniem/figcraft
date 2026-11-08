#!/usr/bin/env bash
# App-specific checks retained alongside shared packaging lint.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

bash packaging/server/install-test.sh
if grep -rniE 'photoshop|psd|photocraft|vectorcraft|illustrator|adobe|figma' packaging/ .github/ \
    --exclude=packaging-lint.yml --exclude=check-app-specific.sh --exclude=Info.plist.in \
    --exclude=ai.storyteller.figcraft.mime.xml --exclude=ai.storyteller.figcraft.metainfo.xml.in \
    --exclude=ai.storyteller.figcraft.desktop; then
  echo "::error::package metadata mentions another product (see above)" >&2
  exit 1
fi

python3 - <<'PYEOF'
import plistlib, re
import xml.etree.ElementTree as ET
from pathlib import Path
raw = Path("packaging/macos/Info.plist.in").read_bytes()
plist = plistlib.loads(raw)
allowed = {"com.figma.document", "Figma Document", "application/x-figma"}
def verify(value, permitted=allowed):
    if isinstance(value, dict):
        for key, item in value.items(): verify(key, permitted); verify(item, permitted)
    elif isinstance(value, list):
        for item in value: verify(item, permitted)
    elif isinstance(value, str) and re.search(r"photoshop|psd|photocraft|vectorcraft|illustrator|adobe|figma", value, re.I):
        assert value in permitted, f"Unexpected foreign product metadata: {value}"
verify(plist)
# Format identifiers describe interoperability, not product branding. Only the exact MIME value
# is allowed in these Linux metadata fields; every other field still receives the same guard.
for name in ("ai.storyteller.figcraft.mime.xml", "ai.storyteller.figcraft.metainfo.xml.in"):
    parser = ET.XMLParser(target=ET.TreeBuilder(insert_comments=True))
    for item in ET.parse("packaging/linux/" + name, parser=parser).iter():
        verify(item.tag, set())
        for key, value in item.attrib.items():
            verify(key, set())
            mime_type = name.endswith(".mime.xml") and item.tag.endswith("}mime-type") and key == "type"
            verify(value, {"application/x-figma"} if mime_type else set())
        media_text = name.endswith(".metainfo.xml.in") and item.tag == "mediatype"
        verify((item.text or "").strip(), {"application/x-figma"} if media_text else set())
        verify((item.tail or "").strip(), set())
for line in Path("packaging/linux/ai.storyteller.figcraft.desktop").read_text().splitlines():
    if "=" in line:
        key, value = line.split("=", 1)
        verify(key, set())
        verify(value.split(";") if key == "MimeType" else value, {"application/x-figma"} if key == "MimeType" else set())
    else:
        verify(line, set())
import yaml
manifests = []
for name in ("ai.storyteller.figcraft.yml", "ai.storyteller.figcraft.bundle.yml"):
    with open("packaging/linux/flatpak/" + name) as source:
        manifest = yaml.safe_load(source)
    assert manifest["id"] == "ai.storyteller.figcraft" and manifest["command"] == "figcraft", name
    manifests.append(manifest)
for key in ("runtime", "runtime-version", "sdk", "finish-args"):
    assert manifests[0][key] == manifests[1][key], f"Flatpak manifests disagree on {key}"
print("FigCraft package identity ok")
PYEOF

python3 packaging/file-associations-test.py
