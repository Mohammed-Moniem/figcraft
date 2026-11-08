#!/usr/bin/env python3
"""Check that imported design formats are offered consistently without taking over defaults."""
import json
import plistlib
from pathlib import Path
import xml.etree.ElementTree as ET

root = Path(__file__).resolve().parent.parent
extensions = {"fig", "jam"}

plist = plistlib.loads((root / "packaging/macos/Info.plist.in").read_bytes())
types = [t for t in plist["UTImportedTypeDeclarations"]
         if extensions <= set(t.get("UTTypeTagSpecification", {}).get("public.filename-extension", []))]
assert len(types) == 1, "macOS must declare both imported extensions"
uti = types[0]["UTTypeIdentifier"]
handlers = [t for t in plist["CFBundleDocumentTypes"] if uti in t.get("LSItemContentTypes", [])]
assert len(handlers) == 1 and handlers[0]["CFBundleTypeRole"] == "Viewer"
assert handlers[0]["LSHandlerRank"] == "Alternate", "imports must not take over the default handler"

wix = ET.parse(root / "packaging/windows/figcraft.wxs")
rows = list(wix.iterfind(".//{*}RegistryValue"))
policy = json.loads((root / "packaging/windows/install-policy.json").read_text())
actual = sorted(["-1" if r.get("Root") == "HKMU" else r.get("Root"), r.get("Key"), r.get("Name", ""), r.get("Value", "")] for r in rows)
assert actual == sorted(policy["registry"]), "MSI registry entries and install-policy expectations must agree"
owners = set()
for extension in extensions:
    matches = [r for r in rows if r.get("Key") == f"Software\\Classes\\.{extension}\\OpenWithProgids"]
    assert len(matches) == 1, f"Windows .{extension} must be offered under Open with"
    owners.add(matches[0].get("Name"))
    assert any(r.get("Key") == "Software\\Classes\\Applications\\figcraft.exe\\SupportedTypes"
               and r.get("Name") == f".{extension}" for r in rows)
    assert not any(e.get("Id") == extension for e in wix.iterfind(".//{*}Extension")), "import must not claim a default Extension"
    assert not any(r.get("Key") == f"Software\\Classes\\.{extension}" and not r.get("Name") for r in rows), "import must not replace a default ProgId"
assert len(owners) == 1
owner = owners.pop()
commands = [r for r in rows if r.get("Key") == f"Software\\Classes\\{owner}\\shell\\open\\command"]
assert len(commands) == 1 and commands[0].get("Value") == '"[#FigcraftExe]" "%1"', "filenames must stay quoted"

mime = ET.parse(root / "packaging/linux/ai.storyteller.figcraft.mime.xml")
matches = [t for t in mime.iterfind(".//{*}mime-type")
           if {f"*.{e}" for e in extensions} <= {g.get("pattern") for g in t.findall("{*}glob")}]
assert len(matches) == 1, "Linux must associate both imported extensions with one MIME type"
media = matches[0].get("type")
desktop = dict(line.split("=", 1) for line in (root / "packaging/linux/ai.storyteller.figcraft.desktop").read_text().splitlines() if "=" in line)
assert media in desktop["MimeType"].split(";"), "desktop entry must offer that MIME type"
assert desktop["Exec"] == "figcraft %U"
appstream = ET.parse(root / "packaging/linux/ai.storyteller.figcraft.metainfo.xml.in")
assert media in {m.text for m in appstream.iterfind(".//mediatype")}, "AppStream must advertise the same import support"
print("Imported design file associations: consistent; no default takeover declared")
