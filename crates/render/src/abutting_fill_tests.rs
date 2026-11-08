//! Original #544 pixel fixtures: adjacent artwork must cover its shared interior edge once.
use super::*;
use figcraft_color::Paint;

fn keep_png(name: &str, png: &[u8]) {
    use std::io::Write;
    let Some(directory) = std::env::var_os("FIGCRAFT_SEAM_EVIDENCE_DIR") else { return };
    let path = std::path::Path::new(&directory).join(format!("{name}.png"));
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap();
    file.write_all(png).unwrap();
}

fn scene(tiled: bool) -> (Document, NodeId, Vec<NodeId>) {
    let mut doc = Document::new("Original abutting-square coverage fixture");
    let page = doc.first_page().unwrap();
    let boxes = if tiled { vec![(0.0, 0.0, 20.0), (20.0, 0.0, 20.0), (0.0, 20.0, 20.0), (20.0, 20.0, 20.0)] } else { vec![(0.0, 0.0, 40.0)] };
    let mut ids = Vec::new();
    for (index, (x, y, side)) in boxes.into_iter().enumerate() {
        let id = doc.create(page, NodeKind::Rectangle, &format!("Original square {index}"), x, y, side, side).unwrap();
        doc.get_mut(id).unwrap().fills = vec![Paint::solid(Color::BLACK)];
        ids.push(id);
    }
    (doc, page, ids)
}

fn view(scale: f64) -> View {
    let size = (40.0 * scale).ceil() as u32;
    View { origin: (0.0, 0.0), scale, width: size, height: size }
}

fn assert_solid_interior(bytes: &[u8], width: u32, height: u32, scale: f64) {
    let mut wrong = Vec::new();
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let at = ((y * width + x) * 4) as usize;
            let pixel = &bytes[at..at + 4];
            if pixel != [0, 0, 0, 255] && wrong.len() < 8 {
                wrong.push((x, y, pixel.to_vec()));
            }
        }
    }
    assert!(wrong.is_empty(), "scale{scale}: opaque black interior contains shared-edge gaps: {wrong:?}");
}

fn canvas(scale: f64) {
    let (doc, page, _) = scene(true);
    let pix = render_view(&doc, page, &view(scale), &Options { background: Some(Color::WHITE), ..Default::default() }).unwrap();
    keep_png(&format!("canvas-{scale}"), &encode_png(&pix).unwrap());
    assert_solid_interior(pix.data(), pix.width(), pix.height(), scale);
}

fn png_export(scale: f64) {
    let (doc, _, ids) = scene(true);
    let pix = render_area(&doc, &ids, Rect::new(0.0, 0.0, 40.0, 40.0), scale).unwrap();
    let png = encode_png(&pix).unwrap();
    keep_png(&format!("export-{scale}"), &png);
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_solid_interior(decoded.as_raw(), decoded.width(), decoded.height(), scale);
}

#[test]
fn canvas_abutting_fills_cover_the_shared_edge_at_scale_137() {
    canvas(1.37);
}

#[test]
fn canvas_abutting_fills_cover_the_shared_edge_at_scale_073() {
    canvas(0.73);
}

#[test]
fn png_abutting_fills_cover_the_shared_edge_at_scale_137() {
    png_export(1.37);
}

#[test]
fn png_abutting_fills_cover_the_shared_edge_at_scale_073() {
    png_export(0.73);
}

#[test]
fn integral_shared_edges_and_one_piece_fractional_reference_are_solid() {
    for scale in [1.0, 2.5] {
        canvas(scale);
        png_export(scale);
    }
    let (doc, page, _) = scene(false);
    for scale in [1.0, 2.5, 1.37, 0.73] {
        let pix = render_view(&doc, page, &view(scale), &Options { background: Some(Color::WHITE), ..Default::default() }).unwrap();
        assert_solid_interior(pix.data(), pix.width(), pix.height(), scale);
    }
}

fn assert_reference_coverage(tiled: &Pixmap, single: &Pixmap) {
    assert_eq!(tiled.width(), single.width());
    assert_eq!(tiled.height(), single.height());
    let mut opaque = 0;
    for y in 1..single.height() - 1 {
        for x in 1..single.width() - 1 {
            let at = ((y * single.width() + x) * 4) as usize;
            // A one-pixel neighbourhood wholly inside the reference excludes its outer AA
            // boundary, where one rectangle and a compound path can quantize differently.
            let interior = (y - 1..=y + 1).all(|row| {
                (x - 1..=x + 1).all(|column| {
                    let p = ((row * single.width() + column) * 4) as usize;
                    single.data()[p..p + 4] == [0, 0, 0, 255]
                })
            });
            if !interior {
                continue;
            }
            opaque += 1;
            assert_eq!(&tiled.data()[at..at + 4], &[0, 0, 0, 255], "shared interior at pixel {},{}", x, y);
        }
    }
    assert!(opaque > 300, "the reference must contain a substantial opaque interior");
}

#[test]
fn translated_rotated_and_reflected_abutting_rectangles_cover_the_reference_interior() {
    let transforms =
        [Affine::translate((0.31, -0.27)), Affine::translate((40.31, -0.27)) * Affine::scale_non_uniform(-1.0, 1.0), Affine::rotate(0.23)];
    let v = View { origin: (-15.0, -15.0), scale: 1.37, width: 100, height: 100 };
    for (index, transform) in transforms.into_iter().enumerate() {
        let image = |tiled| {
            let (mut doc, page, ids) = scene(tiled);
            for id in ids {
                let node = doc.get_mut(id).unwrap();
                node.transform = transform * node.transform;
            }
            render_view(&doc, page, &v, &Options { background: Some(Color::TRANSPARENT), ..Default::default() }).unwrap()
        };
        let tiled = image(true);
        keep_png(&format!("transformed-{index}"), &encode_png(&tiled).unwrap());
        assert_reference_coverage(&tiled, &image(false));
    }
}

#[test]
fn a_clipped_frame_keeps_shared_coverage_inside_and_transparency_outside() {
    let image = |tiled| {
        let (mut doc, page, ids) = scene(tiled);
        let frame = doc.create(page, NodeKind::Frame, "Original clipping fixture", 0.0, 0.0, 30.0, 30.0).unwrap();
        doc.get_mut(frame).unwrap().fills.clear();
        doc.get_mut(frame).unwrap().clip = true;
        for id in ids {
            doc.reparent(id, frame, None).unwrap();
        }
        render_view(&doc, page, &view(1.37), &Options { background: Some(Color::TRANSPARENT), ..Default::default() }).unwrap()
    };
    let tiled = image(true);
    let single = image(false);
    assert_reference_coverage(&tiled, &single);
    // More than two pixels beyond the fractional clip, nothing paints.
    let at = ((45 * tiled.width() + 45) * 4) as usize;
    assert_eq!(&tiled.data()[at..at + 4], &[0, 0, 0, 0]);
}

#[test]
fn translucent_distinct_stroked_and_multiple_fills_keep_separate_paint_compositing() {
    for case in 0..5 {
        let (mut doc, _, ids) = scene(true);
        for (index, id) in ids.iter().enumerate() {
            let node = doc.get_mut(*id).unwrap();
            match case {
                0 => node.fills[0].opacity = 0.5,
                1 => node.opacity = 0.5,
                2 => {
                    node.fills =
                        vec![Paint::solid(if index % 2 == 0 { Color::from_rgb8_const(220, 20, 40) } else { Color::from_rgb8_const(30, 60, 220) })]
                }
                3 => node.strokes = vec![Paint::solid(Color::from_rgb8_const(220, 20, 40))],
                _ => node.fills.push(Paint::solid(Color::from_rgb8_const(20, 180, 90))),
            }
        }
        let v = view(1.37);
        let actual = render_area(&doc, &ids, Rect::new(0.0, 0.0, 40.0, 40.0), v.scale).unwrap();
        // Independent one-node calls retain ordered per-layer source-over compositing.
        let mut reference = Pixmap::new(v.width, v.height).unwrap();
        for id in &ids {
            render_nodes_into(&doc, &[*id], &v, &mut reference);
        }
        assert_eq!(actual.data(), reference.data(), "ineligible paint case {case}");
    }
}

#[test]
fn intervening_different_artwork_keeps_its_place_in_the_stack() {
    let (mut doc, page, _) = scene(true);
    let red = doc.create(page, NodeKind::Rectangle, "Original red intervening artwork", 15.0, 0.0, 10.0, 40.0).unwrap();
    doc.get_mut(red).unwrap().fills = vec![Paint::solid(Color::from_rgb8_const(220, 20, 40))];
    doc.reparent(red, page, Some(1)).unwrap();
    let before = serde_json::to_vec(&doc).unwrap();
    let pix = render_view(&doc, page, &view(1.37), &Options::default()).unwrap();
    let pixel = |x: f64, y: f64| {
        let at = (((y * 1.37) as u32 * pix.width() + (x * 1.37) as u32) * 4) as usize;
        pix.data()[at..at + 4].to_vec()
    };
    assert_eq!(pixel(18.0, 10.0), [220, 20, 40, 255]);
    assert_eq!(pixel(23.0, 10.0), [0, 0, 0, 255]);
    assert_eq!(pixel(18.0, 30.0), [0, 0, 0, 255]);
    assert_eq!(serde_json::to_vec(&doc).unwrap(), before, "rendering never edits artwork");
}

#[test]
fn masks_preserve_shared_interior_and_their_transparent_exterior() {
    for mask_type in [figcraft_doc::MaskType::Alpha, figcraft_doc::MaskType::Vector, figcraft_doc::MaskType::Luminance] {
        let image = |tiled| {
            let (mut doc, page, ids) = scene(tiled);
            let group = doc.create(page, NodeKind::Group, "Original masked tiles", 0.0, 0.0, 40.0, 40.0).unwrap();
            let mask = doc.create(group, NodeKind::Rectangle, "Original mask", 0.0, 0.0, 30.0, 30.0).unwrap();
            let node = doc.get_mut(mask).unwrap();
            node.fills = vec![Paint::solid(Color::WHITE)];
            node.is_mask = true;
            node.mask_type = mask_type;
            for id in ids {
                doc.reparent(id, group, None).unwrap();
            }
            render_view(&doc, page, &view(1.37), &Options { background: Some(Color::TRANSPARENT), ..Default::default() }).unwrap()
        };
        let tiled = image(true);
        assert_reference_coverage(&tiled, &image(false));
        let at = ((45 * tiled.width() + 45) * 4) as usize;
        assert_eq!(&tiled.data()[at..at + 4], &[0, 0, 0, 0], "outside {mask_type:?} mask");
    }
}

#[test]
fn skip_solo_and_outline_keep_their_selection_semantics() {
    let (doc, page, ids) = scene(true);
    let v = view(1.37);
    let options = Options { background: Some(Color::TRANSPARENT), ..Default::default() };
    let mut hidden = doc.clone();
    hidden.get_mut(ids[1]).unwrap().visible = false;
    let skipped = render_view(&doc, page, &v, &Options { skip: vec![ids[1]], ..options.clone() }).unwrap();
    let reference = render_view(&hidden, page, &v, &options).unwrap();
    assert_eq!(skipped.data(), reference.data(), "skip never rejoins through the omitted tile");
    let solo = render_view(&doc, page, &v, &Options { solo: Some(ids[0]), ..options.clone() }).unwrap();
    let mut single = Pixmap::new(v.width, v.height).unwrap();
    render_nodes_into(&doc, &ids[..1], &v, &mut single);
    assert_eq!(solo.data(), single.data());
    let outline = render_view(&doc, page, &v, &Options { outline_mode: true, ..options.clone() }).unwrap();
    // Draw each outline directly, as the pre-batching compositor did. Rendering each first to
    // an intermediate pixmap and compositing it adds an extra rounding step at shared edges.
    let bounds = draw::Bounds::default();
    let ctx = draw::Ctx {
        doc: &doc,
        scale: v.scale,
        outline_mode: true,
        outline: Outline { light: Color::TRANSPARENT.is_dark(), ..Default::default() },
        outlined: &[],
        skip: &[],
        lod: false,
        preview: false,
        magnified_image_pixels: false,
        under: None,
        time: 0.0,
        bounds: &bounds,
        page_at: (v.origin.0 * v.scale, v.origin.1 * v.scale),
        cache: None,
        images: 0,
        accelerate: false,
    };
    let mut ordinary = Pixmap::new(v.width, v.height).unwrap();
    for id in ids {
        draw::draw_node(&ctx, &mut ordinary, id, v.transform(), 0);
    }
    assert!(outline.data() == ordinary.data(), "outline keeps each tile's internal edges exactly");
}

#[test]
fn retained_bands_and_pan_match_fresh_coverage_with_canvas_options() {
    use std::sync::Arc;
    let (doc, page, _) = scene(true);
    let doc = Arc::new(doc);
    let opts = Options { background: Some(Color::WHITE), lod: true, ..Default::default() };
    let v = View { origin: (-4.31, -3.27), scale: 1.37, width: 70, height: 70 };
    let mut retained = Retained::default();
    let (shown, bands) = match retained.begin(&doc, page, &v, &opts, 7).unwrap() {
        Begun::Steps(mut steps) => {
            let mut count = 1;
            while !steps.step() {
                count += 1;
                assert!(count < 100, "bounded band work");
            }
            assert!(count > 2);
            let (shown, pixels) = retained.finish(*steps);
            (shown, pixels.clone())
        }
        Begun::Done(_, _, _) => panic!("plain rectangle scene must use the requested bands"),
    };
    assert_eq!(bands.data(), render_view(&doc, page, &shown, &opts).unwrap().data());
    let panned = View { origin: (1.31, 2.27), ..v };
    let (shown, pixels, _) = retained.render(&doc, page, &panned, &opts).unwrap();
    assert_eq!(pixels.data(), render_view(&doc, page, &shown, &opts).unwrap().data());
}

#[test]
fn thousands_of_offscreen_rectangles_do_not_change_visible_coverage() {
    let (mut doc, page, _) = scene(true);
    let v = view(1.37);
    let opts = Options { background: Some(Color::WHITE), ..Default::default() };
    let reference = render_view(&doc, page, &v, &opts).unwrap();
    // This exceeds a batch and includes distant content in every direction without a timing
    // assertion tied to a particular computer. The outer runner bounds the complete test job.
    for index in 0..5000 {
        let x = if index % 2 == 0 { 10_000.0 } else { -10_000.0 };
        let id = doc.create(page, NodeKind::Rectangle, "Original distant tile", x, f64::from(index), 20.0, 20.0).unwrap();
        doc.get_mut(id).unwrap().fills = vec![Paint::solid(Color::BLACK)];
    }
    assert_eq!(render_view(&doc, page, &v, &opts).unwrap().data(), reference.data());
}

#[test]
fn reflected_opaque_overlaps_cannot_cancel_their_coverage() {
    let (mut doc, page, ids) = scene(true);
    doc.get_mut(ids[1]).unwrap().transform = Affine::translate((30.0, 0.0)) * Affine::scale_non_uniform(-1.0, 1.0);
    let pix = render_view(&doc, page, &view(1.37), &Options { background: Some(Color::WHITE), ..Default::default() }).unwrap();
    let at = ((13 * pix.width() + 20) * 4) as usize;
    assert_eq!(&pix.data()[at..at + 4], &[0, 0, 0, 255], "opposite transform orientation must not punch a hole into overlap");
}

#[test]
fn reversed_child_order_keeps_intervening_artwork_in_its_place() {
    let (mut doc, page, ids) = scene(true);
    let group = doc.create(page, NodeKind::Group, "Original reversed stack", 0.0, 0.0, 40.0, 40.0).unwrap();
    doc.get_mut(group).unwrap().layout = Some(figcraft_doc::AutoLayout { reverse_z: true, ..Default::default() });
    for id in ids {
        doc.reparent(id, group, None).unwrap();
    }
    let red = doc.create(group, NodeKind::Rectangle, "Original red intervening layer", 15.0, 0.0, 10.0, 40.0).unwrap();
    doc.get_mut(red).unwrap().fills = vec![Paint::solid(Color::from_rgb8_const(220, 20, 40))];
    doc.reparent(red, group, Some(1)).unwrap();
    let pix = render_view(&doc, page, &view(1.37), &Options::default()).unwrap();
    let pixel = |x: f64| {
        let at = ((13 * pix.width() + (x * 1.37) as u32) * 4) as usize;
        &pix.data()[at..at + 4]
    };
    assert_eq!(pixel(18.0), &[0, 0, 0, 255]);
    assert_eq!(pixel(23.0), &[220, 20, 40, 255]);
}
