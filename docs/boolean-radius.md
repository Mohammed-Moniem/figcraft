# Uniform corners on live Boolean results

A live Boolean that contains a rectangle can have a uniform corner radius. Select the result and edit **Corner radius**, or use the existing node.set command with its cornerRadius property. Fractional radii are supported. The setting rounds the computed result in its local coordinates, including sharp intersection and concave cutout corners; operands remain editable with their original radii and hierarchy.

Radius changes and edits to operand geometry, visibility or operations recompute the shared outline. Canvas drawing, normal hit tests, raster exports, SVG and PDF use that outline. Native archives and .fig keep the live Boolean, its operands and the uniform setting; component instances inherit the setting as other geometry properties do.

Pasting properties from a uniform-radius source carries that radius to an eligible Boolean. Independent source corners and unsupported smoothing are omitted while paints, effects and other supported appearance still paste; the existing result radius stays unchanged.

Radius zero preserves the existing result. Ellipse-only Boolean groups do not expose or apply a group radius. Independent corner radii, corner smoothing and bounding-box corner handles for arbitrary Boolean geometry are outside this support. The existing bounded circular vector-corner construction clamps each corner to its adjacent edges; this does not claim numerical parity with another product.

Penpot export keeps the Boolean operands editable but omits result rounding and reports that limitation in its warnings. Existing Boolean depth, path-operation and output limits remain in force; geometry that exceeds them is refused rather than computed without bounds.
