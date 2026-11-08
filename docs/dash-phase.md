# Dash phase

Advanced stroke settings offer **Manual dash phase** for dashed strokes. Leave it off to keep automatic placement: open paths fit complete patterns with half a dash at each end; closed paths keep the written pattern. Existing native documents keep this behavior.

Enable manual phase to keep the written dash and gap lengths and move their starting point. **Phase** accepts a signed distance in pixels. Zero starts with the first full dash; positive values advance into the pattern, and negative values wrap backward. Full pattern lengths produce the same placement. Odd dash lists repeat twice, and each subpath restarts the phase independently. Switching automatic placement back on removes the optional phase value.

For a mixed selection, enabling manual placement sets automatic strokes to zero and preserves strokes already using manual values. Arrow and enabled focused-wheel changes apply relatively to mixed values. A common numeric value can also be scrubbed. A field draft is discarded when its layer, document, source value or window focus changes. A phase edit uses the existing command and Undo mechanism; ordinary resizing preserves the distance, while Scale scales it with the dash lengths.

Automation uses `node.set` with `props.dashPhase`: a finite JSON number selects manual placement, and `null` restores automatic placement. Inspection omits the property in automatic mode. Native save and collaboration retain the setting. SVG stores the effective dash array and offset; imported dashed SVG uses explicit manual placement even when its offset is omitted, because SVG's omitted offset means zero. SVG therefore preserves drawn placement rather than the native automatic-fitting relationship. Raster, PDF and Outline Stroke use the same stroke geometry. Figma and Penpot exports warn that editable manual phase is omitted.

This covers manual phase from [issue #1681](https://github.com/storytold/figcraft/issues/1681). Centering dashes on corners remains open. This change does not claim complete stroke parity or cross-platform runtime acceptance.
