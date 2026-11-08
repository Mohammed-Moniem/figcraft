# CJK paragraph justification

Wrapped text using Justify distributes spare line width between eligible Chinese,
Japanese and Korean character units as well as ordinary spaces. The last line of a
paragraph and lines ending in a hard break retain their existing alignment. Korean
KeepAll keeps words together and expands their spaces instead.

Character opportunities follow the existing line-break policy: opening and closing
punctuation stay with their neighbors, shaped and grapheme clusters stay whole,
and a real space expands once. Zero-width spaces remain break markers. Direction
controls add no spacing opportunities; mixed right-to-left words and nested runs
keep their visual boundaries. Carets, hit testing and decorations use the adjusted
glyph positions.
Existing paragraph-indent eligibility remains Left-only; Justify preserves that policy.

This is the local behavior for issue [#1970](https://github.com/storytold/figcraft/issues/1970).
The upstream project merged its own fix in [#1989](https://github.com/storytold/figcraft/pull/1989);
this implementation was written from the local code and public behavior without
copying that patch. The allocation policy is not a claim of exact Figma mixed-script
spacing or complete CSS typography conformance. No preference or document schema
changes. An available font must actually cover the text; this change does not add
fonts or repair font fallback.

The original layout regressions cover right edges, punctuation and combining
clusters, explicit visual positions in mixed/nested bidi text, carets and hit
testing, real-space deduplication, KeepAll, underlines, indentation and unchanged
final/hard-break behavior. Matching native/browser acceptance and whole-workspace
checks remain part of integration validation.
