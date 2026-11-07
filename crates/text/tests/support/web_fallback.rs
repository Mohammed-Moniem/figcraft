//! Exercise shaping with the actual default web catalog, without desktop font fallbacks.

use super::{craft_fonts, pack_font};

#[path = "../../src/embedded.rs"]
mod embedded;
use figcraft_text::fontdb::{Face, FontBytes};

/// The default web faces render Persian and Arabic with joined letters and bidi ordering even
/// when the requested font is unavailable and system-font discovery is disabled.
#[test]
fn default_web_fonts_render_persian_and_arabic_without_system_fonts() {
    let required = std::env::var_os("CRAFT_FONTS_REQUIRED").is_some();
    let Some(dir) = std::env::var_os("CRAFT_FONTS_DIR").map(std::path::PathBuf::from).filter(|p| p.join("fonts/manifest.txt").is_file()) else {
        assert!(!required, "required real-font acceptance needs the pinned craft-fonts input");
        eprintln!("craft-fonts is absent; build-policy fixtures still run, real-font shaping skipped");
        return;
    };
    // Check source availability independently of the web filter under test. Otherwise removing
    // Arabic from that filter would skip this regression instead of reproducing the boxes.
    if [
        ("Inter", "Regular"),
        ("Inter", "Italic"),
        ("Inter", "SemiBold"),
        ("Inter", "Bold"),
        ("Noto Sans Georgian", "Regular"),
        ("Noto Sans Arabic", "Regular"),
    ]
    .iter()
    .any(|(family, style)| !figcraft_text::CRAFT_FONTS.iter().any(|font| font.family == *family && font.style == *style))
    {
        assert!(!required, "required real-font acceptance needs every release-pinned default face");
        eprintln!("complete release font catalog is absent; build-policy fixtures still run, real-font shaping skipped");
        return;
    }
    let packed_dir = super::tests::Fixture::new();
    let mut faces = Vec::new();
    for manifest in ["fonts/latin-manifest.txt", "fonts/manifest.txt"] {
        if dir.join(manifest).is_file() {
            faces.extend(craft_fonts(&dir, manifest, true, false, None).unwrap().faces);
        }
    }

    // This integration-test process owns its font database. Native embedding includes more
    // families than the web, so retain only the web build's faces and disable system discovery.
    figcraft_text::opt_out_of_system_fonts();
    let has_all_inter_styles = ["Regular", "Italic", "SemiBold", "Bold"].iter().all(|style| faces.iter().any(|(f, s)| f == "Inter" && s == style));
    let mut prior_raw_bytes = 0;
    let mut packed_bytes = 0;
    {
        let mut db = figcraft_text::db().write().unwrap();
        *db = figcraft_text::FontDb::default();
        for (family, style) in faces {
            let font = figcraft_text::CRAFT_FONTS.iter().find(|f| f.family == family && f.style == style).expect("manifest face is embedded");
            // Run the build-time compressor and the exact WASM decoder before shaping.
            let source = packed_dir.0.join("face.ttf");
            std::fs::write(&source, font.bytes).unwrap();
            let (packed, len) = pack_font(&source, &packed_dir.0).unwrap();
            let packed = std::fs::read(packed).unwrap();
            packed_bytes += packed.len();
            if family != "Noto Sans Arabic" {
                prior_raw_bytes += font.bytes.len();
            }
            let decoded = embedded::unpack(&packed, len).unwrap();
            assert_eq!(decoded, font.bytes);
            db.add(Face::from_bytes(FontBytes::Shared(std::sync::Arc::new(decoded)), 0, Some(font.family), Some(font.style)).unwrap());
        }
    }

    if has_all_inter_styles {
        assert!(packed_bytes < prior_raw_bytes, "including Arabic must not grow the embedded web font payload");
    }
    eprintln!("web font payload: {packed_bytes} compressed bytes including Arabic; {prior_raw_bytes} raw bytes before Arabic");

    for family in ["Iran Sans", "Inter"] {
        for characters in ["۴۵,۰۰۰ تومان به کیف پول بومرنگ شما واریز شد.", "مرحبا بالعالم ١٢٣"] {
            let mut text = figcraft_doc::TextData::new(characters);
            text.style.font_family = family.into();
            let layout = figcraft_text::layout(&text, None, None);
            assert!(!layout.glyphs.is_empty());
            assert!(layout.glyphs.iter().all(|g| g.gid != 0), "missing glyphs for {family}: {characters}");
            assert!(layout.glyphs.iter().any(|g| g.face.as_ref().is_some_and(|f| f.family == "Noto Sans Arabic")));
            assert!(layout.lines.first().unwrap().rtl, "Arabic/Persian paragraphs run right to left");
            if characters.starts_with('۴') {
                let x = |at| layout.glyphs.iter().find(|g| g.char_start == at).unwrap().x;
                assert!(x(0) < x(5), "Persian digits keep their left-to-right order inside the RTL paragraph");
            }
        }
        let mut joined = figcraft_doc::TextData::new("بب");
        joined.style.font_family = family.into();
        let layout = figcraft_text::layout(&joined, None, None);
        assert!(layout.glyphs.iter().all(|g| g.gid != 0));
        assert!(layout.glyphs.iter().all(|g| g.gid != g.face.as_ref().unwrap().glyph_for('ب')), "adjacent Arabic letters use joined forms");
        let mut isolated = figcraft_doc::TextData::new("ب");
        isolated.style.font_family = family.into();
        let isolated = figcraft_text::layout(&isolated, None, None);
        assert!(!isolated.glyphs.is_empty());
        assert!(isolated.glyphs.iter().all(|g| g.gid != 0));
        let plain: Vec<_> = isolated.glyphs.iter().map(|g| (&g.face.as_ref().unwrap().family, g.gid)).collect();
        // A font may decompose a letter into several glyphs (for example a base and its dot).
        // Test the two source clusters, rather than assuming one output glyph per character.
        for at in [0, 1] {
            let cluster: Vec<_> = layout.glyphs.iter().filter(|g| g.char_start == at).map(|g| (&g.face.as_ref().unwrap().family, g.gid)).collect();
            assert!(!cluster.is_empty(), "each joined source letter must produce glyphs");
            assert_ne!(cluster, plain, "each adjacent letter must shape differently from its isolated form");
        }
        assert!(layout.glyphs.iter().all(|g| g.char_start < 2 && g.char_end <= 2));
    }
}
