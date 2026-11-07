//! Embeds the optional craft-fonts build input (`CRAFT_FONTS_DIR`): both `fonts/manifest.txt` and the
//! opt-in Latin presentation fonts in `fonts/latin-manifest.txt` (Inter, Roboto, …). Unset or missing,
//! `CRAFT_FONTS` is empty and native apps use system fonts. Required web releases enforce Inter
//! Regular/SemiBold, Noto Sans Georgian Regular and Noto Sans Arabic Regular after filtering.
//! Recipe: craft-fonts `docs/integration.md`.
//! Noto Sans CJK only with `FIGCRAFT_EMBED_CJK` set (the web build's only way to show CJK UI text).
//! Web faces are embedded losslessly compressed and decoded once; native faces remain raw.
use std::fmt::Write as _;
use std::path::PathBuf;

/// Font build script: emit `craft_fonts.rs` from `input_fonts`, passing the
/// `CRAFT_FONTS_DIR`, `CRAFT_FONTS_REQUIRED`, wasm target and `FIGCRAFT_EMBED_CJK` inputs;
/// report generation and write failures with `cargo::error`.
fn main() {
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=CRAFT_FONTS_REQUIRED");
    println!("cargo::rerun-if-env-changed=FIGCRAFT_EMBED_CJK");
    let dir = std::env::var_os("CRAFT_FONTS_DIR").map(PathBuf::from);
    let required = std::env::var_os("CRAFT_FONTS_REQUIRED").is_some();
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    let embed_cjk = std::env::var_os("FIGCRAFT_EMBED_CJK").is_some();
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default());
    let entries = match input_fonts(dir.as_deref(), wasm, required, embed_cjk, wasm.then_some(out_dir.as_path())) {
        Ok(entries) => entries,
        Err(e) => {
            println!("cargo::error={e}");
            String::new()
        }
    };
    let out = out_dir.join("craft_fonts.rs");
    if let Err(e) = std::fs::write(&out, catalog_source(&entries, wasm)) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// Preserve the native static slice; decode the complete web catalog once on first access.
fn catalog_source(entries: &str, wasm: bool) -> String {
    let mut src = if wasm {
        String::from(
            "pub static CRAFT_FONTS: std::sync::LazyLock<Vec<CraftFont>> = std::sync::LazyLock::new(|| {\n    static PACKED: &[(&str, &str, &crate::embedded::PackedFont)] = &[\n",
        )
    } else {
        String::from("pub static CRAFT_FONTS: &[CraftFont] = &[\n")
    };
    src.push_str(entries);
    src.push_str("];\n");
    if wasm {
        src.push_str("    PACKED.iter().filter_map(|(family, style, font)| Some(CraftFont { family, style, bytes: font.data(family, style)? })).collect()\n});\n");
    }
    src
}

/// Collect the effective catalog and enforce the required web release faces after filtering.
fn input_fonts(
    dir: Option<&std::path::Path>,
    wasm: bool,
    required: bool,
    embed_cjk: bool,
    packed_dir: Option<&std::path::Path>,
) -> Result<String, String> {
    let Some(dir) = dir else {
        return if required { Err("CRAFT_FONTS_REQUIRED is set but CRAFT_FONTS_DIR is missing".into()) } else { Ok(String::new()) };
    };
    let mut out = String::new();
    let mut faces = Vec::new();
    for manifest in ["fonts/latin-manifest.txt", "fonts/manifest.txt"] {
        let path = dir.join(manifest);
        if manifest == "fonts/latin-manifest.txt" && !path.exists() {
            watch(&path);
            continue;
        }
        match craft_fonts(dir, manifest, wasm, embed_cjk, packed_dir) {
            Ok(catalog) => {
                out.push_str(&catalog.entries);
                faces.extend(catalog.faces);
            }
            Err(e) if required => return Err(format!("CRAFT_FONTS_DIR={}: {e}", dir.display())),
            Err(e) => println!("cargo::warning=craft-fonts {manifest} skipped: CRAFT_FONTS_DIR={}: {e}", dir.display()),
        }
    }
    if required && wasm {
        for (family, style) in [("Inter", "Regular"), ("Inter", "SemiBold"), ("Noto Sans Georgian", "Regular"), ("Noto Sans Arabic", "Regular")] {
            if !faces.iter().any(|(f, s)| f == family && s == style) {
                return Err(format!("required web font missing: {family} {style}; CRAFT_FONTS_DIR must include both Latin and core release faces"));
            }
        }
    }
    Ok(out)
}

/// Rebuild when `manifest` changes or, while it is missing, when it is added to its folder.
///
/// Cargo takes a watched path that doesn't exist as changed on every build, and then rebuilds this
/// crate and every crate that depends on it each time. So a missing manifest is never watched
/// itself: its folder is, when there is one (a craft-fonts checkout without the Latin manifest).
/// With no craft-fonts at all there is nothing to watch, so the build can't notice it arriving:
/// rebuild this crate after adding a previously absent checkout.
fn watch(manifest: &std::path::Path) {
    let watched = if manifest.exists() { Some(manifest) } else { manifest.parent().filter(|folder| folder.is_dir()) };
    if let Some(path) = watched {
        println!("cargo::rerun-if-changed={}", path.display());
    }
}

struct Catalog {
    entries: String,
    faces: Vec<(String, String)>,
}

/// Parse a font manifest of `family | style | file | scripts` rows into `CraftFont`
/// entries: skip Adobe-authored families, skip CJK unless requested, keep the web catalog
/// to its budgeted families, canonicalize and watch each face, and emit `include_bytes!`
/// items.
fn craft_fonts(
    dir: &std::path::Path,
    manifest_rel: &str,
    wasm: bool,
    embed_cjk: bool,
    packed_dir: Option<&std::path::Path>,
) -> Result<Catalog, String> {
    let manifest = dir.join(manifest_rel);
    watch(&manifest);
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let mut out = String::new();
    let mut faces = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(" | ").map(str::trim).collect();
        let [family, style, file, scripts, ..] = f.as_slice() else {
            return Err(format!("malformed manifest line: {line}"));
        };
        // FigCraft's asset policy excludes Adobe-authored visual assets on every target.
        if matches!(*family, "Source Sans 3" | "Source Serif 4") {
            continue;
        }
        // CJK fonts are large; the desktop app finds system CJK fonts instead unless asked.
        let cjk = scripts.contains("Hans");
        if cjk && !embed_cjk {
            continue;
        }
        // Web builds keep Inter, Noto Sans Georgian, Noto Sans Arabic (default Persian/Arabic fallback), the
        // SemiBold wordmark, and optionally Noto Sans CJK (every CJK script, for CJK UI text).
        let wordmark = *family == "Archivo" && *style == "SemiBold";
        if wasm && !matches!(*family, "Inter" | "Noto Sans Georgian" | "Noto Sans Arabic") && !wordmark && !cjk {
            continue;
        }
        let path = dir.join(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
        println!("cargo::rerun-if-changed={}", path.display());
        faces.push(((*family).to_owned(), (*style).to_owned()));
        if let Some(packed_dir) = packed_dir.filter(|_| wasm) {
            let (packed, len) = pack_font(&path, packed_dir)?;
            let _ = writeln!(
                out,
                "    ({family:?}, {style:?}, {{ static FONT: crate::embedded::PackedFont = crate::embedded::PackedFont::new(include_bytes!({:?}), {len}); &FONT }}),",
                packed.display().to_string()
            );
        } else {
            let _ =
                writeln!(out, "    CraftFont {{ family: {family:?}, style: {style:?}, bytes: include_bytes!({:?}) }},", path.display().to_string());
        }
    }
    Ok(Catalog { entries: out, faces })
}

/// Write a lossless zlib copy under OUT_DIR, never into the font checkout. A path hash keeps
/// manifest paths out of generated output names while distinguishing source files.
fn pack_font(path: &std::path::Path, out_dir: &std::path::Path) -> Result<(PathBuf, usize), String> {
    use std::hash::{Hash, Hasher};
    use std::io::{Read as _, Write as _};
    // Match embedded::unpack: no required face may pass the build but exceed the decoder's bound.
    const MAX_FONT_BYTES: u64 = 256 * 1024 * 1024;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = file.metadata().map_err(|e| format!("{}: {e}", path.display()))?.len();
    if len == 0 || len > MAX_FONT_BYTES {
        return Err(format!("{}: embedded font size must be 1..={MAX_FONT_BYTES} bytes", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FONT_BYTES + 1).read_to_end(&mut bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_FONT_BYTES {
        return Err(format!("{}: embedded font size changed outside the supported range", path.display()));
    }
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    encoder.write_all(&bytes).map_err(|e| format!("compressing {}: {e}", path.display()))?;
    let packed = encoder.finish().map_err(|e| format!("compressing {}: {e}", path.display()))?;
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hash);
    let output = out_dir.join(format!("font-{:016x}.zlib", hash.finish()));
    std::fs::write(&output, packed).map_err(|e| format!("{}: {e}", output.display()))?;
    Ok((output, bytes.len()))
}

#[cfg(test)]
#[path = "tests/support/generated_catalog.rs"]
mod generated_catalog;

#[cfg(test)]
#[path = "tests/support/web_fallback.rs"]
mod web_fallback;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    pub(super) struct Fixture(pub(super) PathBuf);

    impl Fixture {
        /// Test fixture: create a unique temporary directory, write a synthetic face file and
        /// install the default Georgian and Arabic manifest.
        pub(super) fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("fig-font-input-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir_all(dir.join("fonts")).unwrap();
            // The build script validates manifest/file presence; font bytes are checked centrally.
            std::fs::write(dir.join("fonts/face.ttf"), b"synthetic build-input fixture").unwrap();
            let fixture = Self(dir);
            fixture.manifest("manifest.txt", &[("Noto Sans Georgian", "Regular"), ("Noto Sans Arabic", "Regular")]);
            fixture
        }

        /// Test fixture: write manifest rows that all point at the fixture's `fonts/face.ttf` face.
        fn manifest(&self, name: &str, faces: &[(&str, &str)]) {
            let text = faces.iter().map(|(family, style)| format!("{family} | {style} | fonts/face.ttf | Latn\n")).collect::<String>();
            std::fs::write(self.0.join("fonts").join(name), text).unwrap();
        }
    }

    impl Drop for Fixture {
        /// Remove the fixture's temporary directory, ignoring errors.
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Asserts the native Latin catalog keeps its 43 permitted presentation faces, the web
    /// catalog keeps only the four Inter faces, Adobe-authored families are absent, and
    /// permitted native faces remain.
    #[test]
    fn restored_latin_catalog_preserves_permitted_native_faces_and_web_budget() {
        let f = Fixture::new();
        std::fs::write(f.0.join("fonts/latin-manifest.txt"), include_str!("tests/fixtures/latin-families.txt")).unwrap();
        let native = craft_fonts(&f.0, "fonts/latin-manifest.txt", false, false, None).unwrap();
        assert_eq!(native.faces.len(), 43, "all prior permitted presentation faces remain available");
        let web = craft_fonts(&f.0, "fonts/latin-manifest.txt", true, false, None).unwrap();
        assert_eq!(web.faces.len(), 4, "the web catalog keeps only the four Inter faces");
        for catalog in [&native, &web] {
            assert!(!catalog.entries.contains("Source Sans 3"));
            assert!(!catalog.entries.contains("Source Serif 4"));
        }
        assert!(native.faces.iter().any(|(family, _)| family == "Carlito"));
        assert!(native.faces.iter().any(|(family, _)| family == "Liberation Serif"));
        assert!(input_fonts(Some(&f.0), true, true, false, None).is_ok());
    }

    /// Asserts required web input rejects a catalog missing Inter Regular or Inter SemiBold.
    #[test]
    fn required_web_rejects_core_only_and_partial_latin_catalogs() {
        let f = Fixture::new();
        assert!(input_fonts(Some(&f.0), true, true, false, None).unwrap_err().contains("Inter Regular"));
        f.manifest("latin-manifest.txt", &[("Inter", "Regular"), ("Unrelated", "SemiBold")]);
        assert!(input_fonts(Some(&f.0), true, true, false, None).unwrap_err().contains("Inter SemiBold"));
    }

    /// Asserts required web input embeds the Inter, Georgian and Arabic catalogs (four entries) and
    /// rejects a manifest without Noto Sans Georgian Regular.
    #[test]
    fn required_web_embeds_both_catalogs_and_requires_georgian() {
        let f = Fixture::new();
        f.manifest("latin-manifest.txt", &[("Inter", "Regular"), ("Inter", "SemiBold")]);
        let entries = input_fonts(Some(&f.0), true, true, false, None).unwrap();
        assert_eq!(entries.matches("CraftFont {").count(), 4);
        assert!(entries.contains("family: \"Inter\""));
        assert!(entries.contains("family: \"Noto Sans Georgian\""));
        assert!(entries.contains("family: \"Noto Sans Arabic\""));
        f.manifest("manifest.txt", &[("Other", "Regular")]);
        assert!(input_fonts(Some(&f.0), true, true, false, None).unwrap_err().contains("Noto Sans Georgian Regular"));
    }

    /// Asserts absent font input is empty when optional and an error when required, while present
    /// input includes Noto Sans Georgian.
    #[test]
    fn optional_and_native_inputs_keep_their_existing_policy() {
        let f = Fixture::new();
        assert!(input_fonts(None, true, false, false, None).unwrap().is_empty());
        assert!(input_fonts(None, true, true, false, None).unwrap_err().contains("CRAFT_FONTS_DIR is missing"));
        assert!(input_fonts(Some(&f.0), true, false, false, None).unwrap().contains("Noto Sans Georgian"));
        assert!(input_fonts(Some(&f.0), false, true, false, None).unwrap().contains("Noto Sans Georgian"));
    }

    /// Asserts required web input rejects a manifest whose face file is missing, even when every
    /// face row is listed.
    #[test]
    fn required_web_rejects_missing_asset_even_with_all_face_rows() {
        let f = Fixture::new();
        f.manifest("latin-manifest.txt", &[("Inter", "Regular"), ("Inter", "SemiBold")]);
        std::fs::remove_file(f.0.join("fonts/face.ttf")).unwrap();
        assert!(input_fonts(Some(&f.0), true, true, false, None).unwrap_err().contains("fonts/face.ttf"));
    }

    /// Reject file sizes the runtime decoder cannot accept before embedding release inputs.
    #[test]
    fn web_font_compression_rejects_empty_and_oversized_inputs() {
        let f = Fixture::new();
        let path = f.0.join("invalid.ttf");
        let file = std::fs::File::create(&path).unwrap();
        assert!(pack_font(&path, &f.0).unwrap_err().contains("font size"));
        file.set_len(256 * 1024 * 1024 + 1).unwrap();
        assert!(pack_font(&path, &f.0).unwrap_err().contains("font size"));
    }

    /// Packed web entries restore exact bytes; native catalogs retain their static raw interface.
    #[test]
    fn web_catalog_packs_fonts_and_native_keeps_original_bytes() {
        let f = Fixture::new();
        let web = craft_fonts(&f.0, "fonts/manifest.txt", true, false, Some(&f.0)).unwrap();
        assert_eq!(web.entries.matches("PackedFont::new").count(), 2);
        let path = f.0.join("fonts/face.ttf").canonicalize().unwrap();
        let (packed, len) = pack_font(&path, &f.0).unwrap();
        assert!(web.entries.contains(&packed.display().to_string()));
        let original = std::fs::read(&path).unwrap();
        let mut restored = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::ZlibDecoder::new(std::fs::File::open(packed).unwrap()), &mut restored).unwrap();
        assert_eq!(restored, original);
        assert_eq!(len, original.len());
        let native = craft_fonts(&f.0, "fonts/manifest.txt", false, false, Some(&f.0)).unwrap();
        assert!(native.entries.contains(&path.display().to_string()));
        let generated = catalog_source(&native.entries, false);
        assert!(generated.starts_with("pub static CRAFT_FONTS: &[CraftFont] = &["));
        assert!(!generated.contains("PackedFont"));
        assert!(!generated.contains("LazyLock"));
    }

    /// Persian and Arabic require a default web fallback even without local-font permission.
    #[test]
    fn default_web_catalog_keeps_arabic_without_opt_in() {
        let f = Fixture::new();
        f.manifest("manifest.txt", &[("Noto Sans Georgian", "Regular"), ("Noto Sans Arabic", "Regular"), ("Noto Sans Hebrew", "Regular")]);
        let entries = input_fonts(Some(&f.0), true, false, false, None).unwrap();
        assert!(entries.contains("family: \"Noto Sans Arabic\""));
        assert!(!entries.contains("Noto Sans Hebrew"), "unrelated script policy stays unchanged");
    }

    /// A required web release fails if the fallback is absent or only a non-Regular style exists.
    #[test]
    fn required_web_rejects_missing_arabic_regular() {
        let f = Fixture::new();
        f.manifest("latin-manifest.txt", &[("Inter", "Regular"), ("Inter", "SemiBold")]);
        for arabic in [None, Some("Bold")] {
            let mut faces = vec![("Noto Sans Georgian", "Regular")];
            if let Some(style) = arabic {
                faces.push(("Noto Sans Arabic", style));
            }
            f.manifest("manifest.txt", &faces);
            assert!(input_fonts(Some(&f.0), true, true, false, None).is_err_and(|e| e.contains("Noto Sans Arabic Regular")));
        }
    }
}
