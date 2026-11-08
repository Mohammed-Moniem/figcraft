# Variable mode token JSON

Status: **IN_VALIDATION**. The supported adapter is locally integrated; matching combined native
and browser acceptance remain separate from its isolated producer checks.

In Local variables, right-click a mode header and choose **Import Figma mode…** or
**Export Figma mode…**. Import updates the chosen local collection and mode while preserving
existing variable metadata, native identities and other modes. New matching types can be added.
The asynchronous picker validates the live document, collection and mode again before using its
result; removed, replaced, remote or extended targets are refused.

The adapter follows the documented JSON dialect in
[Figma's variable-mode format requirements](https://help.figma.com/hc/en-us/articles/15343816063383-Modes-for-variables).
It supports one collection and one mode per file, rather than bulk multi-file intersection import.

| Token | Supported value |
|---|---|
| color | sRGB or HSL components with alpha; export converts to sRGB and reports conversion/clipping |
| dimension | A finite value with unit px |
| duration | A nonnegative finite value with unit s or ms on import; export uses s |
| number | A finite number |
| fontFamily | One font-family string |
| string | A general string outside reserved alias syntax |
| numeric boolean | number with com.figma.type: boolean; zero is false, other numbers are true |

Matching com.figma.type: string metadata is accepted for string and fontFamily tokens. Conflicting
type metadata is refused. Milliseconds normalize once to the native seconds convention.

Nested token names normalize to slash-separated variable names. Local aliases remain live links.
External aliasData first looks for a local variable ID, then a variable name inside an existing
collection ID, then a collection name only if that collection ID does not exist. Its target must
have a compatible type in an accessible writable local collection. Missing references, cycles,
conflicting types and duplicate normalized names are refused atomically. Unknown metadata produces
warnings; producer identifiers never become native identities. These explicit refusals make this
a supported subset, not a complete external round-trip guarantee.

The same command interfaces accept the dialect explicitly:

```json
[
  {"command":"variable.import","params":{"format":"figma","collection":"Theme","mode":"Night","variables":"theme.tokens.json"}},
  {"command":"variable.export","params":{"format":"figma","collections":["Theme"],"mode":"Night","path":"theme-night.tokens.json"}}
]
```

Import defaults to the Tokens collection and Default mode when no destination is supplied.
Without path, export returns text and warnings. The ordinary automation read/write-root and
atomic-file rules apply. Import is one document Undo step; export is read-only.

Auto import detects the Figma extensions or string token type. Explicit format dtcg retains
strict [DTCG subset semantics](dtcg-tokens.md), and format native selects native variables JSON.
Duration metadata remains descriptive: native binding/scope roles retain precedence for CSS and
Tailwind units; an otherwise unassigned duration uses seconds. It does not create native K-scale
authority or restrict native variable pickers.

Input retains the existing 256 MiB file cap and bounded DTCG traversal (64 levels, 100,000 tokens,
2 million JSON values and 64 MiB of text). Normalized names are limited to 4,096 bytes, and output
serialization is capped at 64 MiB. No import fetches a URL.

Source-scoped macOS ARM64 picker and original synthetic external-mode checks are retained.
Figma can normalize dimension to number and fontFamily to string: values and the observed Boolean
alias survive the sample, while type hints do not fully round-trip. These bounded checks do not
establish matching combined browser delivery, other native platforms or full format parity.
