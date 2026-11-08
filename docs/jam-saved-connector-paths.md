# Saved connector paths in FigJam imports

Elbow connectors with valid saved internal runs and curved connectors with two valid saved cubic
handles keep those paths when a `.jam` or `.fig` file opens. Their saved parent coordinates, line
appearance and label midpoint are preserved. Imported
connectors remain editable vectors or frames with a line and label; they do not become native
board connectors that follow moved shapes.

Saved attached endpoints keep their finite relative positions when present, otherwise the
resolved shape-border point. They do not inherit the four-unit gap used by native automatic
Board connectors. This keeps short saved outward runs and the original label midpoint intact.
Endpoint shapes currently support positive axis-aligned transforms; rotated, sheared, reflected
or singular endpoint transforms use the reported generated fallback. Their node-local magnet
normals are not treated as parent-space normals. Free endpoints retain their exact coordinates.

The importer accepts alternating horizontal and vertical saved runs, up to 64 segments including
the first and last. The first and last segments must face outward from the resolved attachment
sides. Missing, malformed, nonfinite, excessive or conflicting saved metadata uses the existing
router and keeps the import report's note about FigCraft's own routing. A preserved saved path
does not count in that note. Existing saved vector snapshots take precedence.

Curved `connectorBezierControlPoints` must contain exactly two ordered, finite parent-space
`position` handles. Optional axis metadata must be finite when supplied; its semantics are not
inferred. Handle coordinates, tangent deltas, bounds and composed transforms must fit the existing
document limits before the importer claims preservation. Saved vector snapshots still take
precedence. Unsupported endpoint transforms and malformed or excessive metadata use the reported
generated fallback. The result is an editable cubic vector, retaining stroke, caps and dashes.

Straight connector points retain their existing behavior. `.jam` export, live Board attachment
and general obstacle avoidance are not added. Original format regressions cover these saved
paths; exact-source native-window acceptance remains separate.
