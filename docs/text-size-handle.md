# Direct font-size dragging

The Move tool shows a circular handle at the bottom center of one selected, visible, unlocked text layer. Drag it down the layer's local vertical axis to enlarge the type, or up to reduce it. Font sizes across styled runs retain their proportions; explicit pixel line heights scale with them, while Auto and percentage line heights keep their units. The factor follows the displacement relative to the starting box height and is limited so every font size remains between 1 and 5000.

Fixed text-box dimensions, transforms, fills, strokes and other spacing settings stay as they were. Auto-sized text and enclosing auto layout recompute naturally. One drag is one Undo step; Escape restores the starting document. The remaining border and corners resize the box as before, and the Scale tool keeps scaling the whole layer. Start with no modifiers to use the circle; modified border drags keep their usual behavior.

The handle is absent for multiple layers, text editing, Dev Mode, text on a path, locked or hidden content, singular transforms, and boxes smaller than 32 screen pixels wide or 8 pixels high, where its hit area would overlap the corner handles. This implements feature request #1563 in FigCraft; exact Adobe XD numerical parity has not been measured.
