# Custom pixel grid

Preferences ▸ Pixel grid size… opens the saved size field in Preferences. The size is in page
pixels, from 0.125 to 10000; Reset restores 1. The same setting is available to scripts as
pref.pixelGridSize with {size:8}, {reset:true}, or an empty query returning {size}.

The size applies to every open and future tab. It changes the canvas grid and pixel snapping
for moves, upright resizes, drawing, vector/pen points and ruler guides. Grid visibility and
Snap to pixel grid remain separate toggles. Keyboard nudge amounts, pixel-preview resolution,
frame layout grids and board grids keep their own settings. Changing this preference does not
edit artwork, selection or document Undo history.

Grid cells show when they are at least four screen pixels apart: the default size keeps its
400% zoom threshold; size 8 is visible from 50%. Lines align to page-origin multiples. Rendering
is bounded to 4096 lines per axis and skips nonfinite or numerically unrepresentable viewports.

Object snapping evaluates the raw pointer first and keeps the axes it reaches, including an
exact target with zero correction. Only free axes quantize to the chosen grid. This preserves
fractional object targets even when custom cells exceed the object-snap tolerance. Move snapping
uses a rotated parent's axes as before. The Scale tool and rotated resize exceptions remain.

Shift keeps the gesture's page direction; a rotated-grid correction is projected onto that
direction, and guides are updated for the resulting position.
