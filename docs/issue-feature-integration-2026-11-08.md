# Local feature integration, 8 November 2026

Status: **IN_PROGRESS**. This batch implements the following scoped changes; it does not complete
the 185 open bugs and feature requests, close upstream issues, or establish whole-product parity.

- [#286](https://github.com/storytold/figcraft/issues/286): optional focused numeric wheel editing,
  off by default, retaining unit mappings, clamps, mixed relative edits and gesture Undo. See
  [numeric-field behavior](ui-parity.md). Physical mouse/trackpad delivery remains separate acceptance.
- [#2036](https://github.com/storytold/figcraft/issues/2036): saved command shortcuts with key capture,
  conflict/cancel/replace, reset and menu/palette dispatch. See [custom shortcuts](custom-shortcuts.md).
  Native integration includes refreshing the macOS menu after idle command-based rebinding and
  preserving physical modifier state across recorder batches. Native Shift+Delete remains an
  independent Cut alternative when the X shortcut is changed or disabled.
- [#1687](https://github.com/storytold/figcraft/issues/1687): [DTCG token interchange](dtcg-tokens.md),
  with explicit supported types and native extensions for modes/expressions. Optional type metadata
  survives native save, variables JSON and clipboard operations without restricting variable pickers.
  Unsupported standard features are refused explicitly; this is not full DTCG conformance.
- [#1951](https://github.com/storytold/figcraft/issues/1951): the Delete → reconnect → round workflow
  repairs the matching fill loop, preserving its winding, holes, paint and neighbors. This backports
  [merged PR1991](https://github.com/storytold/figcraft/pull/1991) with an exact-coverage correction
  for malformed duplicate-index loops. Other malformed/nonmatching loops remain unchanged.
- [#1788](https://github.com/storytold/figcraft/issues/1788): Windows/Linux metadata registers `.fig`
  and `.jam` for Open with, with quoted filename handling and no explicit default takeover. See
  [file associations](import-file-associations.md). Installed-platform behavior remains pending.
- [#1741](https://github.com/storytold/figcraft/issues/1741): [Pencil visibility](pencil-background.md)
  follows the existing mostly opaque solid frame/page background policy. The rare extended curve
  tail is unreproduced and remains open; smoothing geometry is unchanged.
- [#1453](https://github.com/storytold/figcraft/issues/1453): the [starter icon library](starter-icons.md)
  preserves its original 24 outlines and appends 40 original editable icons. Tests cover generated
  components, native save/open, raster output and editable SVG/experimental `.fig` roundtrips.
  External-editor acceptance remains separate.
- [#1776](https://github.com/storytold/figcraft/issues/1776): an optional [UI accent preference](ui-accent.md)
  offers presets and a custom opaque color, with contrast roles and exact default Green palettes.
  This is a local-fork feature; earlier upstream proposals were closed unmerged.
- [#1988](https://github.com/storytold/figcraft/issues/1988) and
  [#2029](https://github.com/storytold/figcraft/issues/2029): [custom pixel-grid spacing](pixel-grid.md)
  applies to open, future and restored views. Free axes use the grid while exact object targets
  remain held. Shift keeps its page direction, including in rotated frames, and painting is bounded.
- [#1560](https://github.com/storytold/figcraft/issues/1560): a named hover button and context action
  [unlock the blocking layer or ancestor](canvas-unlock.md), preserving unrelated selection and Undo.
  Document, page, revision, viewport and presentation guards retire stale controls. Ordinary drags,
  clipping/masks, foreground ownership and read-only restrictions keep their existing semantics.
  A composed touch regression also keeps acquired navigation from blocking on or activating the
  later-painted unlock badge, including its final release. Seven synthetic controls and the full
  isolated UI suite pass; rebuilt browser/native touch acceptance remains pending.
- [#1492](https://github.com/storytold/figcraft/issues/1492): a reviewed three-file
  [progressive CMYK JPEG backport](jpeg-cmyk.md) lets valid finer-black sampling decode instead of
  blanket refusal, without the small-image upsampler panic. The published 0.5.15 source and licenses
  are retained, with a narrow local Zlib exception. No encoder dependency is added. Allocation and
  decoded-byte limits remain; inherited tiny-edge variance and broader color-profile fidelity remain
  explicit limits. The original approval patch is preserved beside a replayable count-normalized patch.
- [#1319](https://github.com/storytold/figcraft/issues/1319): [selection-outline opacity](selection-outline-opacity.md)
  is a saved 0–100% preference. Selection outlines, handles and labels fade together without changing
  artwork or document Undo. Existing hover, text, vector-edit and drop indicators keep their behavior.
- [#1368](https://github.com/storytold/figcraft/issues/1368): the optional
  [attach-only MCP relay](attach-cli.md) uses the running app's authenticated control channel.
  Its separately selected dependency graph omits the engine, document model, renderer and fonts.
  Local macOS ARM64 stripped release measurements produced a 610,707-byte ZIP versus 44,668,379
  bytes for the full CLI on the same feature snapshot. These measurements retain their original
  source identity; this combined source still needs its own runtime acceptance. The full CLI keeps
  headless support, and the relay requires an attached app and private credential file.
  A preceding combined build passed actual attached native MCP edits, Undo, confined save/refusal,
  compositor capture and honest disconnect behavior on macOS ARM64. That debug runtime check
  keeps its own source identity and does not update the earlier release-size measurement.
- [#1881](https://github.com/storytold/figcraft/issues/1881) and
  [#1882](https://github.com/storytold/figcraft/issues/1882): [joined-file history](joined-files.md)
  records successful joins and offers Open and local Forget on Home. Pending opens reuse their
  negotiated session; Forget also suppresses late reconnect records until an explicit new join.
  Native preferences retain atomic private-file writes and report failed saves. Browser storage
  selectively retains names and joined links, including their negotiated guest capability, without
  copying owner/server preferences. A preceding combined native build passed a real loopback
  host/two-guest join, private history persistence, process restart/rejoin, remote Board movement
  without recipient Undo, and local Forget through another restart. Actual Home pointer dispatch,
  browser joins and the newly merged source remain separate acceptance.
- [#1910](https://github.com/storytold/figcraft/issues/1910): [saved connector paths](jam-saved-connector-paths.md)
  preserve supported bounded alternating-axis control points from `.jam` files, including nested
  coordinates, strokes, caps and labels. Malformed or unsupported paths retain an explicit fallback
  warning. Saved endpoints keep their source points independently of the new native Board gap,
  preserving short outward runs and label midpoints. Positive axis-aligned endpoint transforms
  and free points are supported; other endpoint transforms regenerate with the reported fallback.
  Curved connectors also preserve exactly two valid ordered parent-space cubic handles, with
  bounded coordinates/tangents/transforms and saved-vector precedence. Optional axis semantics
  remain unclaimed. The curve follow-up passed 661 unit and four integration format tests with
  two existing ignored tests, strict affected Clippy and formatting. A subsequent matching native
  build preserved the original elbow and saved cubic through archive/reopen, with viewed app and
  export captures. Imported paths remain editable vectors rather than live Board attachments.
- [#1916](https://github.com/storytold/figcraft/issues/1916): native automatic Board connector
  centerlines start four page units outside the attachment border. Manual endpoints remain exact.
  Native file loading settles derived paths before the clean baseline; authoritative live and
  model snapshots retain their geometry. Drag/drop hit testing includes the gap and excludes
  the moving connector and its derived label. See [Board behavior](boards-parity.md).
- [#544](https://github.com/storytold/figcraft/issues/544): consecutive plain opaque rectangles
  with the same solid fill share bounded CPU coverage, removing fractional shared-edge seams
  while retaining outer antialiasing and document geometry. Masks, clips and stacking boundaries
  keep their context. Other shapes, paints, GPU rendering and edges across the batch limit remain
  outside this repair. See [scope and fallback](abutting-fill-seams.md).
- [#1138](https://github.com/storytold/figcraft/issues/1138): a repository-local OpenCode skill
  supplies original command/MCP workflow guidance and a bounded standard-library Python
  capability catalog. Local retained-CLI and MCP save/export/reopen workflows passed on their
  recorded earlier binary, with viewed output. The OpenCode host is unavailable locally, so
  actual host discovery, loading and provider integration remain unverified. Nothing is installed
  into global configuration.
- [#555](https://github.com/storytold/figcraft/issues/555): [keyboard/controller trigger phases](prototype-key-timing.md)
  add key down and key up with a bounded minimum continuous hold. Pending edges belong to their
  presenter, source interaction and input scope, and retire on focus/source/navigation changes.
  The authoring field also retires unfinished text when an external edit replaces its source.
  Existing immediate Key/Gamepad behavior is retained. Unsupported standard export phases warn
  and omit the interaction. Actual sustained physical input and matching app acceptance remain pending.
- [#2076](https://github.com/storytold/figcraft/issues/2076): a fresh Design document consumes its
  first known UI theme once. Native/imported/live/model/version/branch document backgrounds remain
  authoritative. Actual regression tests reproduced a separate startup provenance defect before
  this narrow repair; the reported Windows stale-chrome symptom remains unreproduced.
- [#1791](https://github.com/storytold/figcraft/issues/1791): plain active alignment lines use the
  existing page-adaptive canvas accent. Labelled spacing/distance lines, dashed extensions,
  measured boxes and point markers keep their red role; persistent overlays keep their styles.
  Four focused controls pass, including 32 actual pointer-drag cases and exact one-step Undo.
  The isolated full UI run exposed the separate name-save regression below and remains failed.
- [#2034](https://github.com/storytold/figcraft/issues/2034): five original saved-file sequences
  agree through actual CLI conversion, rendering, export and converted-JSON rendering, with all
  15 PNGs inspected and no production change. All 36 CLI integration tests and strict package
  quality checks pass. The unavailable original Windows 0.13.1 script remains unconfirmed;
  these scoped regression tests do not establish general layout or cross-platform parity.
- [#380](https://github.com/storytold/figcraft/issues/380): an optional saved
  [repeated-menu setting](menu-options.md), off by default, keeps the in-app main menu and nested
  checked options available for repeated changes. Ordinary actions, outside clicks and Escape
  dismiss it. The real Preferences checkbox, command and bounded browser record share the setting;
  native operating-system menus keep platform behavior. Original engine/UI/codec failures precede
  the implementation, and all new scoped controls pass with strict affected quality checks.
  The isolated broad run retained the two baseline name-save failures described below; the
  separately reviewed repair is merged for the next combined check. Existing pixel-grid tests now
  scroll the actual label and numeric control into view, preserving their original assertions.
- Browser versions now selectively keep outline opacity, grid spacing, UI accent, platform
  shortcut overrides, numeric-wheel behavior and the optional repeated-menu choice. The bounded versioned appearance record is
  separate from names and joined capabilities. Invalid or unreadable appearance records are
  preserved for the visit. Embedded hosts' temporary author names do not become durable on
  unrelated preference saves; explicit different name edits and later edits back remain usable.
  See [storage policy and limits](browser-preferences.md). A preceding actual browser artifact
  passed opacity/grid/accent/shortcut reload and scoped embedded-author controls, but exposed a
  missing independent number-wheel save request through both command and checkbox changes.
  The shared Boolean setter and normalized display-name command now request a save when their
  value changes, preserving pending writes and idempotent behavior. This repair has actual failing
  regressions and scoped passing checks. A matching rebuilt browser subsequently passed independent
  true/false/omitted command changes, actual checkbox changes and reload, plus normalized standalone
  names and scoped host/error controls. Broader UI testing found that the existing name field also
  saves immediately, causing a duplicate write when the engine requests another save. Its two
  once-only assertions failed before a narrow coalescing repair. The successful field's immediate
  full snapshot now consumes its fulfilled preference request; failed commands and separately
  pending plugin/live work retain their existing ownership. Both original once-only assertions and
  three added command/actual-field controls pass in the complete isolated UI suite: 1,542 passed,
  no failures and one existing ignored test, with strict UI quality checks. The save service retains
  its existing error policy; this establishes no stronger disk-durability guarantee.
- [#2015](https://github.com/storytold/figcraft/issues/2015): [prototype edge panning](prototype-edge-pan.md)
  keeps an owned connection drag moving toward an offscreen target while the pointer is held
  inside the unoccluded canvas edge. Camera motion alone does not edit the document. Escape,
  lost focus and changed document/page/source ownership retire it; a valid drop remains one Undo.
  Seven actual egui controls and the complete isolated UI suite pass, including stationary holds,
  zoom/direction controls and touch-style release. Matching app acceptance remains pending.
- [#1970](https://github.com/storytold/figcraft/issues/1970): scoped CJK inter-character
  justification preserves glyphs and advances while distributing eligible visual gaps. Mixed and
  nested-direction words, punctuation, KeepAll, whitespace and grapheme controls are covered.
  All 178 isolated text tests and strict affected quality checks pass with pinned fonts.
  This does not establish complete Unicode/CSS behavior or another editor's numerical parity.
- [#1563](https://github.com/storytold/figcraft/issues/1563): the text font-size handle changes
  a selected plain text layer's size from its local axis in Move mode, including rotation and zoom.
  Supported pixel line height scales proportionally; Auto and percentage values retain their mode.
  Existing box/parent-layout constraints, one-step Undo and Escape are preserved. Isolated engine
  and actual egui controls pass; matching app pointer and visual acceptance remains pending.
- [#978](https://github.com/storytold/figcraft/issues/978): [uniform live Boolean-result radius](boolean-radius.md)
  rounds an eligible computed result while retaining editable operands, holes, hit testing and
  raster/SVG/PDF output. Native and experimental `.fig` roundtrips retain the radius. Appearance
  paste keeps supported paints/effects when independent corners are unsupported. Penpot export
  explicitly warns that it omits result rounding. Seventeen final-source workflows, strict affected
  checks and layers pass; prior full affected and UI checks retain their earlier source identity.
  Independent corners, smoothing and arbitrary bounding-box handles remain outside this scope.

The combined source also reconciles the other local development line's frozen ownership, touch,
control-session, project-backup, pure-query, document teardown and capture-progress repairs. An
updater modal also owns its opening-frame keys before ordinary canvas/text and deferred Save
dispatch; its separate lifecycle tests protect uncommitted fields and editor resume.
The #1111 follow-up also adds bounded shared ingress admission and resource ownership across
queued/running native control, input, screenshots and replies. Its 4 MiB frame and conservative
64 MiB logical accounting envelope are not process-memory, document or export-size guarantees.
Absolute frame deadlines, cancellation, teardown and truthful terminal refusal/reconnect controls
are covered. The original line passed its full check and scoped native recovery acceptance;
their receipts retain that producer's identity. This merge needs its own combined validation.
Those checks retain their original scope;
the feature merge requires fresh combined validation. A first attempt passed formatting and
workspace Clippy but exposed three native Cut regressions. Six focused tests pass after the
routing repair. Follow-up integration checks found and repaired Preferences overflow, Actions accent
routing, exact grid display, accepted clamp reversal and newly focused arrow navigation. All31
selected accent/grid/unlock UI cases pass after those repairs and a static-text fixture correction.
An earlier complete attempt passed formatting and workspace Clippy, then recorded 5,935 named
test passes and four failures in Preferences fixtures. Those fixtures now scroll actual controls
into view and distinguish the sharing-server Reset button from Reset grid; all 30 Preferences
checks pass with their edit, cancellation, save and Undo assertions retained. This test-only repair
does not change production Preferences behavior. A subsequent attempt passed formatting and
workspace Clippy and recorded 5,968 named passes, one retained-render test failure and 25 ignored
tests. Its local socket tests passed after the test runner received loopback permission. The
retained-render assertion passed alone; a controlled font-cache invalidation reproduced its failure.
A test-only repair now re-primes only when the observed font epoch changes, keeps the stable-epoch
retention assertion, and separately requires a full redraw after invalidation. All 25 zoom-preview
tests pass on that repair. It does not establish the original failing interleaving conclusively.
A later combined source including joined-file history and saved elbows passed the complete
required check: 6,126 logged pass records, no failures and 27 ignored tests, including WASM,
layers, assets, docs, doctests, six example plugin builds and CLI smoke checks. The source stayed
unchanged throughout. The subsequently integrated Board gap, rectangle coverage, local OpenCode
skill, browser persistence and saved-endpoint follow-up passed a later full combined check with
6,181 logged named pass records, no failures and 27 ignored tests. A matching native build passed
the scoped command JPEG import/export/preview, supported token round trip, icon starter,
accent/grid and vector-region workflows with viewed output. Actual raw pointer reattachment at
100% and 400% kept the routed gap, border marker, one-step Undo and target-follow behavior through
save/reopen. A native fractional-camera rectangle scene passed 42 device-pixel seam samples.
These stages retain their exact earlier source and explicit limits; retained whole harness failures
include separate paint/camera-schema and imported-vector contract mistakes. The
saved-endpoint follow-up passed all 649 format tests (two existing ignored), including 15 saved
path tests, plus strict affected Clippy and formatting. Actual failing endpoint and transformed
normal cases preceded the repair; the common native four-unit gap remains unchanged.

The preceding browser artifact passed scoped actual UI, import, download/reopen and visual checks
on macOS ARM64 with software rendering. Reload exposed missing browser appearance persistence,
whose model and compilation repair is integrated above. That artifact predates the joined-file,
saved-elbow, Board gap, rectangle coverage and browser persistence merges.
The subsequent parent transport/touch reconciliation, theme authority, preference-save and
prototype-phase source passed scoped actual browser acceptance: 16 assertions, 200 commands and
28 viewed compositor captures, including a continuous key hold that opened an overlay before a
control query. This retains its exact source identity and establishes neither physical controller
nor native acceptance. The first whole-source attempt failed on local socket permission; the
unchanged loopback-enabled rerun passed formatting and strict workspace Clippy and recorded 6,120
named test passes, two name-save assertion failures and 25 ignored tests before stopping. Both
failures reproduced the duplicate write described above. The subsequent curve, guide, menu and
name-save source passed a complete combined check: 6,294 named passing records, no failures and
27 existing ignored tests. Matching macOS ARM64 native runs passed saved connectors and actual
menu/name input with viewed captures; the browser passed 30 scoped assertions and 36 captures,
with no page/console errors or external network attempts. These are recorded for that exact earlier
source. Its optimized WASM artifact still exceeded the historical 24 MiB budget; that gate remains
failed. The subsequently merged parent admission, edge-pan, CJK, font-handle and Boolean changes
require a fresh whole-source check and matching app acceptance; earlier results do not qualify them.

The earlier local issue milestone passed a full check with 5,831 named pass records, 27 ignored
tests, WASM checks and plugin build/smoke checks, plus scoped native synthetic visual acceptance
on macOS ARM64. That evidence predates this feature batch. Each feature has scoped regressions;
the new combined source still requires its own full check and native visual acceptance.
