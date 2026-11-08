# Connecting to off-screen prototype targets

In the Prototype tab, drag a layer's connection handle toward an unobstructed canvas edge and hold it there. The viewport pans toward that edge, so you can bring another frame into view and release on it to create the interaction. Moving back to the canvas center stops the pan. The source end follows its layer while the free end stays under the pointer.

Panning changes the view without changing the document or Undo history. A valid drop adds one interaction and one Undo step. Release, Escape, focus loss, a missing pointer, switching away from Prototype, or changing the document, page, source or window retires the gesture. A touch-style release can still finish a valid drop when its pointer disappears in the release frame. Side panels and covering widgets do not trigger edge panning.

The local interaction uses a 32 logical-point edge margin, speed increasing toward the edge up to 480 logical points per second, and a frame step capped at 1/30 second. Its speed is independent of canvas zoom and diagonal movement is bounded. These are FigCraft interaction choices for issue #2015; numerical parity with another editor has not been measured. Prototype-player scrolling is a separate feature.
