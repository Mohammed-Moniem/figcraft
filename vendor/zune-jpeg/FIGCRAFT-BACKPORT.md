# FigCraft PR374 backport

The starting point is the published zune-jpeg 0.5.15 crate: SHA256
`27bc9d5b815bc103f142aa054f561d9187d191692ec7c2d1e2b4737f8dbd7296`,
VCS revision `31d81fed7551c8ccea456d9d8e2b1fd8bebb6995`, path `crates/zune-jpeg`.
All 39 published files and original license notices are preserved.

Only three production files are altered, using both reviewed commits from
[upstream PR374](https://github.com/etemesi254/zune-image/pull/374):

- [81582c09](https://github.com/etemesi254/zune-image/commit/81582c0901e6366602baf6ffa02aa3dbddf11913) computes maximum sampling before component strides in src/misc.rs.
- [5c436b46](https://github.com/etemesi254/zune-image/commit/5c436b46c40de58323ae5cb015708c77b61e41e2) sizes baseline/progressive scratch space from all component strides in src/mcu.rs and src/mcu_prog.rs.

These results match the three files at merged revision
`36007420831a5b9d57ebebf71fa63621bfbe0dc7`. The included PR374.patch replays the complete +28/−17-line production change, SHA256
`660f9746f05a9b5be0ac3d4a518f939feb3d7fd361d2da59a7e03cb03de37d07`.

PR374-approved-raw.patch preserves the exact approved historical artifact, SHA256
`6e4b23aed71e67cc2b099f6daad871093c4d34524f348c9b9ee30c6b319ce925`.
Its two hunk counts included trailing blank context that the artifact writer stripped, so that
raw file is not a replay patch. The replay file corrects only those two hunk counts (mcu6/6 and
mcu_prog6/13); no changed source line is added or removed. Both represent the same reviewed
three-file content. Keep the raw approval record unchanged.
No upstream fuzz image or unrelated change is copied. The version remains 0.5.15:
this is an explicit source backport, not a published 0.5.16 release or upstream FigCraft acceptance.

The unchanged full SPDX expression is **MIT OR Apache-2.0 OR Zlib**.
LICENSE-MIT, LICENSE-APACHE and LICENSE-ZLIB remain here. This local fork has a narrow
Zlib licensing exception for the reviewed #1492 backport; the global allowlist is unchanged.
No extra encoder dependency is added. The workspace excludes this third-party package from
app-package ownership; app lint policy is unchanged.

See [compatibility scope](../../docs/jpeg-cmyk.md) and [original fixtures](../../crates/format/tests/fixtures/jpeg/README.md).
