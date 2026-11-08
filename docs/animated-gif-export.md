# Animated GIF export

Status: **IN_VALIDATION** for the current combined build. This page describes the implemented Motion controls and command; matching combined and platform acceptance remain pending.

Open **Motion → Export GIF…**, choose a visible top-level frame, then set Duration, Frame rate, Scale and Loop. The dialog prefers the selected frame or the top-level frame containing a single selected layer; otherwise it starts with the first visible top-level frame on the page. Add a visible frame first if none is available. Opening the dialog pauses Motion playback.

The export samples the page's Motion timeline. Its viewport stays fixed at the chosen frame's unanimated bounds and scale: the camera does not follow moving or resizing content, and content outside that viewport is cropped. The export settings apply to this output without rewriting the saved timeline. Exporting leaves the base artwork, selection and document Undo history unchanged.

On a native save host, Export GIF opens a save picker with a frame-based .gif filename. Hosts that use downloads receive the generated file through their download service. A pending native save checks its document and page owner before exporting.

## Image and timing limits

GIF uses at most 256 colors per frame. Pixels are converted to sRGB; alpha below 50% becomes transparent and the rest becomes fully opaque. Soft transparency and wide-gamut colors therefore lose information. Editable Motion tracks stay in the FigCraft document.

Frame delays are rounded cumulatively to 10 ms. Individual delays can alternate at rates such as 60 fps, and the encoded duration can differ from the requested duration after rounding. A combination that would give any frame less than 10 ms is refused. Playback timing can also vary between viewers. Loop controls whether infinite-repeat metadata is written.

The exporter refuses work exceeding any of these bounds:

| Limit | Maximum |
|---|---:|
| Encoded frames | 1,200 |
| Output width or height | 16,384 px |
| Pixels in one frame | 16,777,216 |
| Pixels across all frames | 1,000,000,000 |
| Document nodes × frame count | 20,000,000 |
| Encoded output | 64 MiB |

The Motion page is also checked before sampling, including hidden layers: it must be a consistent tree within 1,000,000 visited nodes and 256 levels of descendant depth. Invalid references or excessive nesting are refused. Reduce scale, duration, frame rate or document size when a limit is reached. Frames are encoded one at a time; these are workload and output bounds, not a whole-process memory guarantee.

## Command

Use motion.export through the existing command interfaces. It supports GIF only:

~~~json
{
  "method": "engine.execute",
  "params": {
    "command": "motion.export",
    "params": {
      "id": 42,
      "format": "gif",
      "duration": 1000,
      "fps": 25,
      "loop": true,
      "scale": 1
    }
  }
}
~~~

Replace 42 with the frame's id.

| Option | Meaning |
|---|---|
| id | A visible top-level frame. If omitted, exactly one such frame must be selected. |
| format | gif, the default and only supported animated output. |
| duration | Integer milliseconds, 10–600,000. Defaults to the page's Motion duration. |
| fps | Integer 1–100. Defaults to the page's Motion frame rate; all other limits still apply. |
| loop | Boolean. Defaults to the page's Motion loop setting. |
| scale | Finite number from 0.01 to 64, default 1. |
| path | Optional nonempty output path. Automation write-root permissions apply. |

Without path, the result includes base64 data. With path, it writes the GIF and returns path instead. Both forms report format, id, width, height, frames, encoded duration in milliseconds, requested fps, loop, byte count and palette/timing warnings. Invalid options are rejected before output writing.

The existing static file.export workflow, layer export settings, JPEG quality/color-profile choices and asset folder layouts remain separate. GIF always uses the sRGB/palette rules above.

Isolated macOS ARM64 synthetic exports and Web compilation have their own source-scoped evidence. They do not establish browser delivery, other native platforms, the current combined runtime, or broad animation-format parity.
