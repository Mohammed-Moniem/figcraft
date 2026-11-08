# Pencil stroke visibility

The Pencil chooses a black or white 2-unit stroke at its first point. It follows the same
background policy as new text: use the nearest containing frame's top visible mostly opaque
solid fill, otherwise the page. A white frame on a dark page therefore gets a black stroke.
The live preview and saved vector use the same color, retaining the existing minimum preview
width when zoomed out, pen-pressure profile, brush and one-stroke Undo behavior.

This policy does not sample image or gradient pixels, arbitrary non-container shapes, or effective
group opacity/compositing. It changes the color part of
[issue1741](https://github.com/storytold/figcraft/issues/1741). The rare extended curve-tail report
remains unreproduced; this change does not alter smoothing or clamp fitted curve geometry.

Regression coverage includes light/dark pages and frames, saved strokes and Undo, and the live
preview matching the completed stroke. Native acceptance and broader combined checks are tracked
separately; local test coverage is not a cross-platform or whole-product parity claim.
