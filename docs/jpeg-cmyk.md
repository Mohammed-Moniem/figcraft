# Progressive CMYK JPEG sampling

The local zune-jpeg 0.5.15 package backports both fixes from
[upstream PR374](https://github.com/etemesi254/zune-image/pull/374): determine maximum sampling
before component strides and allocate upsampling scratch for all components. Valid progressive
CMYK JPEGs with C/M/Y 1×1 and K 2×2 reach the decoder instead of being refused from their headers
(#1492), while avoiding the 8×8 upsampler panic (#1490).

The version remains 0.5.15. This is a portable source backport, rather than adoption of an
unrelated prerelease; see [vendor provenance](../vendor/zune-jpeg/FIGCRAFT-BACKPORT.md).
The full MIT OR Apache-2.0 OR Zlib expression is retained under the narrow exception for this
local backport. Global license rules are unchanged. All original package licenses accompany
the source. No encoder dependency or encoder/transcoder binary is added.

The renderer and thumbnail decoder retain their existing decoded-byte, side and allocation
limits. Sampling inspection remains a diagnostic API, not an image-validity test or rejection
rule. Invalid and truncated JPEGs can still be refused; general malformed-scan recovery is
not promised.

[Original fixtures and recipes](../crates/format/tests/fixtures/jpeg/README.md) cover 8×8,
300×200 and 17×19 frames. The small case contains the original color pattern; larger cases
are neutral padding for dimension and stride boundaries. Tests compare decoded/exported
pixels with an independent reference and exercise placement, cache reuse, filling, replacement,
opening, thumbnails, archive previews, malformed data and decode budgets.

Native, browser and cross-platform acceptance must be reported separately from targeted tests.
This change does not claim general JPEG/CMYK color-profile parity or include every later
upstream decoder change.

The tiny fixture has an inherited bottom/right padding difference from libjpeg fancy upsampling.
The progressive decode must match its baseline CMYK counterpart byte for byte, and strict
independent reference checks cover samples away from that padded edge. No edge-color fidelity
fix is included in the three-file backport.
