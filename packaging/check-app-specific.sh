#!/usr/bin/env bash
# App-specific checks retained alongside shared packaging lint.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

bash packaging/server/install-test.sh
if grep -rniE 'photoshop|psd|photocraft|vectorcraft|illustrator|adobe|figma' packaging/ .github/ \
    --exclude=packaging-lint.yml --exclude=check-app-specific.sh --exclude=Info.plist.in; then
  echo "::error::package metadata mentions another product (see above)" >&2
  exit 1
fi

python3 - <<'PYEOF'
import plistlib, re
from pathlib import Path
raw = Path("packaging/macos/Info.plist.in").read_bytes()
plist = plistlib.loads(raw)
allowed = {"com.figma.document", "Figma Document", "application/x-figma"}
def verify(value):
    if isinstance(value, dict):
        for key, item in value.items(): verify(key); verify(item)
    elif isinstance(value, list):
        for item in value: verify(item)
    elif isinstance(value, str) and re.search(r"photoshop|psd|photocraft|vectorcraft|illustrator|adobe|figma", value, re.I):
        assert value in allowed, f"Unexpected foreign product metadata: {value}"
verify(plist)
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
