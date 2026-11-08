//! Sampling-header inspection, malformed input safety, and original valid CMYK thumbnail fixtures.

use std::io::Cursor;

use crate::jpeg::finer_than_first;
use crate::thumbnail::{archive_preview, decode_thumbnail};

include!("jpeg_fixtures.rs");

#[test]
fn the_jpeg_as_written_is_not_finer_than_its_first() {
    assert_eq!(TINY_PROGRESSIVE[SECOND_SAMPLING], 0x11);
    assert!(!finer_than_first(TINY_PROGRESSIVE));
    // Chroma sampled coarser than luma (the usual way round) isn't finer either.
    assert!(!finer_than_first(&{
        let mut b = TINY_PROGRESSIVE.to_vec();
        b[SECOND_SAMPLING - 3] = 0x22;
        b
    }));
}

#[test]
fn a_progressive_jpeg_sampling_a_component_finer_than_its_first_is_caught_from_its_header() {
    for hv in [0x22, 0x21, 0x12] {
        assert!(finer_than_first(&with_second_sampled(hv)), "second component {hv:#x}");
    }
    let mut third = TINY_PROGRESSIVE.to_vec();
    third[SECOND_SAMPLING + 3] = 0x22;
    assert!(finer_than_first(&third));
}

#[test]
fn baseline_frames_and_other_files_are_not_caught() {
    // A well-formed baseline frame with its black sampled finer than its cyan: the decoder takes it.
    assert!(!finer_than_first(TINY_BASELINE_CMYK));
    // Not JPEGs, or JPEGs that end before their frame.
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(4, 4, image::Rgba([1, 2, 3, 255])).write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
    assert!(!finer_than_first(&png));
    assert!(!finer_than_first(&TINY_PROGRESSIVE[..19]));
    assert!(!finer_than_first(&[0xFF, 0xD8]));
    assert!(!finer_than_first(&[]));
}

#[test]
fn malformed_sampling_thumbnails_do_not_panic_and_truncated_archives_have_no_picture() {
    for hv in [0x22, 0x21, 0x12] {
        if let Some(p) = decode_thumbnail(&with_second_sampled(hv), 64) {
            assert_eq!((p.width, p.height), (8, 8));
        }
    }
    let mut third = TINY_PROGRESSIVE.to_vec();
    third[SECOND_SAMPLING + 3] = 0x22;
    if let Some(p) = decode_thumbnail(&third, 64) {
        assert_eq!((p.width, p.height), (8, 8));
    }
    let mut zip = Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(&mut zip);
    z.start_file("thumbnail.png", zip::write::SimpleFileOptions::default()).unwrap();
    std::io::Write::write_all(&mut z, &TINY_PROGRESSIVE[..19]).unwrap();
    z.finish().unwrap();
    assert_eq!(archive_preview(Cursor::new(zip.into_inner()), 64).picture, None);
    // The well-formed ones still give their picture.
    for good in [TINY_PROGRESSIVE, TINY_BASELINE_CMYK] {
        let t = decode_thumbnail(good, 64).expect("decoded");
        assert_eq!((t.width, t.height), (8, 8));
    }
}

#[test]
fn progressive_cmyk_with_finer_black_has_thumbnail_and_archive_preview() {
    for (bytes, width, height) in [
        (include_bytes!("../tests/fixtures/jpeg/progressive-cmyk-8x8.jpg").as_slice(), 8, 8),
        (include_bytes!("../tests/fixtures/jpeg/progressive-cmyk-300x200.jpg").as_slice(), 300, 200),
        (include_bytes!("../tests/fixtures/jpeg/progressive-cmyk-17x19.jpg").as_slice(), 17, 19),
    ] {
        let picture = decode_thumbnail(bytes, 512).expect("valid progressive CMYK thumbnail");
        assert_eq!((picture.width, picture.height), (width, height));
        assert!(picture.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
        let expected = if width == 8 { [102, 104, 101] } else { [64, 64, 64] };
        for (&got, expected) in picture.rgba[..3].iter().zip(expected) {
            assert!(got.abs_diff(expected) <= 3, "independent libjpeg reference: {got} vs {expected}");
        }
        if width != 8 {
            assert!(picture.rgba.chunks_exact(4).all(|p| p[..3].iter().all(|v| v.abs_diff(64) <= 3)));
        }
        let mut archive = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut archive);
        writer.start_file("thumbnail.png", zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut writer, bytes).unwrap();
        writer.finish().unwrap();
        let preview = archive_preview(Cursor::new(archive.into_inner()), 512).picture.expect("archive picture");
        assert_eq!(preview, picture);
    }
}
