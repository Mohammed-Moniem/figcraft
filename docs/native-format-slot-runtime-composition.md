# Native format, slot properties and runtime composition

Status: **IN_PROGRESS**, 8 November 2026. This local composition follows the point alignment and
point-radius work. It does not complete the issue inventory or change product readiness estimates.

## Changes and bounded evidence

- Experimental native Motion interchange retains the reviewed inward XY Position and positive
  uniform Scale subset, and outward identity-linear rectangle XY Position subset. The producer
  checked the synthetic outward file in another application. General animation interchange, outward
  Scale, other shapes, timeline settings and other platforms remain separate. See
  [format details and limits](file-format-parity.md).
- Inherited instance-owned slots apply text properties using the binding's defining component,
  rather than pairing source-hop count with enclosing-instance depth (#2124). Direct slots,
  independent outer/inner property namespaces, explicit text overrides, two outer copies and
  public Undo/Redo are covered by the same seven controls before and after the repair.
- Export work accounting includes explicit sharp-zero settings when a vector has a positive
  physical layer-radius fallback (#2114). A bounded original control fails before the correction;
  ordinary zero-layer-radius and small fallback vectors remain admitted. Editable zero-radius
  wire settings and the whole-document roundtrip comparison remain checked. The bounded exporter
  explicitly warns when derived rounded-vector fill outlines are omitted; the compatibility test
  requires that exact sole warning. The earlier seven-failure composition run is retained as
  unqualified, followed by a corrected original run with exactly six expected product failures.
- A native canvas drag belongs to the document in which it began. Switching or closing that
  document retires the gesture before another document's first layout can pan or move it.
- Browser appearance preferences store both finite, bounded panel widths. Narrow-window fitting
  does not replace the saved widths, so widening or reloading can restore them.

## Validation

All 38 selected composition controls pass on the repaired isolated source; its corrected original
source has 32 passes and six expected failures. Separately, the drag/panel-width originals have
seven expected failures and the repaired related suites have 15 passes. Exact source maps, raw
logs and original/repaired executable hashes are retained locally.

The combined source also passed full CI on macOS ARM64: 6,977 tests, no failures and 27 existing
ignored tests, with formatting, strict Clippy, layering, assets, WASM checks, plugin builds and the
CLI smoke workflow. All 160 required regression controls were observed. The corrected catalog
check passed; earlier failed runs remain retained.

Matching retained CLI/native/browser artifacts and actual rebuilt native drag-owner, Point-row
and browser same-origin panel-width reload workflows remain pending. Previous builds' successful
runtime checks remain evidence for those exact builds.

The 18:43 UTC audit contains 200 open issues and 201 tracked records, including one issue closed
upstream. All bugs and feature requests remain in scope; this composition does not close them.
