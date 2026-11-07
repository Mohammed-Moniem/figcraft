# Local issue fixes, 7 November 2026

These changes build on the combined FigCraft source. They do not close the upstream issues or
establish platform-wide acceptance. The full issue audit is maintained separately from the
application source.

- [#2080](https://github.com/storytold/figcraft/issues/2080): a whole ellipse has four cubic
  segments with anchors at its top, right, bottom and left. Each ring contour has the same four
  segments with opposite winding for the hole. Partial arcs retain their existing construction.
  The standard cubic circle has a small approximation error; geometry and raster checks cover it.
- [#2045](https://github.com/storytold/figcraft/issues/2045): an experimental `.fig` export lists
  the chosen opening page first and uses its background in archive metadata. The export command
  takes that page from the current viewer, unless Set as opening page explicitly chose another.
  Both file.export and Save as .fig follow this path. All other pages retain their
  relative order. The editable document's order is unchanged, but reopening the exported file
  follows the exported order. Layer-budget assignment prioritises the active page too. This
  addresses the synthetic template/opening-page case, following the approach proposed in
  [#2079](https://github.com/storytold/figcraft/pull/2079). The reporter's Windows file and a fresh
  external Figma import have not been verified; broader `.fig` interoperability remains open.
- [#1319](https://github.com/storytold/figcraft/issues/1319): View > Selection outlines temporarily
  hides the selected layers' outlines, transform handles and size labels. The same checked menu
  action restores them. `view.selectionOutlines {on?}` exposes it through automation and the
  Actions palette. Selection, property editing, hit tests, undo, and text/vector editing remain
  available. The default is on. Outline opacity adjustment is not included.
- [#574](https://github.com/storytold/figcraft/issues/574) and
  [#1786](https://github.com/storytold/figcraft/issues/1786): exhausting the decoded-value budget
  reports the supported limit and explains that the file is too large to open. Malformed data
  retains its corruption error. The limits are unchanged: this improves diagnosis and does not
  make every large file importable.
- [#2023](https://github.com/storytold/figcraft/issues/2023): a canvas-owned drag keeps receiving
  finite pointer positions over either sidebar, including release over a layer row. A press
  starting in a sidebar cannot start a canvas gesture, and Escape still cancels the owned drag.
  The report's separate alignment concern has no precise reproduction and remains open.

The new regressions reproduced the ellipse anchors, opening-page order, missing selection-overlay
toggle and misleading size-limit errors before their corresponding fixes. Test logs and the
combined source identity accompany the local integration handoff.
