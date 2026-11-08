# DTCG variables interchange

Local variables ▸ Export… ▸ **DTCG design tokens…** writes a `.tokens.json` file.
Import… reads it back from a native file picker or a browser file input. The same commands work through the control channel, MCP, plugins and
`figcraft-cli run script.json`:

```json
[
  {"command":"variable.export","params":{"format":"dtcg","path":"theme.tokens.json"}},
  {"command":"variable.import","params":{"variables":"theme.tokens.json"}}
]
```

Omit `path` to receive `{text}`. `collections` filters by collection id/name as in the other
variable formats. DTCG import is detected automatically from an object, JSON text or a regular
file within the existing 256 MiB cap. Import merges by collection/variable/mode name and is one
undo step. Invalid input is refused before document mutation. Scripts/batches reserve inline
`$ref` objects for step references; pass DTCG containing JSON Pointers as a file path or JSON text
to preserve those pointers through that transport. Direct command calls accept the token object.

The implementation targets the variables FigCraft already has, using the published
[DTCG 2025.10 Format module](https://www.designtokens.org/tr/2025.10/format/) and
[Color module](https://www.designtokens.org/tr/2025.10/color/). It is a supported subset, not a
claim of complete DTCG authoring or resolver conformance.

- Export creates a group per collection. Color values identify sRGB or Display P3; numbers used
  as lengths become `dimension` in pixels, and opacity/font-weight numbers remain `number`.
  A retained token type takes priority for DTCG export. Descriptions and compatible aliases use `$description` and `{group.token}`. Illegal name
  characters are escaped reversibly in token keys. Original names stay in the native extension.
- Import supports inherited group types, nested groups, `$root`, typed/chained aliases, local
  whole-token JSON Pointer aliases and local property pointers. Following
  [Format §5.2.2](https://www.designtokens.org/tr/2025.10/format/#type), aliases without a token-level
  type infer the referenced type before considering a group default; explicit types must agree. Supported values are `color`,
  `dimension`, `number`, single-string `fontFamily`, and `fontWeight`. Color input supports sRGB,
  linear sRGB, Display P3, HSL, LCH and OKLCH; values are converted to the receiving file's profile
  and clipped to its representable gamut, using the existing color conversion. LCH/OKLCH first use
  the existing sRGB gamut mapping (including its practical chroma bounds), then convert profiles.
  Other spaces, missing color components (`none`), composite types, group `$extends`, external
  references and font-family fallback arrays are refused with the affected path.
- `rem` dimensions become pixel numbers using `remSize` (default 16). Pass a positive
  `remSize` up to 100000 to choose the conversion. Export writes the resulting pixel value.

## Modes and native values

The ordinary token tree exposes each collection's default mode. `variable.export` with
`{"format":"dtcg","mode":"Dark"}` exposes that mode in collections that have it, with defaults
elsewhere; an unknown mode is refused. Export separate files with different `mode` arguments
when a downstream pipeline needs one file per theme. This does not implement the separate
[DTCG Resolver module](https://www.designtokens.org/tr/2025.10/resolver/).

The `$extensions["ai.storyteller.figcraft"]` object, version 1, preserves the native variables
snapshot, modes and color profile. Token extensions identify the native variable and remember the
exported value/type. This retains strings, booleans and color calculations without inventing
nonstandard DTCG `$type` values. Generic DTCG tools see the standard token tree; general strings,
booleans and additional modes are available to consumers that understand this extension. Native
round trips retain values, alias relationships and usage scopes. Standard types are retained as
optional native designTokenType metadata through native files and variables JSON. Import enriches
older untyped variables with the exposed standard type; general strings and booleans remain untyped.
The Variables editor keeps it through copy/paste and Undo/Redo; variable.create accepts an optional
compatible designTokenType for programmatic creation. This metadata does not restrict native variable pickers. Real bindings and explicit scopes still
take priority when CSS determines units. Aliases between different standard types are flattened in
the standard tree while the native extension keeps their live expressions. Edited standard token values override
the snapshot for the exposed mode; unedited calculated values keep their expressions.

Importing tokens without that extension creates ordinary local variables with one Default mode.
The existing import merge materializes collections as local collections rather than reconnecting
library inheritance. Referenced variables must be included in an export; missing dependencies are
refused. Foreign extension data is refused because the current variables model cannot preserve it.

Input is bounded to 64 nested levels, 100000 tokens, 2 million JSON values/reference visits and
64 MiB of text, in addition to the file cap. Cycles, dangling references, mismatched alias types,
invalid units, invalid colors and unsupported types produce errors. Alias/calculation graphs must
fit the engine's 32-variable resolution limit. Import never fetches a URL. A merge whose referenced
variable has a conflicting type is refused before mutation.
