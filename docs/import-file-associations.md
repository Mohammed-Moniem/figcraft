# Imported design files in Open with

The macOS bundle declares a Viewer handler of Alternate rank for `.fig` and `.jam`. Windows
installer metadata offers both extensions through OpenWithProgids and SupportedTypes, using a
quoted filename argument. Linux's shared MIME database, desktop entry and AppStream metadata
advertise the same import MIME type. These declarations offer FigCraft without setting a new
default handler for imported documents.

`python3 packaging/file-associations-test.py` checks consistency across the three metadata sets,
the Windows registry policy, quoted arguments and the absence of a Windows default-extension
claim. The packaging identity guard is restricted to the exact interoperability MIME identifier
in Linux fields; unrelated product branding, tags and comments retain validation. The broader
packaging shell check still needs execution where the local command policy permits it.

Metadata checks do not establish installed behavior. A Windows MSI install/uninstall test and
Linux desktop MIME refresh/Open with test remain necessary on those systems. The importer and
experimental export format limitations are unchanged.
