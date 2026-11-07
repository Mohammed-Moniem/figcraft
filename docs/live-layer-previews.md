# Live rectangle previews in Layers

A rectangle's Layers glyph follows its current aspect ratio, independent corner radii and
corner smoothing. Radius edits and Undo update it immediately; no reselection is needed.
Rectangles with image fills retain their Image glyph. Boolean operands retain the operation
glyph, and accessibility continues to describe the layer kind.

The preview uses the same original rounded-rectangle geometry as the document, normalized to
the row's small box before flattening. Invalid or empty dimensions use the ordinary type glyph.
It is an outline preview, not a rendered thumbnail of fills, effects or child content.
