//! Original valid progressive CMYK images exercise the PR374 decoder backport. Header-mutated
//! data remains a separate malformed-input/no-panic regression, never a positive JPEG fixture.

use crate::images;

// The fixtures live with the header check in `figcraft-format` (`jpeg.rs`), which tests it there.
include!("../../format/src/jpeg_fixtures.rs");

#[test]
fn the_jpeg_as_written_decodes() {
    let p = images::decode(TINY_PROGRESSIVE).expect("decoded");
    assert_eq!((p.width(), p.height()), (8, 8));
}

#[test]
fn malformed_sampling_headers_do_not_panic() {
    for hv in [0x22, 0x21, 0x12] {
        if let Some(p) = images::decode(&with_second_sampled(hv)) {
            assert_eq!((p.width(), p.height()), (8, 8));
        }
    }
    let mut third = TINY_PROGRESSIVE.to_vec();
    third[SECOND_SAMPLING + 3] = 0x22;
    if let Some(p) = images::decode(&third) {
        assert_eq!((p.width(), p.height()), (8, 8));
    }
}

#[test]
fn baseline_frames_and_other_files_are_left_to_the_decoder() {
    // A well-formed baseline frame with its black sampled finer than its cyan: the decoder takes it.
    let p = images::decode(TINY_BASELINE_CMYK).expect("decoded");
    assert_eq!((p.width(), p.height()), (8, 8));
    assert!(images::decode(&TINY_PROGRESSIVE[..19]).is_none());
}

#[test]
fn progressive_cmyk_with_finer_black_decodes_original_valid_fixtures() {
    for (bytes, width, height, sample) in [
        (include_bytes!("../../format/tests/fixtures/jpeg/progressive-cmyk-8x8.jpg").as_slice(), 8, 8, [102, 104, 101]),
        (include_bytes!("../../format/tests/fixtures/jpeg/progressive-cmyk-300x200.jpg").as_slice(), 300, 200, [64, 64, 64]),
        (include_bytes!("../../format/tests/fixtures/jpeg/progressive-cmyk-17x19.jpg").as_slice(), 17, 19, [64, 64, 64]),
    ] {
        let decoded = images::decode(bytes).expect("valid progressive CMYK must decode");
        assert_eq!((decoded.width(), decoded.height()), (width, height));
        let first = decoded.pixel(0, 0).unwrap();
        assert_eq!(first.alpha(), 255);
        for (got, expected) in [first.red(), first.green(), first.blue()].into_iter().zip(sample) {
            assert!(got.abs_diff(expected) <= 3, "independent libjpeg reference: {got} vs {expected}");
        }
        if width == 8 {
            for (x, y, expected) in [(4, 4, [30, 28, 27]), (6, 6, [35, 77, 85])] {
                let p = decoded.pixel(x, y).unwrap();
                for (got, expected) in [p.red(), p.green(), p.blue()].into_iter().zip(expected) {
                    assert!(got.abs_diff(expected) <= 3, "reference at {x},{y}: {got} vs {expected}");
                }
            }
            // The decoder's existing right/bottom padding differs from libjpeg's fancy
            // upsampling. The backport must retain every baseline pixel, including those edges.
            let baseline = images::decode(TINY_BASELINE_CMYK).unwrap();
            assert_eq!(decoded.data(), baseline.data());
        } else {
            assert!(decoded.pixels().iter().all(|p| p.alpha() == 255 && [p.red(), p.green(), p.blue()].iter().all(|v| v.abs_diff(64) <= 3)));
        }
    }
}

#[test]
fn progressive_cmyk_keeps_decode_budgets_and_rejects_truncated_data() {
    use figcraft_color::ColorProfile;
    let good = include_bytes!("../../format/tests/fixtures/jpeg/progressive-cmyk-300x200.jpg");
    assert!(images::decode_within(good, ColorProfile::Srgb, 300, 300 * 200 * 3 - 1).is_none());
    let (scaled, _) = images::decode_within(good, ColorProfile::Srgb, 15, 16 * 1024 * 1024).unwrap();
    assert_eq!((scaled.width(), scaled.height()), (15, 10));
    for bytes in [&good[..19], &good[..2], b"not a jpeg".as_slice(), &[]] {
        assert!(images::decode(bytes).is_none());
    }
    let mut no_components = good.to_vec();
    let sof = no_components.windows(2).position(|w| w == [0xFF, 0xC2]).unwrap();
    no_components[sof + 9] = 0;
    assert!(images::decode(&no_components).is_none());
}
