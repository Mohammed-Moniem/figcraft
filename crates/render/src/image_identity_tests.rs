//! Importer-local image IDs must never make one document borrow another document's pixels.
use crate::images;
use figcraft_color::{ColorProfile, ImageFilters};
use figcraft_doc::ImageData;
use std::sync::Arc;

fn data(r: u8, g: u8, a: u8) -> ImageData {
    let mut p = tiny_skia::Pixmap::new(2, 2).unwrap();
    p.fill(tiny_skia::Color::from_rgba8(r, g, 0, a));
    ImageData { bytes: Arc::new(crate::encode_png(&p).unwrap()), width: 2, height: 2 }
}

#[test]
fn equal_importer_names_keep_distinct_pixels_and_padded_edges() {
    let (a, b) = (data(255, 0, 255), data(0, 255, 128));
    let (f, p) = (ImageFilters::default(), ColorProfile::Srgb);
    let one = images::get("same-import-id", Some(&a), &f, p).unwrap();
    let two = images::get("same-import-id", Some(&b), &f, p).unwrap();
    assert_ne!(one.data(), two.data());
    assert!(Arc::ptr_eq(&one, &images::get("same-import-id", Some(&a), &f, p).unwrap()));
    assert!(images::get("same-import-id", None, &f, p).is_none());
    let pa = images::padded(&images::cache_key("same-import-id", Some(&a)), &f, p, &one).unwrap();
    let pb = images::padded(&images::cache_key("same-import-id", Some(&b)), &f, p, &two).unwrap();
    assert_ne!(pa.data(), pb.data());
    assert!(images::opaque(&images::cache_key("same-import-id", Some(&a)), &one));
    assert!(!images::opaque(&images::cache_key("same-import-id", Some(&b)), &two));
}

#[test]
fn replacing_mutable_image_bytes_changes_the_identity() {
    let mut a = data(255, 0, 255);
    let before = images::cache_key("import-id", Some(&a));
    // Weak memo ownership forces a new Arc allocation when somebody edits its bytes.
    *Arc::make_mut(&mut a.bytes) = data(0, 255, 255).bytes.as_ref().clone();
    assert_ne!(before, images::cache_key("import-id", Some(&a)));
}
