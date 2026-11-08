//! Public export options: encoding quality, converted pixels and native setting persistence.

use base64::Engine as _;
use figcraft_color::{Color, ColorProfile};
use figcraft_doc::NodeId;
use image::ImageDecoder as _;
use serde_json::{Value, json};

use crate::Session;

fn ex(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(command, &params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn rectangle(s: &mut Session, fills: Value) -> NodeId {
    let value = ex(s, "shape.rectangle", json!({"width":200,"height":120}));
    let id = crate::params::id_of(&value["id"]).unwrap();
    ex(s, "node.set", json!({"id":id,"props":{"fills":fills}}));
    id
}

fn data(value: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.decode(value["data"].as_str().unwrap()).unwrap()
}

fn export(s: &mut Session, id: NodeId, format: &str, extra: Value) -> Vec<u8> {
    let mut params = json!({"ids":[id],"format":format});
    params.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    data(&ex(s, "file.export", params))
}

fn pixel(bytes: &[u8]) -> [u8; 4] {
    image::load_from_memory(bytes).unwrap().to_rgba8().get_pixel(50, 50).0
}

fn near(actual: [u8; 4], expected: [u8; 4], tolerance: u8) {
    assert!(actual.into_iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= tolerance), "{actual:?} != {expected:?}");
}

#[test]
fn jpeg_quality_is_checked_used_and_defaults_to_ninety() {
    let mut s = Session::new();
    let id = rectangle(&mut s, json!([{"type":"linearGradient","stops":["#FF0000","#00FF88","#2040FF"]}]));
    let default = export(&mut s, id, "jpg", json!({}));
    assert_eq!(default, export(&mut s, id, "jpg", json!({"quality":90})));
    let low = export(&mut s, id, "jpg", json!({"quality":20}));
    let high = export(&mut s, id, "jpg", json!({"quality":100}));
    assert!(low.len() < default.len() && default.len() < high.len(), "{} {} {}", low.len(), default.len(), high.len());
    assert_eq!(low, export(&mut s, id, "jpg", json!({"quality":"20"})));
    assert_eq!(image::load_from_memory(&low).unwrap().width(), 200);
    assert_eq!(export(&mut s, id, "png", json!({})), export(&mut s, id, "png", json!({"quality":20})));
    let profile = image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&low)).unwrap().icc_profile().unwrap();
    assert_eq!(profile, Some(ColorProfile::Srgb.icc()));
}

#[test]
fn invalid_export_options_refuse_without_changing_settings_or_writing_files() {
    let mut s = Session::new();
    let id = rectangle(&mut s, json!(["#FF0000"]));
    ex(&mut s, "export.add", json!({"ids":[id],"setting":{"format":"jpg","quality":40,"colorProfile":"displayP3"}}));
    let before = s.doc().unwrap().clone();
    let history = s.ds().unwrap().history.undo.len();
    for bad in [json!(0), json!(101), json!(-1), json!(1.5), json!(true), json!([80]), json!("high"), json!(1e300)] {
        for command in ["file.export", "export.update"] {
            let params = json!({"ids":[id],"index":0,"quality":bad});
            let params = if command == "file.export" { json!({"ids":[id],"format":"png","quality":bad}) } else { params };
            let error = s.execute(command, &params).unwrap_err().to_string();
            assert!(error.contains("whole number from 1 to 100"), "{bad}: {error}");
        }
        assert!(s.execute("export.add", &json!({"ids":[id],"setting":{"quality":bad}})).is_err());
    }
    for bad in [json!("invalid"), json!(42), json!(true), json!({})] {
        assert!(s.execute("file.export", &json!({"ids":[id],"colorProfile":bad})).is_err());
        assert!(s.execute("export.update", &json!({"ids":[id],"index":0,"colorProfile":bad})).is_err());
    }
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.ds().unwrap().history.undo.len(), history);
    let path = std::env::temp_dir().join(format!("figcraft-invalid-export-options-{}.png", std::process::id()));
    let _ = std::fs::remove_file(&path);
    assert!(s.execute("file.export", &json!({"ids":[id],"path":path,"quality":0})).is_err());
    assert!(!path.exists());
}

#[test]
fn pixel_exports_convert_channels_and_embed_the_selected_profile_without_editing_the_document() {
    let mut s = Session::new();
    let id = rectangle(&mut s, json!(["#FF0000"]));
    let authored = s.ds().unwrap().doc.clone();
    for format in ["png", "webp"] {
        let original = export(&mut s, id, format, json!({}));
        assert_eq!(pixel(&original), [255, 0, 0, 255]);
        let converted = export(&mut s, id, format, json!({"colorProfile":"displayP3"}));
        // Independent standard sRGB-red -> Display-P3 reference, including channel conversion.
        near(pixel(&converted), [234, 51, 35, 255], 2);
        let icc = if format == "png" {
            image::codecs::png::PngDecoder::new(std::io::Cursor::new(&converted)).unwrap().icc_profile().unwrap()
        } else {
            image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&converted)).unwrap().icc_profile().unwrap()
        };
        assert_eq!(icc, Some(ColorProfile::DisplayP3.icc()));
        assert_eq!(original, export(&mut s, id, format, json!({"colorProfile":"sameAsFile"})));
    }
    assert!(std::sync::Arc::ptr_eq(&authored, &s.ds().unwrap().doc));
    assert_eq!(s.doc().unwrap().color_profile, ColorProfile::Srgb);
}

#[test]
fn display_p3_to_srgb_clips_after_conversion_and_tags_srgb() {
    let mut s = Session::new();
    ex(&mut s, "file.colorProfile", json!({"profile":"displayP3","convert":false}));
    let id = rectangle(&mut s, json!(["#FF8000"]));
    let bytes = export(&mut s, id, "png", json!({"colorProfile":"srgb"}));
    near(pixel(&bytes), [255, 119, 0, 255], 2);
    assert!(bytes.windows(4).any(|chunk| chunk == b"sRGB"));
    assert_eq!(s.doc().unwrap().color_profile, ColorProfile::DisplayP3);
    let jpg = export(&mut s, id, "jpg", json!({"colorProfile":"srgb","quality":100}));
    near(pixel(&jpg), [255, 119, 0, 255], 4);
    assert_eq!(image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(jpg)).unwrap().icc_profile().unwrap(), Some(ColorProfile::Srgb.icc()));
}

#[test]
fn converted_jpeg_flattens_transparency_in_source_space_before_conversion() {
    let mut s = Session::new();
    let id = rectangle(&mut s, json!(["#FF000080"]));
    let bytes = export(&mut s, id, "jpg", json!({"quality":100,"colorProfile":"displayP3"}));
    let color = ColorProfile::Srgb.conversion_to(ColorProfile::DisplayP3).apply(Color::new(1.0, 127.0 / 255.0, 127.0 / 255.0, 1.0));
    near(pixel(&bytes), [(color.r * 255.0).round() as u8, (color.g * 255.0).round() as u8, (color.b * 255.0).round() as u8, 255], 4);
    assert_eq!(
        image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(bytes)).unwrap().icc_profile().unwrap(),
        Some(ColorProfile::DisplayP3.icc())
    );
}

#[test]
fn export_setting_options_roundtrip_node_aliases_native_archive_presets_and_undo() {
    let mut s = Session::new();
    let id = rectangle(&mut s, json!(["#FF0000"]));
    ex(&mut s, "export.add", json!({"ids":[id],"setting":{"format":"jpg","quality":30,"colorProfile":"displayP3","suffix":"-low"}}));
    let got = ex(&mut s, "node.get", json!({"id":id}))["exports"].clone();
    assert_eq!(got[0]["quality"], 30);
    assert_eq!(got[0]["colorProfile"], "displayP3");
    ex(&mut s, "node.set", json!({"id":id,"props":{"exports":[]}}));
    ex(&mut s, "node.set", json!({"id":id,"props":{"exports":got}}));
    assert_eq!(ex(&mut s, "node.get", json!({"id":id}))["exports"], got);
    ex(&mut s, "node.set", json!({"id":id,"props":{"exports":[{"format":"jpg","quality":30,"color_profile":"displayP3","suffix":"-low"}]}}));
    assert_eq!(ex(&mut s, "node.get", json!({"id":id}))["exports"], got);
    let direct = export(&mut s, id, "jpg", json!({"quality":30,"colorProfile":"displayP3"}));
    let files = ex(&mut s, "file.exportLayers", json!({"ids":[id]}));
    assert_eq!(data(&files["files"][0]), direct);
    ex(&mut s, "export.update", json!({"ids":[id],"index":0,"format":"png"}));
    ex(&mut s, "export.update", json!({"ids":[id],"index":0,"quality":null,"colorProfile":null}));
    let kept = ex(&mut s, "node.get", json!({"id":id}))["exports"].clone();
    assert_eq!(kept[0]["quality"], 30);
    assert_eq!(kept[0]["colorProfile"], "displayP3");
    ex(&mut s, "export.update", json!({"ids":[id],"index":0,"format":"jpg"}));
    ex(&mut s, "export.update", json!({"ids":[id],"index":0,"quality":95,"colorProfile":"srgb"}));
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(ex(&mut s, "node.get", json!({"id":id}))["exports"], got);
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (loaded, issues) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    let mut reopened = Session::with_document(loaded);
    assert_eq!(ex(&mut reopened, "node.get", json!({"id":id}))["exports"], got);
    let files = ex(&mut reopened, "file.exportLayers", json!({"ids":[id]}));
    assert_eq!(data(&files["files"][0]), direct);
}

#[test]
fn vector_exports_ignore_valid_raster_options_and_keep_existing_srgb_policy() {
    let mut s = Session::new();
    let id = rectangle(&mut s, json!(["#FF0000"]));
    assert_eq!(export(&mut s, id, "svg", json!({})), export(&mut s, id, "svg", json!({"quality":20,"colorProfile":"displayP3"})));
}

// Original composition controls. Append to export_options_tests.rs after the composition owner freezes.
// Uses that module's ex/rectangle/data/export/pixel/near helpers and existing image/zip/tempfile dependencies.
// Stored-row semantics only: file.exportLayers per-call quality/profile is undocumented and out of scope.
#[test]
fn mobile_stored_png_profile_preserves_converted_pixels_metadata_and_zip_members_in_every_layout() {
    use std::io::Read;
    let mut s = Session::new();
    let id = rectangle(&mut s, json!(["#FF0000"]));
    ex(&mut s, "node.set", json!({"ids":[id],"props":{"name":"Original Profile"}}));
    ex(&mut s, "export.add", json!({"ids":[id],"setting":{"format":"png","size":"2x","quality":21,"colorProfile":"displayP3"}}));
    let before = s.ds().unwrap().doc.clone();
    let history = s.undo_depth();
    let srgb = export(&mut s, id, "png", json!({"colorProfile":"srgb"}));
    assert_eq!(pixel(&srgb), [255, 0, 0, 255]);
    for layout in ["flutter", "ios", "android"] {
        let plain = ex(&mut s, "file.exportLayers", json!({"ids":[id],"layout":layout}));
        let files = plain["files"].as_array().unwrap();
        let images: Vec<_> = files.iter().filter(|f| f["name"].as_str().unwrap().ends_with(".png")).collect();
        let scales: &[f64] = if layout == "android" { &[1.0, 1.5, 2.0, 3.0, 4.0] } else { &[1.0, 2.0, 3.0] };
        assert_eq!(images.len(), scales.len());
        for (file, scale) in images.into_iter().zip(scales) {
            let bytes = data(file);
            let image = image::load_from_memory(&bytes).unwrap();
            assert_eq!((image.width(), image.height()), ((200.0 * scale) as u32, (120.0 * scale) as u32));
            // Independent standard sRGB-red to Display-P3 reference, rather than tag-only equality.
            near(pixel(&bytes), [234, 51, 35, 255], 2);
            let profile = image::codecs::png::PngDecoder::new(std::io::Cursor::new(&bytes)).unwrap().icc_profile().unwrap();
            assert_eq!(profile, Some(ColorProfile::DisplayP3.icc()));
            assert_eq!(bytes, export(&mut s, id, "png", json!({"scale":scale,"quality":21,"colorProfile":"displayP3"})));
        }
        let zipped = ex(&mut s, "file.exportLayers", json!({"ids":[id],"layout":layout,"zip":true}));
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data(&zipped))).unwrap();
        assert_eq!(archive.len(), files.len());
        for file in files {
            let mut archived = Vec::new();
            archive.by_name(file["name"].as_str().unwrap()).unwrap().read_to_end(&mut archived).unwrap();
            assert_eq!(archived, data(file), "ZIP retains exactly the profiled image or generated manifest");
        }
    }
    assert!(std::sync::Arc::ptr_eq(&before, &s.ds().unwrap().doc));
    assert_eq!(s.undo_depth(), history);
    assert_eq!(s.doc().unwrap().color_profile, ColorProfile::Srgb);
}

#[test]
fn mobile_stored_jpeg_quality_and_profile_reach_every_density_without_mutating_rows() {
    let mut s = Session::new();
    let id = rectangle(&mut s, json!([{"type":"linearGradient","stops":["#FF0000","#00FF88","#2040FF"]}]));
    ex(&mut s, "export.add", json!({"ids":[id],"setting":{"format":"jpg","quality":21,"colorProfile":"displayP3"}}));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    for layout in ["flutter", "ios", "android"] {
        let result = ex(&mut s, "file.exportLayers", json!({"ids":[id],"layout":layout}));
        let images: Vec<_> = result["files"].as_array().unwrap().iter().filter(|f| f["name"].as_str().unwrap().ends_with(".jpg")).collect();
        let scales: &[f64] = if layout == "android" { &[1.0, 1.5, 2.0, 3.0, 4.0] } else { &[1.0, 2.0, 3.0] };
        assert_eq!(images.len(), scales.len());
        for (file, scale) in images.into_iter().zip(scales) {
            let bytes = data(file);
            assert_eq!(bytes, export(&mut s, id, "jpg", json!({"scale":scale,"quality":21,"colorProfile":"displayP3"})));
            let high = export(&mut s, id, "jpg", json!({"scale":scale,"quality":90,"colorProfile":"displayP3"}));
            assert!(bytes.len() < high.len(), "stored quality21 must not silently become default90");
            assert_eq!(
                image::codecs::jpeg::JpegDecoder::new(std::io::Cursor::new(&bytes)).unwrap().icc_profile().unwrap(),
                Some(ColorProfile::DisplayP3.icc())
            );
        }
    }
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
}

#[test]
fn invalid_stored_or_direct_export_options_leave_rows_history_and_owned_output_empty() {
    let mut s = Session::new();
    let id = rectangle(&mut s, json!(["#FF0000"]));
    ex(&mut s, "export.add", json!({"ids":[id],"setting":{"format":"png","quality":21,"colorProfile":"displayP3"}}));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let folder = tempfile::tempdir().unwrap();
    let output = folder.path().join("invalid.png");
    for bad in [
        json!({"quality":0}),
        json!({"quality":101}),
        json!({"quality":1.5}),
        json!({"quality":true}),
        json!({"colorProfile":"invalid"}),
        json!({"colorProfile":42}),
    ] {
        let mut row = bad.clone();
        row["format"] = json!("png");
        assert!(s.execute("export.add", &json!({"ids":[id],"setting":row})).is_err());
        let mut update = bad.clone();
        update["ids"] = json!([id]);
        update["index"] = json!(0);
        assert!(s.execute("export.update", &update).is_err());
        let mut direct = bad;
        direct["ids"] = json!([id]);
        direct["format"] = json!("png");
        direct["path"] = json!(output);
        assert!(s.execute("file.export", &direct).is_err());
        assert!(!output.exists());
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
        assert_eq!(s.doc().unwrap(), &before);
        assert_eq!(s.undo_depth(), history);
    }
}
