use crate::{Session, params::id_of};
use base64::Engine as _;
use figcraft_doc::NodeId;
use serde_json::{Value, json};

fn execute(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(command, &params).unwrap()
}

fn asset(s: &mut Session, name: &str, format: &str) -> NodeId {
    let r = execute(s, "shape.rectangle", json!({"width":12,"height":8}));
    let id = id_of(&r["id"]).unwrap();
    execute(s, "node.set", json!({"ids":[id],"props":{"name":name,"fills":["#E63B56"]}}));
    execute(s, "export.add", json!({"ids":[id],"setting":{"format":format}}));
    id
}

fn names(r: &Value) -> Vec<&str> {
    r["files"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect()
}

fn bytes(f: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.decode(f["data"].as_str().unwrap()).unwrap()
}

fn assert_pngs(r: &Value, expected: &[(u32, u32)]) {
    let files = r["files"].as_array().unwrap();
    assert_eq!(files.len(), expected.len());
    for (file, size) in files.iter().zip(expected) {
        let b = bytes(file);
        assert!(b.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!((u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap())), *size);
    }
}

#[test]
fn mobile_layout_flutter_generates_resolution_variants() {
    let mut s = Session::new();
    let id = asset(&mut s, "Arrow Left", "png");
    let r = execute(&mut s, "file.exportLayers", json!({"ids":[id],"layout":"flutter"}));
    assert_eq!(names(&r), ["arrow_left.png", "2.0x/arrow_left.png", "3.0x/arrow_left.png"]);
    assert_pngs(&r, &[(12, 8), (24, 16), (36, 24)]);
}

#[test]
fn mobile_layout_ios_manifest_references_every_actual_scale() {
    let mut s = Session::new();
    let id = asset(&mut s, "Arrow Left", "png");
    let r = execute(&mut s, "file.exportLayers", json!({"ids":[id],"layout":"ios"}));
    assert_eq!(
        names(&r),
        [
            "Arrow Left.imageset/Arrow Left.png",
            "Arrow Left.imageset/Arrow Left@2x.png",
            "Arrow Left.imageset/Arrow Left@3x.png",
            "Arrow Left.imageset/Contents.json"
        ]
    );
    let files = r["files"].as_array().unwrap();
    for (f, scale) in files[..3].iter().zip([1, 2, 3]) {
        let image = image::load_from_memory(&bytes(f)).unwrap();
        assert_eq!((image.width(), image.height()), (12 * scale, 8 * scale));
    }
    let manifest: Value = serde_json::from_slice(&bytes(&files[3])).unwrap();
    assert_eq!(manifest["info"], json!({"author":"FigCraft","version":1}));
    for (entry, (filename, scale)) in
        manifest["images"].as_array().unwrap().iter().zip([("Arrow Left.png", "1x"), ("Arrow Left@2x.png", "2x"), ("Arrow Left@3x.png", "3x")])
    {
        assert_eq!(entry, &json!({"filename":filename,"idiom":"universal","scale":scale}));
    }
    assert_eq!(manifest["images"].as_array().unwrap().len(), 3);
}

#[test]
fn mobile_layout_android_generates_five_density_buckets() {
    let mut s = Session::new();
    let id = asset(&mut s, "Arrow Left", "png");
    let r = execute(&mut s, "file.exportLayers", json!({"ids":[id],"layout":"android"}));
    assert_eq!(
        names(&r),
        [
            "drawable-mdpi/arrow_left.png",
            "drawable-hdpi/arrow_left.png",
            "drawable-xhdpi/arrow_left.png",
            "drawable-xxhdpi/arrow_left.png",
            "drawable-xxxhdpi/arrow_left.png"
        ]
    );
    assert_pngs(&r, &[(12, 8), (18, 12), (24, 16), (36, 24), (48, 32)]);
}

#[test]
fn mobile_layout_android_xml_uses_single_drawable_resource() {
    let mut s = Session::new();
    let id = asset(&mut s, "Arrow Left", "xml");
    let r = execute(&mut s, "file.exportLayers", json!({"ids":[id],"layout":"android"}));
    assert_eq!(names(&r), ["drawable/arrow_left.xml"]);
    assert!(String::from_utf8(bytes(&r["files"][0])).unwrap().contains("<vector"));
    assert_eq!(r["files"][0]["warnings"], json!([]));
}

#[test]
fn mobile_layout_rejects_unknown_layout_and_unsupported_format() {
    let mut s = Session::new();
    let id = asset(&mut s, "Icon", "svg");
    assert!(s.execute("file.exportLayers", &json!({"ids":[id],"layout":"andriod"})).is_err());
    for layout in ["flutter", "ios", "android"] {
        assert!(s.execute("file.exportLayers", &json!({"ids":[id],"layout":layout})).is_err(), "{layout}");
    }
}

#[test]
fn mobile_layout_flat_is_the_existing_default() {
    let mut s = Session::new();
    let id = asset(&mut s, "icons/Arrow Left", "png");
    let default = execute(&mut s, "file.exportLayers", json!({"ids":[id]}));
    let explicit = execute(&mut s, "file.exportLayers", json!({"ids":[id],"layout":"flat"}));
    assert_eq!(default, explicit);
    assert_eq!(names(&default), ["icons/Arrow Left.png"]);
    assert_pngs(&default, &[(12, 8)]);
}

#[test]
fn mobile_layout_all_delivery_modes_keep_identical_ios_members() {
    use std::io::Read;
    let mut s = Session::new();
    let id = asset(&mut s, "Clock", "png");
    let plain = execute(&mut s, "file.exportLayers", json!({"ids":[id],"layout":"ios"}));
    let zipped = execute(&mut s, "file.exportLayers", json!({"ids":[id],"layout":"ios","zip":true}));
    assert_eq!(names(&plain), names(&zipped));
    let archive_bytes = bytes(&zipped);
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(archive_bytes)).unwrap();
    assert_eq!(archive.len(), 4);
    for f in plain["files"].as_array().unwrap() {
        let mut actual = Vec::new();
        archive.by_name(f["name"].as_str().unwrap()).unwrap().read_to_end(&mut actual).unwrap();
        assert_eq!(actual, bytes(f));
    }
    let dir = tempfile::tempdir().unwrap();
    let folder = execute(&mut s, "file.exportLayers", json!({"ids":[id],"layout":"ios","dir":dir.path().to_string_lossy()}));
    assert_eq!(names(&plain), names(&folder));
    for (actual, want) in folder["files"].as_array().unwrap().iter().zip(plain["files"].as_array().unwrap()) {
        assert_eq!(std::fs::read(actual["path"].as_str().unwrap()).unwrap(), bytes(want));
    }
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn mobile_layout_coalesces_density_rows_and_preserves_variants_and_options() {
    let mut s = Session::new();
    let id = asset(&mut s, "Clock", "png");
    execute(&mut s, "export.add", json!({"ids":[id]}));
    execute(&mut s, "export.add", json!({"ids":[id]}));
    execute(&mut s, "export.add", json!({"ids":[id],"setting":{"suffix":"-dark","format":"png"}}));
    execute(&mut s, "export.add", json!({"ids":[id],"setting":{"includeBoundingBox":true,"format":"png"}}));
    let before = s.doc().unwrap().get(id).unwrap().exports.clone();
    let r = execute(&mut s, "file.exportLayers", json!({"ids":[id],"layout":"flutter"}));
    assert_eq!(
        names(&r),
        [
            "clock.png",
            "2.0x/clock.png",
            "3.0x/clock.png",
            "clock_dark.png",
            "2.0x/clock_dark.png",
            "3.0x/clock_dark.png",
            "clock_2.png",
            "2.0x/clock_2.png",
            "3.0x/clock_2.png"
        ]
    );
    assert_eq!(s.doc().unwrap().get(id).unwrap().exports, before, "export never edits stored rows");
}

#[test]
fn mobile_layout_names_are_safe_unique_and_consistent_across_densities() {
    let mut s = Session::new();
    let mut ids = Vec::new();
    for name in ["../Arrow Left", "..\\Arrow Left", "CON", "42", "⚙️", "NUL.txt"] {
        ids.push(asset(&mut s, name, "png"));
    }
    let r = execute(&mut s, "file.exportLayers", json!({"ids":ids,"layout":"android"}));
    let names = names(&r);
    assert_eq!(names.len(), 30);
    for (i, base) in ["arrow_left", "arrow_left_2", "asset_con", "asset_42", "asset", "nul_txt"].iter().enumerate() {
        for (j, density) in ["mdpi", "hdpi", "xhdpi", "xxhdpi", "xxxhdpi"].iter().enumerate() {
            assert_eq!(names[i * 5 + j], format!("drawable-{density}/{base}.png"));
        }
    }
    assert_eq!(names.iter().collect::<std::collections::BTreeSet<_>>().len(), 30);
}

#[test]
fn mobile_layout_rejects_oversized_or_invalid_complete_plan_before_writing() {
    use super::{ExportLayout, mobile_export_plan};
    use figcraft_doc::ExportSetting;
    let mut s = Session::new();
    let good = asset(&mut s, "Clock", "png");
    let bad = asset(&mut s, "Unsupported", "svg");
    let dir = tempfile::tempdir().unwrap();
    let result = s.execute("file.exportLayers", &json!({"ids":[good,bad],"layout":"ios","dir":dir.path().to_string_lossy()}));
    assert!(result.is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    let too_many = (0..201).map(|i| ExportSetting { suffix: format!("-{i}"), ..Default::default() }).collect();
    let error = mobile_export_plan(s.doc().unwrap(), [(good, too_many)], ExportLayout::Android).err().unwrap().to_string();
    assert!(error.contains("1000 files"), "{error}");
    let duplicate_rows = vec![ExportSetting::default(); 1001];
    let error = mobile_export_plan(s.doc().unwrap(), [(good, duplicate_rows)], ExportLayout::Flutter).err().unwrap().to_string();
    assert!(error.contains("1000 source settings"), "{error}");
    for value in [Value::Null, json!(true), json!(1), json!([]), json!({})] {
        assert!(s.execute("file.exportLayers", &json!({"ids":[good],"layout":value})).is_err());
    }
}

#[test]
fn mobile_layout_confined_directory_cannot_write_outside_authorized_root() {
    let mut s = Session::new();
    let id = asset(&mut s, "Clock", "png");
    let dir = tempfile::tempdir().unwrap();
    let inside = dir.path().join("inside");
    std::fs::create_dir(&inside).unwrap();
    let roots = std::sync::Arc::new(crate::file_access::Roots::new(&[], &[inside.to_string_lossy().into_owned()]).unwrap());
    let outside = dir.path().join("outside");
    let result = crate::file_access::confined(Some(&roots), || {
        s.execute("file.exportLayers", &json!({"ids":[id],"layout":"ios","dir":outside.to_string_lossy()}))
    });
    assert!(result.is_err());
    assert!(!outside.exists());
    let result = crate::file_access::confined(Some(&roots), || {
        s.execute("file.exportLayers", &json!({"ids":[id],"layout":"ios","dir":inside.to_string_lossy()}))
    })
    .unwrap();
    assert_eq!(names(&result).len(), 4);
    assert!(inside.join("Clock.imageset/Contents.json").is_file());
}

#[test]
fn mobile_layout_ios_canonical_names_keep_both_actual_asset_families() {
    use std::io::Read;
    let mut s = Session::new();
    let first = asset(&mut s, "Café", "png");
    let second = asset(&mut s, "Cafe\u{0301}", "png");
    execute(&mut s, "node.set", json!({"ids":[second],"props":{"width":24,"height":16}}));
    let dir = tempfile::tempdir().unwrap();
    let folder = execute(&mut s, "file.exportLayers", json!({"ids":[first,second],"layout":"ios","dir":dir.path().to_string_lossy()}));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2, "canonical aliases must not overwrite an image set on macOS");
    assert_eq!(
        names(&folder),
        [
            "Cafe.imageset/Cafe.png",
            "Cafe.imageset/Cafe@2x.png",
            "Cafe.imageset/Cafe@3x.png",
            "Cafe.imageset/Contents.json",
            "Cafe_2.imageset/Cafe_2.png",
            "Cafe_2.imageset/Cafe_2@2x.png",
            "Cafe_2.imageset/Cafe_2@3x.png",
            "Cafe_2.imageset/Contents.json"
        ]
    );
    let plain = execute(&mut s, "file.exportLayers", json!({"ids":[first,second],"layout":"ios"}));
    let zipped = execute(&mut s, "file.exportLayers", json!({"ids":[first,second],"layout":"ios","zip":true}));
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes(&zipped))).unwrap();
    assert_eq!(archive.len(), 8);
    for (a, b) in folder["files"].as_array().unwrap().iter().zip(plain["files"].as_array().unwrap()) {
        let expected = bytes(b);
        assert_eq!(std::fs::read(a["path"].as_str().unwrap()).unwrap(), expected);
        let mut actual = Vec::new();
        archive.by_name(b["name"].as_str().unwrap()).unwrap().read_to_end(&mut actual).unwrap();
        assert_eq!(actual, expected);
    }
    for (index, size) in [(0, (12, 8)), (4, (24, 16))] {
        let image = image::load_from_memory(&bytes(&plain["files"][index])).unwrap();
        assert_eq!((image.width(), image.height()), size);
    }
}

#[test]
fn mobile_layout_response_budget_is_shared_and_refusal_does_not_consume_it() {
    use super::mobile_export::reserve_response_bytes;
    let mut total = 0usize;
    reserve_response_bytes(&mut total, 6, 10).unwrap();
    reserve_response_bytes(&mut total, 4, 10).unwrap();
    assert_eq!(total, 10);
    assert!(reserve_response_bytes(&mut total, 1, 10).is_err());
    assert_eq!(total, 10);
    total = usize::MAX;
    assert!(reserve_response_bytes(&mut total, 1, usize::MAX).is_err());
    assert_eq!(total, usize::MAX);
}
