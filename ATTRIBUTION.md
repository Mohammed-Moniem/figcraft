# Attribution

Every asset bundled in this repository (images, icons, fonts, data files) is listed here with its
author, source and licence. `cargo xtask assets` fails if a file under `assets/`, `docs/brand/` or
`docs/images/` is missing from this list.

FigCraft's user-interface icons are drawn in code (`crates/ui-egui/src/icons.rs`): original work,
MIT OR Apache-2.0. No third-party icon sets are used.

## Assets in this repository

| Path | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `assets/app-icon/` (all files: `figcraft.svg`, `figcraft-small.svg`, `figcraft-*.png`, `figcraft.ico`, `hicolor/`) | The project owner (engraving-style drawing, vectorised) | Original work, see [`assets/app-icon/README.md`](assets/app-icon/README.md); derived files rendered by `cargo xtask icon` | MIT OR Apache-2.0, [`assets/app-icon/LICENSE.txt`](assets/app-icon/LICENSE.txt) | FigCraft app icon (chameleon) |
| `docs/brand/artcraft-logo.svg`, `docs/brand/artcraft-logo.png`, `docs/brand/artcraft-logo-white.svg`, `docs/brand/artcraft-logo-white.png`, `docs/brand/artcraft-mark.svg`, `docs/brand/artcraft-mark.png`, `docs/brand/artcraft-mark-black.svg`, `docs/brand/artcraft-mark-black.png` | ArtCraft Team | ArtCraft brand kit | ArtCraft brand terms (`docs/brand/LICENSE-brand.txt`), not open source | Trademarks; unmodified use within FigCraft only |
| `docs/images/` (all screenshots and exports: `shot-*.png`, `art-*.png`) | FigCraft contributors | Screenshots of FigCraft showing synthetic demo documents made in FigCraft, and PNG exports of those documents by FigCraft's renderer | MIT OR Apache-2.0 | No third-party artwork |
| `examples/*.figcraft` | FigCraft contributors | Synthetic demo documents built through FigCraft's command API; invented copy, shapes and vectors drawn from scratch, no images or embedded fonts | MIT OR Apache-2.0 | Text uses Inter (OFL, from craft-fonts or the system); no third-party artwork |
| `.opencode/skills/figcraft/references/card.json` | FigCraft contributors | Original synthetic command recipe: one auto-layout frame and two solid-color rectangles | MIT OR Apache-2.0 | No fonts, images, third-party skill code or product assets |

## Offline spelling dictionary

`crates/engine/src/spelling/english.txt.zlib` is derived from Wordnik's MIT word list,
Git blob `06064b4a57884842eaabb8a996656ee84d22260d` of
<https://github.com/wordnik/wordlist/blob/main/wordlist-20210729.txt>.
Normalization retains lowercase ASCII words of at most64letters, deduplicates and sorts them,
then compresses them losslessly. Copyright2020 Wordnik; the full notice is preserved in
`crates/engine/src/spelling/WORDNIK-LICENSE.txt` and must accompany redistributed builds.
The backend is English, offline, with no network/system-dictionary access.

## Fonts (build input, not in this repository)

Release builds embed fonts from the optional craft-fonts build input
([storytold/craft-fonts](https://github.com/storytold/craft-fonts), `CRAFT_FONTS_DIR`). Its own
`ATTRIBUTION.md` and licence files are authoritative; the packages ship each font's licence.

| Family | Licence | Upstream |
|---|---|---|
| Inter | SIL Open Font License 1.1 | https://github.com/rsms/inter |
| Archivo | SIL Open Font License 1.1 | [Google Fonts / Archivo](https://github.com/google/fonts/tree/6c70c829f09ea345d3590406693220ea35c6553f/ofl/archivo) |
| Roboto, Montserrat, Lato, Open Sans, Poppins, Nunito Sans, Merriweather, Playfair Display, Carlito, Caladea | SIL Open Font License 1.1 | Google Fonts / upstream repositories (see craft-fonts) |
| Liberation Sans, Serif, Mono | SIL Open Font License 1.1 | https://github.com/liberationfonts |
| BIZ UDPGothic, BIZ UDMincho, Shippori Mincho, Noto Sans Arabic, Noto Sans Georgian | SIL Open Font License 1.1 | Google Fonts (see craft-fonts) |

The central catalog also contains Adobe-authored Source Sans 3 and Source Serif 4.
FigCraft filters both families from every build in accordance with its asset policy.

egui's default fonts (compiled into the `egui`/`epaint` crates) keep their own licences (OFL / Apache-2.0 / Ubuntu Font Licence); see those crates.

## egui dependency font icons

| Asset | Author | Source | Licence | Use |
|---|---|---|---|---|
| `epaint_default_fonts::EGUI_ICONS`, `epaint_default_fonts::EMOJI_ICON` (dependency font inputs) | John Slegers | [egui 6d518fc7](https://github.com/storytold/egui/tree/6d518fc799681571963842f95b55a6b68431b15a/crates/epaint_default_fonts/fonts), [emoji-icon-font](https://github.com/jslegers/emoji-icon-font) | MIT (Copyright (c) 2014 John Slegers; full upstream text in `NOTICE`) | egui-icons.ttf is the five-glyph subset, included by default_fonts independently of monochrome_emoji_fonts; emoji-icon-font.ttf is the full monochrome fallback. Dependency assets unmodified here; no font file copied into this repository. |

## Arabic interface catalog

| Path | Contributor | Source | Licence | Review |
|---|---|---|---|---|
| `crates/ui-egui/locales/ar.toml` | @hussain-hz with OpenAI Codex (AI-assisted) | Original Arabic wording from FigCraft's English catalog and engine command labels; no proprietary localisation consulted | MIT OR Apache-2.0 | Draft; native-speaker review and maintainer approval pending |

## Original JPEG regression fixtures

| Path | Author | Source | Licence | Notes |
|---|---|---|---|---|
| `crates/format/tests/fixtures/jpeg/progressive-cmyk-8x8.jpg` | FigCraft contributors | Original synthetic TINY_BASELINE_CMYK seed; [recipe](crates/format/tests/fixtures/jpeg/README.md) | MIT OR Apache-2.0 | Progressive CMYK C/M/Y 1×1, K 2×2; SHA256 `f64cd3a8359fcc4567e65cb18e00d66ea43b37b5c2620e93bca8cc0e17b504b9` |
| `crates/format/tests/fixtures/jpeg/progressive-cmyk-300x200.jpg` | FigCraft contributors | Original synthetic TINY_BASELINE_CMYK seed; [recipe](crates/format/tests/fixtures/jpeg/README.md) | MIT OR Apache-2.0 | Progressive CMYK C/M/Y 1×1, K 2×2; SHA256 `45a0e22f1f80df41d7d98b47edeb3ee3c41fe6d07517262051ed106e430c4a05` |
| `crates/format/tests/fixtures/jpeg/progressive-cmyk-17x19.jpg` | FigCraft contributors | Original synthetic TINY_BASELINE_CMYK seed; [recipe](crates/format/tests/fixtures/jpeg/README.md) | MIT OR Apache-2.0 | Progressive CMYK C/M/Y 1×1, K 2×2; SHA256 `c4ff081c7b5c472c0d065164c07b54ba07e004079683d9162dd5a20bd689ccbe` |

The decoder source package has separate [backport provenance and original license texts](vendor/zune-jpeg/FIGCRAFT-BACKPORT.md), under its unchanged MIT OR Apache-2.0 OR Zlib expression and the narrow local-fork exception. No third-party encoder/transcoder is redistributed.
