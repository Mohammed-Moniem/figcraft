# Build, verify, save, export

The original synthetic `card.json` recipe uses no fonts or third-party assets. It creates a
320×120 horizontal auto-layout frame at page position (100,50), with padding16 and gap12.
Its blue80×40 child should have absolute bounds (116,66,80,40); the orange120×60 child should
have (208,66,120,60). These are assertions to verify, not coordinates to force onto the children.

From the skill directory, choose a new scratch directory for this example:

```sh
mkdir -p /absolute/work/design
figcraft-cli --no-system-fonts run references/card.json --out /absolute/work/design/card.figcraft --automation-read-root "$PWD/references" --automation-write-root /absolute/work/design
figcraft-cli info /absolute/work/design/card.figcraft --json --automation-read-root /absolute/work/design
figcraft-cli --no-system-fonts export /absolute/work/design/card.figcraft --node 'Skill Card' -o /absolute/work/design/card.png --automation-write-root /absolute/work/design
```

The command script is one all-or-nothing undo step. Each output line reports its step result;
retain the `id` returned for `card`, `blue` and `orange`. Re-running creation commands creates
more layers: inspect existing work before repeating them.

With configured MCP, run the recipe's steps through `batch {"commands":[...]}` and retain the
returned IDs. Then `get_node {"id":<card-id>,"depth":2}` must show the frame and both children's
actual `absoluteBounds`, sizes, fills and layout. Save with `save_file`, then export PNG using
the live `export` schema. Reopen the saved native file with `open_file` and inspect it again.
For a CLI-created file, `open_file {"path":"/absolute/work/design/card.figcraft"}` in a confined
headless MCP session gives the same inspection workflow. Do not invent a CLI `inspect` command.

Open the PNG and check both colored tiles, padding and gap. Inspect only the frame when choosing
an image export; a whole page may include unrelated work or a large background. Native saving,
reopening, numerical inspection and visual review are separate checks: report any you could not do.

For an existing design, use explicit `ids` with `node.set` and one small scoped `batch`. Query
the current params before changing layout. Validate a component by creating one instance and
checking its link/overrides, a variable by changing one mode on a test frame, and a prototype by
inspecting the interaction destination/trigger. These are verification patterns; their exact
commands come from the installed catalog rather than a frozen list here.
