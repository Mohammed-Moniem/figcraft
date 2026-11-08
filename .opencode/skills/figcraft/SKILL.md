---
name: figcraft
description: Create, inspect, edit and export FigCraft interface designs with its command API, full CLI or MCP server. Use for frames, auto layout, components, variables, prototypes and design-file automation, including attached desktop work.
license: MIT OR Apache-2.0
---

# FigCraft

Use the installed application's capabilities, rather than assuming a release's command count or
copying IDs from a different document. This directory is a project-local OpenCode skill with ID
`figcraft`; invoke it with `@figcraft`. Supporting paths below are relative to this directory.
Use an available full CLI or configured MCP tools. Optional scripts require Python3.9+.

1. Establish the requested document, output folder and mode. A fresh headless MCP session is a
   separate untitled document; it does not edit an open desktop tab. For attached work read
   [connection modes](references/modes.md), inspect the app/document first, and keep that owner.
2. Discover the available API. With MCP use `list_commands` with a narrow `filter`, and the
   host's live tool schemas. Without configured MCP, use `figcraft-cli --help` and
   `figcraft-cli commands --json --filter node.create`. Optional `python3 scripts/catalog.py probe` reports
   the installed full CLI version and current command/tool counts without modifying configuration.
   `python3 scripts/catalog.py commands --filter layout` prints the current catalog. Both accept
   `--cli /absolute/path/figcraft-cli`. They do not attach, install, or fetch releases.
3. Inspect before changing: `inspect_document` or `get_node` with a small depth. Most commands
   target explicit `ids`, falling back to selection; new layers change selection. Retain returned
   IDs, pass explicit parents and targets, and use `batch` references for dependent creations.
4. Build layout through `layout`, `gap`, `padding`, `mainAlign`, `crossAlign`, `sizingH` and
   `sizingV`. An auto-layout parent positions its children; changing their `x`/`y` is not a way
   to space them. Coordinates otherwise use the parent frame, with positive y downward. Verify
   `absoluteBounds` in page coordinates. See the [tested starter workflow](references/workflows.md).
5. Re-inspect affected layers after each meaningful change. Check bounds, fills, layout,
   component links, overrides and warnings. Use normalized opacity 0–1 and hex colors such as
   `#2463EB`; query the current parameter docs before other property edits. Unknown keys and
   failed commands are errors, not confirmation that the edit happened.
6. Save a native `.figcraft` document, then export images with the separate `export` tool or CLI
   export command. A save path is not an image-format selector. Inspect the result and actually
   open the exported PNG when visual review is possible; file existence alone is insufficient.
   Report the edited IDs, paths, warnings and validation that actually ran.

For components, variants, variables or prototypes, first filter `list_commands` for `component`,
`variable`, `style` or `proto`; read the returned params before writing. Create a small instance
or mode control and verify it before applying a large change. Do not assume imported formats
round-trip every feature: inspect import/export warnings and keep the native source.

Do not install MCP client configuration, change global OpenCode settings, start sharing, publish
plugins, or download/update binaries as a prerequisite for design work. Those are separate user
actions. An optional explicit `python3 scripts/catalog.py release` checks only public release
metadata; offline or rate-limit failures are reported, and nothing is downloaded or installed.
For release maintenance, regenerate the local probe/catalog using the exact release binary;
do not put static counts in these instructions. See [maintenance and limits](references/maintenance.md).
