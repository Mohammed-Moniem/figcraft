# Shared edges between opaque rectangles

The CPU renderer fills consecutive plain rectangles of the same opaque solid color as one
winding path. Their shared fractional-pixel boundaries then receive coverage once, so a tiled
black area stays black on the canvas and in PNG exports. The rectangles remain separate editable
layers; rendering changes no document geometry or history. Outer edges remain antialiased.

Only visible, square-cornered leaf rectangles with one fully opaque normal solid fill, full
layer opacity, ordinary blending, and no strokes or effects use this path. Other artwork keeps
its existing compositor and stacking position. Existing frame clips, masks, selection-only
rendering, outline mode and retained/banded canvas rendering retain their semantics. Reflected
rectangles use a common winding direction so overlaps cannot cancel.

Path construction is linear and bounded to 4,096 rectangles at a time. Shared edges crossing
those chunk boundaries can retain the previous seam. Effective low-detail previews (currently
at scales of 0.5 and below) keep their existing path. Different colors, gradients, images,
translucent paints, strokes, effects, other shapes and the separate GPU canvas are outside this
repair. This is a scoped improvement for [issue #544](https://github.com/storytold/figcraft/issues/544),
not a general solution for every abutting fill.

Original synthetic fixtures check four touching 20 × 20 black squares at scales 1, 2.5, 1.37
and 0.73 against a single-piece reference on the CPU canvas and in decoded PNG exports. Further
controls cover translated/rotated/reflected geometry, reflected overlaps, alpha/vector/luminance
masks, clipping, stacking and reverse stacking, skip/solo/outline options, retained bands and
panning, and 5,000 off-screen rectangles. Fractional outer-edge quantization is excluded from
shared-interior assertions by requiring a fully opaque 3 × 3 reference neighborhood; the
asserted interior pixel values remain exact. No geometry rounding, inflation, export-scale
change or disabled antialiasing is involved. Launched native/browser and combined-source
acceptance remain separate checks.
