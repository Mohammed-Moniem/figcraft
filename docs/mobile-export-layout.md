# Mobile asset folder layouts

`file.exportLayers` accepts `layout: "flat" | "flutter" | "ios" | "android"` (#1689).
The default `flat` layout keeps existing export rows, scales, suffixes and names. In the Export
panel, **Asset layout** chooses the layout for that panel's export action. The choice is local to
the window and does not edit the document's settings. Desktop mobile exports choose a folder;
browser mobile exports download one ZIP, even for a single source asset.

| Layout | Output for a 12 × 8 layer named `Arrow Left`, with a PNG export row |
|---|---|
| Flutter | `arrow_left.png` (12 × 8), `2.0x/arrow_left.png` (24 × 16), `3.0x/arrow_left.png` (36 × 24) |
| iOS | `Arrow Left.imageset/` containing `Arrow Left.png`, `Arrow Left@2x.png`, `Arrow Left@3x.png`, and `Contents.json` |
| Android | `drawable-mdpi/arrow_left.png` (12 × 8), `drawable-hdpi/arrow_left.png` (18 × 12), `drawable-xhdpi/arrow_left.png` (24 × 16), `drawable-xxhdpi/arrow_left.png` (36 × 24), `drawable-xxxhdpi/arrow_left.png` (48 × 32) |

Flutter and Android accept PNG, JPG and WebP rows. iOS accepts PNG and JPG rows. For Android,
an explicitly selected XML row writes one `drawable/arrow_left.xml` using the existing Vector
Drawable encoder, compatibility mode and warnings. Other format combinations give an error;
switch to a supported format or use Flat. Vector eligibility is not inferred automatically.

Mobile layouts replace each row's size with the platform density scales. Repeated rows that differ
only by size and a conventional `@2x` / `@3x` suffix are combined into one asset family. Explicit
variant suffixes and different rendering options produce separate families. Names for Flutter
and Android become ASCII lower snake case; names without ASCII letters or digits use `asset`.
iOS transliterates names to readable ASCII; two Unicode spellings that macOS treats as the same
name cannot overwrite each other. A collision adds `_2`, `_3`, and so on, consistently across the
family. Paths, Windows device names, component lengths and total file count are bounded. Planning
rejects more than 1000 input rows or output files before rendering or writing. The ZIP byte limit
and rendering limits still apply. Individual base64 output shares the same 256 MiB byte budget
as ZIP output, before encoding another retained copy. A later I/O or encoding failure can leave files already written
to a folder, as with flat export; an error does not claim a complete output.

The iOS manifest lists every generated image with its exact filename, universal idiom and scale;
it identifies FigCraft as author and uses format version1. It is returned as an ordinary file in
folder, individual base64 and ZIP output. Programmatic mobile export uses explicit layer settings;
the Inspect panel's `assets: true` discovery remains a separate Flat export action.

The CLI uses the same planner:

```sh
figcraft-cli export screen.figcraft -o assets --node "Arrow Left" --layout flutter
figcraft-cli export screen.figcraft -o Assets.xcassets --layout ios
figcraft-cli export screen.figcraft -o res --layout android --format xml
```

A mobile output is a folder, with an extension that does not name an image or document. Omit
`--scale`: the layout supplies density scales. CLI automation roots continue to confine every
created directory and written image or manifest.

Folder conventions come from the public [Flutter resolution-aware asset documentation](https://docs.flutter.dev/ui/assets/assets-and-images#resolution-aware-image-assets),
[Android density documentation](https://developer.android.com/training/multiscreen/screendensities),
and [Apple image-set format reference](https://developer.apple.com/library/archive/documentation/Xcode/Reference/xcode_ref-Asset_Catalog_Format/ImageSetType.html).
The export does not edit pubspec.yaml, create an Xcode project/catalog root, generate Android
launcher icons, or prove platform SDK acceptance. Synthetic engine/CLI checks and fresh app
validation are recorded separately from this format contract.
