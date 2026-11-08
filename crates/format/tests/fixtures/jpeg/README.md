# Original progressive CMYK JPEG fixtures

These are original synthetic test images, MIT OR Apache-2.0, derived exclusively from
FigCraft's existing original TINY_BASELINE_CMYK constant in src/jpeg_fixtures.rs.
No external image or upstream fuzz corpus was used. That 8×8 baseline seed has SHA256
`2a1b987d5ab4ab4a59a965ecebb8baf809c2859f9317c50a3cea08de687dabca` (752 bytes).

Transformation used installed libjpeg-turbo 3.1.4.1 jpegtran as local tooling only.
Neither its source nor binaries are shipped, and it is not a product/test dependency.
Extract the named seed's hex byte literals without modification, verify its hash, then run:

```sh
jpegtran -progressive -copy all -outfile progressive-cmyk-8x8.jpg baseline-cmyk-8x8.jpg
jpegtran -crop 300x200+0+0 -copy all -outfile baseline-cmyk-300x200.jpg baseline-cmyk-8x8.jpg
jpegtran -progressive -copy all -outfile progressive-cmyk-300x200.jpg baseline-cmyk-300x200.jpg
jpegtran -crop 17x19+0+0 -copy all -outfile baseline-cmyk-17x19.jpg baseline-cmyk-8x8.jpg
jpegtran -progressive -copy all -outfile progressive-cmyk-17x19.jpg baseline-cmyk-17x19.jpg
djpeg -strict -outfile reference.ppm progressive-cmyk-8x8.jpg
```

All three have SOF2, four components, C/M/Y 1×1 and K 2×2. The 8×8 case retains the
original color pattern. Expansion uses coefficient-domain neutral padding: the larger and
odd-size cases are opaque gray 64, rather than retained or replicated color patterns.
Strict djpeg reference decoding completed without warnings. Use its default CMYK conversion;
forcing -rgb is unsupported by this tool. The 8×8 RGB references are (0,0)=[102,104,101],
(4,4)=[30,28,27], (6,6)=[35,77,85]; larger cases are [64,64,64]. Reference checks away from
the bottom/right padded edge allow three levels per channel. The existing decoder differs from
libjpeg fancy upsampling on the tiny fixture's last row/column (maximum channel difference 42,
mean 2.505 across the image); this backport does not change that policy. Tests instead require
the entire progressive 8×8 output, including edges, to equal the baseline CMYK output exactly.
Every larger neutral pixel is checked against the independent gray reference.

| File | Bytes | SHA256 |
|---|---:|---|
| progressive-cmyk-8x8.jpg | 879 | f64cd3a8359fcc4567e65cb18e00d66ea43b37b5c2620e93bca8cc0e17b504b9 |
| progressive-cmyk-300x200.jpg | 1218 | 45a0e22f1f80df41d7d98b47edeb3ee3c41fe6d07517262051ed106e430c4a05 |
| progressive-cmyk-17x19.jpg | 788 | c4ff081c7b5c472c0d065164c07b54ba07e004079683d9162dd5a20bd689ccbe |

Header-byte edits in jpeg_fixtures.rs are malformed-input controls. They must not replace
these valid full-scan positive fixtures.
