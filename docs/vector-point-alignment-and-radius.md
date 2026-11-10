# Vector point alignment and imported rounding

Status: **IN_VALIDATION**. The isolated controls pass; fresh combined quality checks and matching
launched native/browser acceptance follow this local integration.

While editing a vector, the six alignment buttons align selected points to the selected-point
box. The distribution menu spaces three or more selected points evenly between the extremes.
An empty or single-point selection is unchanged; distribution of fewer than three points is
unchanged. Segment endpoints are counted once. Handles and point metadata travel with a point,
and one operation is one Undo step. A single rotated layer uses its existing point-box axes;
points edited across layers use page axes. Explicit layer targets retain layer alignment.

Native .fig vector import uses a positive layer corner radius only where a point has no saved
radius. An explicit zero keeps that point sharp. Layer and point values keep their respective
instance scales. The vector-data supplier remains authoritative, while an outline-only fallback
stays sharp. Experimental .fig export preserves explicit sharp points on a rounded vector.
Missing-point-settings warnings distinguish a positive layer fallback from absent or nonpositive
layer radii. This does not establish arbitrary-file or full-format round-trip fidelity.

Validation: identical tests on the original implementation recorded 6 passes and 23 failures;
the repaired implementation passed all 29 controls. Workspace formatting, strict all-target
Clippy for engine/UI/format, and layering passed in isolation. The wider 197-issue request remains
unfinished; other platforms and actual rebuilt application workflows require separate evidence.
