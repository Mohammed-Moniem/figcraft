//! Original #1681 phase contracts. JSON keeps these tests compilable on the pre-feature model.

use crate::Session;
use crate::params::id_of;
use figcraft_doc::{NodeId, StrokeAlign, StrokeProps, WidthProfile};
use figcraft_geom::kurbo::Shape;
use figcraft_geom::{BezPath, Point, Rect};
use serde_json::{Value, json};
use std::io::Read;

fn props(phase: Value) -> StrokeProps {
    serde_json::from_value(json!({"weight":4,"align":"center","dashes":[10,10],"dash_phase":phase})).unwrap()
}

fn path() -> BezPath {
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    p.line_to((100.0, 0.0));
    p
}

fn ink(outline: &BezPath, x: f64) -> bool {
    outline.winding(Point::new(x, 0.0)) != 0
}

fn scene() -> (Session, NodeId, NodeId) {
    let mut s = Session::new();
    let frame = id_of(&s.execute("frame.create", &json!({"width":100,"height":20,"props":{"fill":"#FFFFFF"}})).unwrap()["id"]).unwrap();
    let line = id_of(&s.execute("vector.create", &json!({"parent":frame,"path":"M0 0 L100 0","x":0,"y":10})).unwrap()["id"]).unwrap();
    s.execute("node.set", &json!({"id":line,"props":{"fill":null,"strokes":["#000000"],"strokeWeight":4,"strokeAlign":"center","dashes":[10,10]}}))
        .unwrap();
    (s, frame, line)
}

fn raw_phase(s: &mut Session, id: NodeId, phase: Value) {
    let n = s.doc_mut().unwrap().get_mut(id).unwrap();
    let mut stroke = serde_json::to_value(&n.stroke).unwrap();
    stroke["dash_phase"] = phase;
    n.stroke = serde_json::from_value(stroke).unwrap();
}

#[test]
fn manual_zero_keeps_raw_lengths_instead_of_automatic_half_ends() {
    let p = props(json!(0));
    let out = figcraft_pathops::stroke_center(&path(), p.weight, &p);
    assert!(ink(&out, 8.0), "manual0: first dash covers0..10; automatic has already entered its gap");
    assert!(!ink(&out, 12.0));
    assert!(ink(&out, 28.0));
}

#[test]
fn signed_manual_phase_moves_into_dash_or_gap_and_wraps_by_full_periods() {
    for phase in [12.0, -8.0, 32.0, -28.0] {
        let p = props(json!(phase));
        let out = figcraft_pathops::stroke_center(&path(), p.weight, &p);
        assert!(!ink(&out, 2.0), "{phase}: the first8 units are a gap");
        assert!(ink(&out, 12.0));
        assert!(!ink(&out, 22.0));
    }
}

#[test]
fn variable_width_manual_phase_uses_the_same_start_as_uniform_strokes() {
    let p = props(json!(0));
    let out = figcraft_pathops::width::variable_outline(&path(), p.weight, &p, &WidthProfile::uniform()).unwrap();
    assert!(ink(&out, 8.0));
    assert!(!ink(&out, 12.0));
}

#[test]
fn public_phase_set_inspect_clear_and_undo_keep_the_original_pattern() {
    let (mut s, _, line) = scene();
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    s.execute("node.set", &json!({"id":line,"props":{"dashPhase":-8}})).unwrap();
    assert_eq!(s.execute("node.get", &json!({"id":line})).unwrap()["dashPhase"].as_f64(), Some(-8.0));
    assert_eq!(s.doc().unwrap().node(line).unwrap().stroke.dashes, [10.0, 10.0]);
    assert_eq!(s.undo_depth().0, history.0 + 1);
    let after = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &after);
    s.execute("node.set", &json!({"id":line,"props":{"dashPhase":null}})).unwrap();
    assert!(s.execute("node.get", &json!({"id":line})).unwrap().get("dashPhase").is_none());
}

#[test]
fn manual_phase_native_storage_is_optional_and_survives_reopen() {
    let (mut s, _, line) = scene();
    let original = figcraft_format::to_json(s.doc().unwrap()).unwrap().to_string();
    assert!(!original.contains("dash_phase"));
    raw_phase(&mut s, line, json!(-3.5));
    let (back, _) = figcraft_format::load_bytes(&figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(&back.node(line).unwrap().stroke).unwrap()["dash_phase"], -3.5);
}

#[test]
fn manual_phase_scales_as_a_length_with_the_original_dash_array() {
    let (mut s, _, line) = scene();
    raw_phase(&mut s, line, json!(8));
    s.execute("transform.scale", &json!({"ids":[line],"factor":2})).unwrap();
    let stroke = &s.doc().unwrap().node(line).unwrap().stroke;
    assert_eq!(stroke.dashes, [20.0, 20.0]);
    assert_eq!(serde_json::to_value(stroke).unwrap()["dash_phase"].as_f64(), Some(16.0));
}

#[test]
fn manual_phase_png_export_contains_the_requested_start() {
    use base64::Engine as _;
    let (mut s, frame, line) = scene();
    raw_phase(&mut s, line, json!(12));
    let png = s.execute("file.export", &json!({"ids":[frame],"format":"png","includeBoundingBox":true})).unwrap();
    let bytes = base64::engine::general_purpose::STANDARD.decode(png["data"].as_str().unwrap()).unwrap();
    let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(image.dimensions(), (100, 20));
    assert!(image.get_pixel(2, 10).0[0] > 240, "manual12 starts in a gap, unlike automatic");
    assert!(image.get_pixel(12, 10).0[0] < 20);
}

#[test]
fn manual_phase_svg_export_contains_the_requested_offset_and_raw_pattern() {
    let (mut s, frame, line) = scene();
    raw_phase(&mut s, line, json!(12));
    let svg = figcraft_svg::export_svg(s.doc().unwrap(), &[frame], &Default::default()).unwrap();
    assert!(svg.contains("stroke-dasharray=\"10 10\"") && svg.contains("stroke-dashoffset=\"12\""), "{svg}");
}

#[test]
fn manual_phase_svg_import_without_offset_keeps_svg_zero_instead_of_auto_fit() {
    let mut doc = figcraft_doc::Document::new("Original SVG phase0");
    let page = doc.first_page().unwrap();
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="20"><path d="M0 10L100 10" fill="none" stroke="#000" stroke-width="4" stroke-dasharray="10 10"/></svg>"##;
    figcraft_svg::import_svg(&mut doc, svg, page, 0.0, 0.0, "Original raw dashes").unwrap();
    let n = doc.nodes().find(|n| !n.stroke.dashes.is_empty()).unwrap();
    assert_eq!(n.stroke.dashes, [10.0, 10.0]);
    assert_eq!(serde_json::to_value(&n.stroke).unwrap()["dash_phase"].as_f64(), Some(0.0), "SVG's omitted offset still means manual0");
    let out = figcraft_pathops::stroke_center(&path(), n.stroke.weight, &n.stroke);
    assert!(ink(&out, 8.0));
    assert!(!ink(&out, 12.0));
}

fn pdf_streams(pdf: &[u8]) -> String {
    let mut text = String::new();
    let mut offset = 0;
    while let Some(start) = pdf[offset..].windows(7).position(|w| w == b"stream\n") {
        let start = offset + start + 7;
        let Some(end) = pdf[start..].windows(9).position(|w| w == b"endstream") else { break };
        let mut bytes = Vec::new();
        if flate2::read::ZlibDecoder::new(&pdf[start..start + end]).take(1024 * 1024).read_to_end(&mut bytes).is_ok() {
            text.push_str(&String::from_utf8_lossy(&bytes));
        }
        offset = start + end + 9;
    }
    text
}

#[test]
fn manual_phase_pdf_contains_the_offset_dash_outline() {
    let (mut s, _, line) = scene();
    raw_phase(&mut s, line, json!(12));
    let opts = figcraft_pdf::PdfOptions { area: Some(Rect::new(0.0, 0.0, 100.0, 20.0)), ..Default::default() };
    let pdf = figcraft_pdf::export_pdf(s.doc().unwrap(), &[line], &opts).unwrap();
    let streams = pdf_streams(&pdf);
    assert!(!streams.is_empty());
    let tokens: Vec<_> = streams.split_whitespace().collect();
    assert!(tokens.windows(3).any(|w| w[0] == "8" && w[2] == "m"), "manual12's first filled dash begins at8: {streams}");
}

#[test]
fn automatic_old_strokes_keep_the_original_half_dash_placement() {
    let p = StrokeProps { weight: 4.0, align: StrokeAlign::Center, dashes: vec![10.0, 10.0], ..Default::default() };
    let out = figcraft_pathops::stroke_center(&path(), p.weight, &p);
    assert!(ink(&out, 2.0));
    assert!(!ink(&out, 8.0));
    assert!(ink(&out, 98.0));
    assert!(serde_json::to_value(&p).unwrap().get("dash_phase").is_none());
}

#[test]
fn manual_phase_invalid_command_values_leave_document_and_history_unchanged() {
    let (mut s, _, line) = scene();
    for value in [json!("NaN"), json!("inf"), json!(true), json!([]), json!({})] {
        let before = s.doc().unwrap().clone();
        let history = s.undo_depth();
        assert!(s.execute("node.set", &json!({"id":line,"props":{"dashPhase":value}})).is_err());
        assert_eq!(s.doc().unwrap(), &before);
        assert_eq!(s.undo_depth(), history);
    }
}

#[test]
fn manual_phase_property_copy_and_clear_keep_null_distinct_from_zero() {
    let (mut s, frame, line) = scene();
    let other = id_of(&s.execute("shape.rectangle", &json!({"parent":frame,"width":20,"height":10})).unwrap()["id"]).unwrap();
    s.execute("node.set", &json!({"id":line,"props":{"dashPhase":0}})).unwrap();
    s.execute("edit.copyProperties", &json!({"id":line})).unwrap();
    s.execute("edit.pasteProperties", &json!({"ids":[other]})).unwrap();
    assert_eq!(s.doc().unwrap().node(other).unwrap().stroke.dash_phase, Some(0.0));
    s.execute("node.set", &json!({"id":line,"props":{"dashPhase":null}})).unwrap();
    s.execute("edit.copyProperties", &json!({"id":line})).unwrap();
    s.execute("edit.pasteProperties", &json!({"ids":[other]})).unwrap();
    assert_eq!(s.doc().unwrap().node(other).unwrap().stroke.dash_phase, None);
}

#[test]
fn manual_phase_instance_override_reset_and_ordinary_resize_preserve_ownership() {
    let (mut s, frame, line) = scene();
    s.execute("node.set", &json!({"id":line,"props":{"dashPhase":3}})).unwrap();
    let component = id_of(&s.execute("component.create", &json!({"ids":[frame]})).unwrap()["id"]).unwrap();
    let instance = id_of(&s.execute("instance.create", &json!({"component":component,"x":200,"y":0})).unwrap()["id"]).unwrap();
    let copy = s.doc().unwrap().subtree(instance).into_iter().find(|id| s.doc().unwrap().node(*id).is_some_and(|n| n.source == Some(line))).unwrap();
    assert_eq!(s.doc().unwrap().node(copy).unwrap().stroke.dash_phase, Some(3.0));
    s.execute("node.set", &json!({"id":copy,"props":{"dashPhase":-8}})).unwrap();
    assert!(s.doc().unwrap().node(copy).unwrap().overrides.contains("strokes"));
    s.execute("node.set", &json!({"id":line,"props":{"dashPhase":5}})).unwrap();
    assert_eq!(s.doc().unwrap().node(copy).unwrap().stroke.dash_phase, Some(-8.0));
    s.execute("node.set", &json!({"id":copy,"props":{"width":150,"height":20}})).unwrap();
    assert_eq!(s.doc().unwrap().node(copy).unwrap().stroke.dash_phase, Some(-8.0), "ordinary resize keeps physical phase");
    s.execute("component.resetOverride", &json!({"id":copy,"group":"strokes"})).unwrap();
    assert_eq!(s.doc().unwrap().node(copy).unwrap().stroke.dash_phase, Some(5.0));
    s.execute("node.set", &json!({"id":copy,"props":{"dashPhase":null}})).unwrap();
    s.execute("node.set", &json!({"id":line,"props":{"dashPhase":6}})).unwrap();
    assert_eq!(s.doc().unwrap().node(copy).unwrap().stroke.dash_phase, None, "explicit automatic mode remains a stroke override");
}

#[test]
fn manual_phase_collab_stroke_property_roundtrip_preserves_the_offset() {
    use figcraft_collab::{Change, Mode, Prop, Replica};
    let (mut s, _, line) = scene();
    let mut replica = Replica::new(s.doc().unwrap().clone());
    s.execute("node.set", &json!({"id":line,"props":{"dashPhase":-8.25}})).unwrap();
    let changes = figcraft_collab::diff(&replica, s.doc().unwrap()).unwrap().changes;
    assert!(changes.iter().any(|change| matches!(change, Change::Set { id, prop: Prop::Stroke, .. } if *id == line)));
    let wire = serde_json::to_vec(&changes).unwrap();
    let decoded: Vec<Change> = serde_json::from_slice(&wire).unwrap();
    replica.apply(&decoded, Mode::Strict).unwrap();
    assert_eq!(replica.document().node(line).unwrap().stroke.dash_phase, Some(-8.25));
}

#[test]
fn manual_phase_compound_closed_and_odd_patterns_restart_without_rotating_the_array() {
    let mut p = path();
    p.move_to((0.0, 20.0));
    p.line_to((100.0, 20.0));
    let s = props(json!(12));
    let out = figcraft_pathops::stroke_center(&p, s.weight, &s);
    for y in [0.0, 20.0] {
        assert_eq!(out.winding(Point::new(3.0, y)), 0);
        assert_ne!(out.winding(Point::new(13.0, y)), 0);
    }
    let closed = Rect::new(0.0, 0.0, 100.0, 20.0).to_path(0.1);
    let out = figcraft_pathops::stroke_center(&closed, s.weight, &s);
    assert!(!ink(&out, 3.0));
    assert!(ink(&out, 13.0));
    let odd = StrokeProps { dashes: vec![8.0, 4.0, 2.0], ..s };
    let out = figcraft_pathops::stroke_center(&path(), odd.weight, &odd);
    for (x, expected) in [(1.0, true), (3.0, false), (11.0, true), (15.0, false), (17.0, true), (25.0, false)] {
        assert_eq!(ink(&out, x), expected, "odd pattern at{x}");
    }
    assert_eq!(odd.dashes, [8.0, 4.0, 2.0]);
}

#[test]
fn manual_phase_extreme_saved_values_and_tiny_patterns_remain_bounded() {
    for phase in [f64::MAX, -f64::MAX, f64::NAN, f64::INFINITY] {
        let p = StrokeProps { dash_phase: Some(phase), ..props(Value::Null) };
        let out = figcraft_pathops::stroke_center(&path(), p.weight, &p);
        assert!(out.elements().len() < 500);
        assert!(out.bounding_box().x0.is_finite() && out.bounding_box().x1.is_finite());
    }
    for dashes in [vec![f64::MAX, f64::MAX], vec![0.0, 0.0], vec![0.00001, 0.00001]] {
        let p = StrokeProps { dashes, ..props(json!(13)) };
        let out = figcraft_pathops::stroke_center(&path(), p.weight, &p);
        assert!(out.elements().len() < 500, "existing budget draws this solid rather than expanding dashes");
        assert!(ink(&out, 3.0) && ink(&out, 13.0));
    }
    let (mut s, _, line) = scene();
    raw_phase(&mut s, line, json!(f64::MAX));
    let before = s.doc().unwrap().clone();
    assert!(s.execute("transform.scale", &json!({"ids":[line],"factor":2})).is_err());
    assert_eq!(s.doc().unwrap(), &before, "phase overflow refuses before any geometry changes");
}

#[test]
fn manual_phase_foreign_fig_and_penpot_exports_report_the_loss() {
    let (mut s, _, line) = scene();
    raw_phase(&mut s, line, json!(12));
    for format in ["fig", "penpot"] {
        let out = s.execute("file.export", &json!({"format":format})).unwrap();
        let warnings = out["warnings"].as_array().unwrap();
        assert!(warnings.iter().any(|v| v.as_str().is_some_and(|s| s.to_ascii_lowercase().contains("manual dash phase"))), "{out}");
    }
}
