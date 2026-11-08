use super::*;
use figcraft_color::{Color, Paint};
use figcraft_doc::anim::motion::{AnimProperty, AnimValue, Track};
use figcraft_geom::Vec2;
use image::AnimationDecoder;
use image::codecs::gif::GifDecoder;
use std::io::Cursor;

fn scene() -> (Document, NodeId, NodeId) {
    let mut d = Document::new("Original synthetic motion");
    let page = d.first_page().unwrap();
    let frame = d.create(page, NodeKind::Frame, "Viewport", 40.0, 30.0, 16.0, 12.0).unwrap();
    let child = d.create(frame, NodeKind::Rectangle, "Moving square", 0.0, 0.0, 4.0, 4.0).unwrap();
    d.node_mut(frame).unwrap().fills.clear();
    d.node_mut(child).unwrap().fills = vec![Paint::solid(Color::from_rgba8(255, 0, 0, 255))];
    let mut track = Track::new(AnimProperty::Position);
    track.set_key(0, AnimValue::Vec2(Vec2::new(0.0, 0.0)), None);
    track.set_key(1000, AnimValue::Vec2(Vec2::new(8.0, 0.0)), None);
    d.node_mut(child).unwrap().motion.tracks.push(track);
    (d, frame, child)
}

fn options(fps: u32) -> Options {
    Options { duration_ms: 1000, fps, looping: false, scale: 1.0 }
}

#[test]
fn decoded_frames_move_in_a_fixed_viewport_and_clear_transparent_trails() {
    let (doc, frame, _) = scene();
    let before = serde_json::to_value(&doc).unwrap();
    let out = export(&doc, frame, options(2)).unwrap();
    let frames = GifDecoder::new(Cursor::new(out.bytes)).unwrap().into_frames().collect_frames().unwrap();
    assert_eq!((out.width, out.height, out.frames, out.duration_ms), (16, 12, 2, 1000));
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].buffer().get_pixel(1, 1).0, [255, 0, 0, 255]);
    assert_eq!(frames[1].buffer().get_pixel(1, 1).0[3], 0, "previous square is cleared");
    assert_eq!(frames[1].buffer().get_pixel(5, 1).0, [255, 0, 0, 255]);
    assert_eq!(frames[1].buffer().dimensions(), (16, 12));
    assert_eq!(serde_json::to_value(&doc).unwrap(), before);
}

#[test]
fn cumulative_delay_rounding_preserves_sixty_fps_duration_and_loop_is_explicit() {
    let (doc, frame, _) = scene();
    for looping in [false, true] {
        let out = export(&doc, frame, Options { looping, ..options(60) }).unwrap();
        assert_eq!(out.bytes.windows(11).any(|v| v == b"NETSCAPE2.0"), looping);
        let frames = GifDecoder::new(Cursor::new(out.bytes)).unwrap().into_frames().collect_frames().unwrap();
        let delays: Vec<u32> = frames
            .iter()
            .map(|f| {
                let (n, d) = f.delay().numer_denom_ms();
                n / d
            })
            .collect();
        assert_eq!(delays.len(), 60);
        assert_eq!(delays.iter().sum::<u32>(), 1000);
        assert!(delays.iter().all(|d| [10, 20].contains(d)));
    }
}

#[test]
fn dimensions_and_work_are_refused_before_rendering() {
    let (mut doc, frame, _) = scene();
    assert!(export(&doc, frame, Options { scale: f64::NAN, ..options(2) }).is_err());
    assert!(export(&doc, frame, Options { fps: 101, ..options(2) }).is_err());
    assert!(export(&doc, frame, Options { duration_ms: 600000, ..options(100) }).unwrap_err().contains("frames"));
    assert!(export(&doc, frame, Options { duration_ms: 11, ..options(100) }).unwrap_err().contains("shorter than 10"));
    let node = doc.node_mut(frame).unwrap();
    node.width = 16000.0;
    node.height = 16000.0;
    assert!(export(&doc, frame, options(2)).unwrap_err().contains("pixel budget"));
}

#[test]
fn bounded_writer_includes_finalization_failures() {
    let mut w = BoundedWriter { bytes: Vec::new(), limit: 3, failed: false };
    w.write_all(b"GIF").unwrap();
    assert!(w.write_all(b";").is_err());
    assert!(w.failed);
    assert_eq!(w.bytes, b"GIF");
}

#[test]
fn hidden_track_and_static_frame_keep_the_base_pixels() {
    let (mut doc, frame, child) = scene();
    doc.node_mut(child).unwrap().motion.tracks[0].visible = false;
    let out = export(&doc, frame, options(2)).unwrap();
    let frames = GifDecoder::new(Cursor::new(out.bytes)).unwrap().into_frames().collect_frames().unwrap();
    assert_eq!(frames[0].buffer(), frames[1].buffer());
}

#[test]
fn alpha_is_quantized_explicitly_and_scaled_dimensions_are_stable() {
    let (mut doc, frame, child) = scene();
    doc.node_mut(child).unwrap().motion.tracks.clear();
    for alpha in [64, 192] {
        doc.node_mut(child).unwrap().fills = vec![Paint::solid(Color::from_rgba8(255, 0, 0, alpha))];
        let out = export(&doc, frame, Options { scale: 2.0, ..options(2) }).unwrap();
        assert_eq!((out.width, out.height), (32, 24));
        let frames = GifDecoder::new(Cursor::new(out.bytes)).unwrap().into_frames().collect_frames().unwrap();
        assert_eq!(frames[0].buffer().get_pixel(2, 2).0[3], if alpha < 128 { 0 } else { 255 });
    }
}

#[test]
fn cyclic_and_deep_subtrees_return_errors() {
    let (mut doc, frame, _) = scene();
    doc.node_mut(frame).unwrap().children.push(frame);
    assert!(export(&doc, frame, options(2)).unwrap_err().contains("cyclic or repeated"));
}

#[test]
fn hidden_cycles_and_duplicate_references_are_refused_before_player_construction() {
    let (mut doc, frame, child) = scene();
    doc.node_mut(child).unwrap().visible = false;
    doc.node_mut(child).unwrap().children.push(child);
    let start = std::time::Instant::now();
    assert!(export(&doc, frame, Options { duration_ms: 12000, ..options(100) }).unwrap_err().contains("cyclic or repeated"));
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    doc.node_mut(child).unwrap().children.clear();
    doc.node_mut(frame).unwrap().children.push(child);
    assert!(export(&doc, frame, options(2)).unwrap_err().contains("cyclic or repeated"));
}

#[test]
fn hidden_depth_and_page_visit_limits_are_refused() {
    let (mut doc, frame, child) = scene();
    doc.node_mut(child).unwrap().visible = false;
    let page = doc.first_page().unwrap();
    assert!(check_motion_tree(&doc, page, 2).unwrap_err().contains("exceeds 2 nodes"));
    let mut parent = doc.create(frame, NodeKind::Group, "Hidden group", 0.0, 0.0, 1.0, 1.0).unwrap();
    doc.node_mut(parent).unwrap().visible = false;
    for _ in 0..crate::MAX_EXPORT_DEPTH {
        parent = doc.create(parent, NodeKind::Group, "Hidden descendant", 0.0, 0.0, 1.0, 1.0).unwrap();
    }
    assert!(export(&doc, frame, options(2)).unwrap_err().contains("maximum export depth"));
}

#[test]
fn valid_hidden_animation_stays_hidden_and_invalid_parent_links_are_refused() {
    let (mut doc, frame, child) = scene();
    doc.node_mut(child).unwrap().visible = false;
    let out = export(&doc, frame, options(2)).unwrap();
    let frames = GifDecoder::new(Cursor::new(out.bytes)).unwrap().into_frames().collect_frames().unwrap();
    assert!(frames.iter().all(|f| f.buffer().pixels().all(|p| p.0[3] == 0)));
    doc.node_mut(child).unwrap().parent = Some(child);
    assert!(export(&doc, frame, options(2)).unwrap_err().contains("inconsistent parent"));
}

#[test]
fn compressed_output_cap_includes_the_encoders_trailer() {
    let (doc, frame, _) = scene();
    let full = export(&doc, frame, options(2)).unwrap();
    assert!(export_with_limit(&doc, frame, options(2), full.bytes.len() - 1).is_err());
    assert!(export_with_limit(&doc, frame, options(2), full.bytes.len()).is_ok());
}

#[test]
fn total_pixel_and_node_frame_work_limits_are_enforced() {
    let (mut doc, frame, _) = scene();
    let node = doc.node_mut(frame).unwrap();
    node.width = 4000.0;
    node.height = 4000.0;
    assert!(export(&doc, frame, options(100)).unwrap_err().contains("pixel budget"));
    let node = doc.node_mut(frame).unwrap();
    node.width = 16.0;
    node.height = 12.0;
    for _ in 0..17000 {
        doc.create(frame, NodeKind::Rectangle, "Control", 0.0, 0.0, 1.0, 1.0).unwrap();
    }
    assert!(export(&doc, frame, Options { duration_ms: 12000, ..options(100) }).unwrap_err().contains("node-frames"));
}

#[test]
fn embedded_gif_media_advances_with_the_export_clock() {
    use figcraft_color::ImageScaleMode;
    use figcraft_doc::ImageData;
    use std::sync::Arc;
    let (mut doc, frame, child) = scene();
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        for rgba in [[255, 0, 0, 255], [0, 255, 0, 255]] {
            encoder
                .encode_frame(Frame::from_parts(RgbaImage::from_pixel(4, 4, image::Rgba(rgba)), 0, 0, Delay::from_numer_denom_ms(500, 1)))
                .unwrap();
        }
    }
    let key = "original-synthetic-gif-for-motion-export".to_string();
    doc.images.insert(key.clone(), ImageData { bytes: Arc::new(bytes), width: 4, height: 4 });
    let node = doc.node_mut(child).unwrap();
    node.motion.tracks.clear();
    node.fills = vec![Paint::image(key, ImageScaleMode::Fill)];
    let out = export(&doc, frame, options(2)).unwrap();
    let frames = GifDecoder::new(Cursor::new(out.bytes)).unwrap().into_frames().collect_frames().unwrap();
    assert_eq!(frames[0].buffer().get_pixel(1, 1).0, [255, 0, 0, 255]);
    assert_eq!(frames[1].buffer().get_pixel(1, 1).0, [0, 255, 0, 255]);
}
