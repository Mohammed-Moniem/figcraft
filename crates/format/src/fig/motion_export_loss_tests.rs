use figcraft_doc::anim::motion::{AnimProperty, AnimValue, Clip, ClipKind, ClipMode, TimelineSettings, Track};
use figcraft_doc::geom::Vec2;
use figcraft_doc::{Document, NodeId, NodeKind};

fn scene() -> (Document, NodeId) {
    let mut doc = Document::new("Original synthetic motion loss control");
    let page = doc.first_page().unwrap();
    let frame = doc.create(page, NodeKind::Frame, "Viewport", 0.0, 0.0, 100.0, 60.0).unwrap();
    let child = doc.create(frame, NodeKind::Rectangle, "Moving original rectangle", 10.0, 10.0, 20.0, 20.0).unwrap();
    (doc, child)
}

fn assert_motion_warning(warnings: &[String]) {
    let motion: Vec<_> = warnings.iter().filter(|w| w.contains("Motion") && w.contains(".figcraft")).collect();
    assert_eq!(motion.len(), 1, "unsupported native Motion must be reported once with a native-save alternative: {warnings:?}");
    assert!(motion[0].contains("keyframes") && motion[0].contains("clips") && motion[0].contains("timeline"));
}

#[test]
fn fig_export_warns_when_keyframes_are_lost_and_preserves_the_editable_source() {
    let (mut doc, child) = scene();
    let mut track = Track::new(AnimProperty::Position);
    track.set_key(0, AnimValue::Vec2(Vec2::new(10.0, 10.0)), None);
    track.set_key(1000, AnimValue::Vec2(Vec2::new(50.0, 10.0)), None);
    doc.node_mut(child).unwrap().motion.tracks.push(track);
    let page = doc.first_page().unwrap();
    doc.timelines.insert(page, TimelineSettings::default());
    let before = crate::to_json(&doc).unwrap();
    let (bytes, warnings) = super::export(&doc, None).unwrap();
    assert_motion_warning(&warnings);
    assert_eq!(crate::to_json(&doc).unwrap(), before);
    let (back, _) = super::import(&bytes, "Self-reader control").unwrap();
    assert!(back.nodes().all(|n| n.motion.is_empty()));
    assert!(back.timelines.is_empty());
    let (native, _) = crate::from_json(&before.to_string()).unwrap();
    assert_eq!(native.node(child).unwrap().motion, doc.node(child).unwrap().motion);
    assert_eq!(native.timelines, doc.timelines);
}

#[test]
fn fig_export_reports_hidden_clips_without_requiring_a_timeline_entry() {
    let (mut doc, child) = scene();
    let node = doc.node_mut(child).unwrap();
    node.visible = false;
    node.motion.clips.push(Clip::new(ClipKind::Opacity, ClipMode::In));
    let (_, warnings) = super::export(&doc, None).unwrap();
    assert_motion_warning(&warnings);
}

#[test]
fn fig_export_reports_timeline_only_settings_and_keeps_static_exports_quiet() {
    let (mut doc, _) = scene();
    let (_, warnings) = super::export(&doc, None).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    doc.timelines.insert(doc.first_page().unwrap(), TimelineSettings::default());
    let (_, warnings) = super::export(&doc, None).unwrap();
    assert_motion_warning(&warnings);
}
