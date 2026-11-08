//! Original live Boolean-result rounding workflows; operands remain editable, never flattened.

use std::io::Read;

use base64::Engine as _;
use figcraft_doc::{Document, NodeId, NodeKind};
use figcraft_geom::{BezPath, Point, Shape};
use serde_json::{Value, json};

use crate::{Session, params::id_of};

fn ex(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(command, &params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn create(s: &mut Session, command: &str, params: Value) -> NodeId {
    id_of(&ex(s, command, params)["id"]).unwrap()
}

fn scene() -> (Session, NodeId, [NodeId; 2]) {
    let mut s = Session::new();
    let bottom = create(&mut s, "shape.rectangle", json!({"x":20,"y":20,"width":120,"height":100,"props":{"fill":"#101010"}}));
    let cut = create(&mut s, "shape.rectangle", json!({"x":90,"y":70,"width":70,"height":70}));
    let group = create(&mut s, "bool.subtract", json!({"ids":[bottom,cut]}));
    (s, group, [bottom, cut])
}

fn set_radius(s: &mut Session, group: NodeId, radius: f64) {
    ex(s, "node.set", json!({"ids":[group],"props":{"cornerRadius":radius}}));
}

fn outline(doc: &Document, id: NodeId) -> BezPath {
    let mut path = figcraft_render::node_outline(doc, doc.node(id).unwrap()).unwrap().0;
    path.apply_affine(doc.world_transform(id));
    path
}

fn expected_area(radius: f64) -> f64 {
    // Five convex corners and one concave corner: four net square-minus-quarter-circle cuts.
    9500.0 - 4.0 * (1.0 - std::f64::consts::PI / 4.0) * radius * radius
}

fn export(s: &mut Session, group: NodeId, format: &str) -> Vec<u8> {
    let result = ex(s, "file.export", json!({"ids":[group],"format":format,"scale":4}));
    base64::engine::general_purpose::STANDARD.decode(result["data"].as_str().unwrap()).unwrap()
}

#[test]
fn boolean_radius_rounds_the_live_result_and_keeps_operands_and_undo() {
    let (mut s, group, children) = scene();
    let original = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let before = outline(&original, group);
    assert!((before.area().abs() - 9500.0).abs() < 0.01);
    assert_ne!(before.winding(Point::new(20.5, 20.5)) % 2, 0);
    assert_eq!(before.winding(Point::new(90.5, 70.5)) % 2, 0);
    set_radius(&mut s, group, 12.0);
    let doc = s.doc().unwrap();
    let rounded = outline(doc, group);
    assert!((rounded.area().abs() - expected_area(12.0)).abs() < 0.2, "rounded area {} instead of {}", rounded.area(), expected_area(12.0));
    assert_eq!(rounded.winding(Point::new(20.5, 20.5)) % 2, 0, "outer result corner is rounded");
    assert_ne!(rounded.winding(Point::new(90.5, 70.5)) % 2, 0, "concave cutout corner is rounded too");
    assert!(matches!(doc.node(group).unwrap().kind, NodeKind::BooleanOp { .. }));
    assert_eq!(doc.node(group).unwrap().children, children);
    for id in children {
        assert_eq!(doc.node(id), original.node(id), "radius belongs to the result, not operand {id}");
    }
    assert_eq!(s.undo_depth().0, history.0 + 1);
    let (reopened, _) = figcraft_format::load_bytes(&figcraft_format::save_bytes(doc, None).unwrap()).unwrap();
    assert!((outline(&reopened, group).area().abs() - rounded.area().abs()).abs() < 1e-5);
    assert!(matches!(reopened.node(group).unwrap().kind, NodeKind::BooleanOp { .. }));
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &original);
    s.redo().unwrap();
    assert!((outline(s.doc().unwrap(), group).area().abs() - rounded.area().abs()).abs() < 1e-5);
}

#[test]
fn boolean_radius_and_child_edits_replace_a_primed_result() {
    let (mut s, group, [_, cut]) = scene();
    set_radius(&mut s, group, 12.0);
    let first = outline(s.doc().unwrap(), group);
    assert_eq!(outline(s.doc().unwrap(), group), first, "unchanged result can be reused");
    set_radius(&mut s, group, 20.0);
    let second = outline(s.doc().unwrap(), group);
    assert!((second.area().abs() - expected_area(20.0)).abs() < 0.5, "radius edit cannot reuse the old or unrounded result: {}", second.area());
    assert_ne!(second, first);
    ex(&mut s, "node.set", json!({"ids":[cut],"props":{"width":30}}));
    let edited = outline(s.doc().unwrap(), group);
    assert_ne!(edited, second, "operand geometry remains live after group rounding");
    assert!(edited.area().abs() > second.area().abs());
    assert_eq!(s.doc().unwrap().node(cut).unwrap().corners.radii, [0.0; 4]);
    s.undo().unwrap();
    assert_eq!(outline(s.doc().unwrap(), group), second);
    s.undo().unwrap();
    assert_eq!(outline(s.doc().unwrap(), group), first);
}

#[test]
fn boolean_radius_png_export_cuts_the_outer_corner() {
    let (mut s, group, _) = scene();
    let square = image::load_from_memory(&export(&mut s, group, "png")).unwrap().to_rgba8();
    assert!(square.get_pixel(1, 1).0[3] > 250, "control covers the original outer corner");
    set_radius(&mut s, group, 12.0);
    let rounded = image::load_from_memory(&export(&mut s, group, "png")).unwrap().to_rgba8();
    assert!(rounded.get_pixel(1, 1).0[3] < 10, "actual PNG must draw the rounded result");
    assert!(rounded.get_pixel(100, 100).0[3] > 250, "filled interior stays visible");
}

#[test]
fn boolean_radius_svg_export_reimports_the_rounded_geometry() {
    let (mut s, group, _) = scene();
    set_radius(&mut s, group, 12.0);
    let svg = export(&mut s, group, "svg");
    let mut imported = Document::new("Original rounded Boolean SVG");
    let page = imported.first_page().unwrap();
    figcraft_svg::import_svg(&mut imported, &svg, page, 0.0, 0.0, "Original geometry").unwrap();
    let paths: Vec<_> = imported.nodes().filter(|n| matches!(n.kind, NodeKind::Vector { .. })).map(|n| outline(&imported, n.id)).collect();
    assert_eq!(paths.len(), 1);
    assert!((paths[0].area().abs() - expected_area(12.0)).abs() < 0.5, "SVG must contain the drawn radius: {}", paths[0].area());
}

fn pdf_streams(pdf: &[u8]) -> String {
    // Original simple geometry has no images/fonts. Inspect inflated drawing operators as the
    // existing PDF tests do, with a bounded read for each stream.
    let mut result = String::new();
    let mut offset = 0;
    while let Some(start) = pdf[offset..].windows(7).position(|w| w == b"stream\n") {
        let start = offset + start + 7;
        let Some(end) = pdf[start..].windows(9).position(|w| w == b"endstream") else { break };
        let mut bytes = Vec::new();
        if flate2::read::ZlibDecoder::new(&pdf[start..start + end]).take(1024 * 1024).read_to_end(&mut bytes).is_ok() {
            result.push_str(&String::from_utf8_lossy(&bytes));
        }
        offset = start + end + 9;
    }
    result
}

#[test]
fn boolean_radius_pdf_export_writes_cubic_corners() {
    let (mut s, group, _) = scene();
    let before = pdf_streams(&export(&mut s, group, "pdf"));
    assert!(!before.is_empty(), "actual PDF control has a readable drawing stream");
    assert_eq!(before.split_whitespace().filter(|word| *word == "c").count(), 0);
    set_radius(&mut s, group, 12.0);
    let after = pdf_streams(&export(&mut s, group, "pdf"));
    assert!(after.split_whitespace().any(|word| word == "c"), "PDF must contain cubic corner geometry: {after}");
}

#[test]
fn boolean_radius_zero_and_ellipse_only_controls_keep_existing_geometry() {
    let (mut s, group, _) = scene();
    let before = outline(s.doc().unwrap(), group);
    set_radius(&mut s, group, 0.0);
    assert_eq!(outline(s.doc().unwrap(), group), before);
    let mut s = Session::new();
    let a = create(&mut s, "shape.ellipse", json!({"x":10,"y":10,"width":100,"height":80}));
    let b = create(&mut s, "shape.ellipse", json!({"x":50,"y":40,"width":100,"height":80}));
    let group = create(&mut s, "bool.subtract", json!({"ids":[a,b]}));
    let before = outline(s.doc().unwrap(), group);
    set_radius(&mut s, group, 12.0);
    assert_eq!(outline(s.doc().unwrap(), group), before, "only-ellipse groups remain unsupported for corner radius");
}

#[test]
fn boolean_radius_keeps_a_closed_cutout_hole_and_contour_orientation() {
    for reflected in [false, true] {
        let mut s = Session::new();
        let bottom = create(&mut s, "shape.rectangle", json!({"x":20,"y":20,"width":140,"height":120,"props":{"fill":"#101010"}}));
        let cut = create(&mut s, "shape.rectangle", json!({"x":65,"y":55,"width":50,"height":50}));
        let group = create(&mut s, "bool.subtract", json!({"ids":[bottom,cut]}));
        if reflected {
            ex(&mut s, "object.flipH", json!({"ids":[group]}));
        }
        let before = outline(s.doc().unwrap(), group);
        let original = s.doc().unwrap().clone();
        set_radius(&mut s, group, 8.0);
        let rounded = outline(s.doc().unwrap(), group);
        let world = s.doc().unwrap().world_transform(group);
        // All probes are group-local so reflection verifies the same contour contract.
        for (point, before_inside, rounded_inside) in [
            (Point::new(0.5, 0.5), true, false),
            (Point::new(45.5, 35.5), false, true),
            (Point::new(70.0, 60.0), false, false),
            (Point::new(25.0, 60.0), true, true),
        ] {
            let point = world * point;
            assert_eq!(before.winding(point) % 2 != 0, before_inside);
            assert_eq!(rounded.winding(point) % 2 != 0, rounded_inside, "hole/material probe {point:?}");
        }
        assert_eq!(rounded.elements().iter().filter(|e| matches!(e, figcraft_geom::kurbo::PathEl::MoveTo(_))).count(), 2);
        assert_eq!(s.doc().unwrap().node(bottom), original.node(bottom));
        assert_eq!(s.doc().unwrap().node(cut), original.node(cut));
        let svg = export(&mut s, group, "svg");
        let mut imported = Document::new("Original rounded hole SVG");
        let page = imported.first_page().unwrap();
        figcraft_svg::import_svg(&mut imported, &svg, page, 0.0, 0.0, "Hole").unwrap();
        let vectors: Vec<_> = imported.nodes().filter(|n| matches!(n.kind, NodeKind::Vector { .. })).collect();
        assert_eq!(vectors.len(), 1);
        let svg_path = outline(&imported, vectors[0].id);
        assert!((svg_path.area().abs() - rounded.area().abs()).abs() < 0.5);
        assert_eq!(svg_path.elements().iter().filter(|e| matches!(e, figcraft_geom::kurbo::PathEl::MoveTo(_))).count(), 2);
        let bounds = svg_path.bounding_box();
        assert_eq!(svg_path.winding(bounds.center()) % 2, 0, "SVG keeps the closed hole");
        assert_ne!(svg_path.winding(Point::new(bounds.x0 + 25.0, bounds.y0 + 60.0)) % 2, 0);
        let png = image::load_from_memory(&export(&mut s, group, "png")).unwrap().to_rgba8();
        assert_eq!(png.get_pixel(280, 240).0[3], 0, "raster keeps the hole");
        assert!(png.get_pixel(100, 240).0[3] > 250, "raster keeps surrounding material");
        assert!(png.get_pixel(1, 1).0[3] < 10, "raster rounds the outer corner");
    }
}

#[test]
fn boolean_radius_tracks_operation_and_visibility_after_a_primed_result() {
    let (mut s, group, [_, cut]) = scene();
    set_radius(&mut s, group, 12.0);
    let cutout = outline(s.doc().unwrap(), group);
    ex(&mut s, "bool.union", json!({"ids":[cut]}));
    let union = outline(s.doc().unwrap(), group);
    assert_ne!(union, cutout);
    assert!(union.area().abs() > cutout.area().abs());
    ex(&mut s, "node.set", json!({"ids":[cut],"props":{"visible":false}}));
    let hidden = outline(s.doc().unwrap(), group);
    assert_ne!(hidden, union);
    assert!((hidden.area().abs() - (12000.0 - 4.0 * (1.0 - std::f64::consts::PI / 4.0) * 12.0 * 12.0)).abs() < 0.2);
    s.undo().unwrap();
    assert_eq!(outline(s.doc().unwrap(), group), union);
    s.undo().unwrap();
    assert_eq!(outline(s.doc().unwrap(), group), cutout);
}

#[test]
fn boolean_radius_rejects_independent_bbox_corners_without_changing_the_document() {
    let (mut s, group, _) = scene();
    for props in [json!({"cornerRadius":[1,2,3,4]}), json!({"topLeftRadius":4}), json!({"bottomRightRadius":3})] {
        let before = s.doc().unwrap().clone();
        let history = s.undo_depth();
        assert!(s.execute("node.set", &json!({"ids":[group],"props":props})).is_err());
        assert_eq!(s.doc().unwrap(), &before);
        assert_eq!(s.undo_depth(), history);
    }
    ex(&mut s, "node.set", json!({"ids":[group],"props":{"cornerRadius":[2.5,2.5,2.5,2.5]}}));
    assert!((outline(s.doc().unwrap(), group).area().abs() - expected_area(2.5)).abs() < 0.05);
    assert!(crate::shape_handles::handles(s.doc().unwrap(), &[group], 1.0).is_empty(), "an arbitrary result has no bounding-box radius handles");
}

#[test]
fn boolean_radius_plain_canvas_hits_follow_the_rounded_result() {
    use crate::tools::{PointerEvent, PointerKind};
    let (mut s, group, _) = scene();
    set_radius(&mut s, group, 12.0);
    for (point, inside) in [((20.5, 20.5), false), ((90.5, 70.5), true)] {
        ex(&mut s, "select.none", json!({}));
        for kind in [PointerKind::Down, PointerKind::Up] {
            s.pointer(&PointerEvent::at(kind, point.0, point.1, 1.0)).unwrap();
        }
        assert_eq!(s.selection().contains(&group), inside, "plain canvas hit at {point:?}");
    }
}

#[test]
fn boolean_radius_survives_fig_roundtrip_as_an_editable_boolean() {
    let (mut s, group, _) = scene();
    set_radius(&mut s, group, 12.0);
    let bytes = export(&mut s, group, "fig");
    let (doc, _) = figcraft_format::fig::import(&bytes, "Original live rounded Boolean").unwrap();
    let booleans: Vec<_> = doc.nodes().filter(|n| matches!(n.kind, NodeKind::BooleanOp { .. })).collect();
    assert_eq!(booleans.len(), 1);
    let group = booleans[0];
    assert_eq!(group.corners.radii, [12.0; 4]);
    assert_eq!(group.children.len(), 2);
    assert!(group.children.iter().all(|id| matches!(doc.node(*id).unwrap().kind, NodeKind::Rectangle)));
    assert!((outline(&doc, group.id).area().abs() - expected_area(12.0)).abs() < 0.5);
}

#[test]
fn boolean_radius_inside_a_reused_component_stays_live() {
    let (mut s, group, original_operands) = scene();
    set_radius(&mut s, group, 12.0);
    let component = create(&mut s, "component.create", json!({"ids":[group]}));
    let instance = create(&mut s, "instance.create", json!({"component":component,"x":300,"y":20}));
    let instance_result = |s: &Session| {
        let node = s.doc().unwrap().node(instance).unwrap();
        assert_eq!(node.children.len(), 1);
        let id = node.children[0];
        assert!(matches!(s.doc().unwrap().node(id).unwrap().kind, NodeKind::BooleanOp { .. }));
        id
    };
    let copied = instance_result(&s);
    assert!((outline(s.doc().unwrap(), copied).area().abs() - expected_area(12.0)).abs() < 0.2);
    set_radius(&mut s, group, 20.0);
    let copied = instance_result(&s);
    assert!((outline(s.doc().unwrap(), copied).area().abs() - expected_area(20.0)).abs() < 0.5);
    assert_eq!(s.doc().unwrap().node(group).unwrap().children, original_operands);
    assert!(original_operands.iter().all(|id| matches!(s.doc().unwrap().node(*id).unwrap().kind, NodeKind::Rectangle)));
    s.undo().unwrap();
    let copied = instance_result(&s);
    assert!((outline(s.doc().unwrap(), copied).area().abs() - expected_area(12.0)).abs() < 0.2);
}

#[test]
fn boolean_radius_penpot_export_reports_omitted_live_result_rounding() {
    let (mut s, group, _) = scene();
    let control = ex(&mut s, "file.export", json!({"format":"penpot"}));
    assert!(!control["warnings"].as_array().unwrap().iter().any(|w| w.as_str().is_some_and(|w| w.contains("Boolean result corner"))));
    set_radius(&mut s, group, 12.0);
    assert!((outline(s.doc().unwrap(), group).area().abs() - expected_area(12.0)).abs() < 0.2);
    let result = ex(&mut s, "file.export", json!({"format":"penpot"}));
    let bytes = base64::engine::general_purpose::STANDARD.decode(result["data"].as_str().unwrap()).unwrap();
    let (back, _) = figcraft_format::penpot::import(&bytes).unwrap();
    let groups: Vec<_> = back.nodes().filter(|n| matches!(n.kind, NodeKind::BooleanOp { .. })).collect();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].corners.radii, [0.0; 4], "the existing Penpot Boolean schema does not retain this setting");
    assert_eq!(groups[0].children.len(), 2, "operands remain editable in the destination");
    assert!((outline(&back, groups[0].id).area().abs() - 9500.0).abs() < 0.1, "actual roundtrip loses the rounded outline");
    assert!(
        result["warnings"].as_array().unwrap().iter().any(|w| w.as_str().is_some_and(|w| w.contains("Boolean result corner"))),
        "radius loss must be reported: {}",
        result["warnings"]
    );
}

#[test]
fn boolean_radius_preserves_editable_curved_operands_and_rounds_result_corners() {
    let mut s = Session::new();
    let bottom = create(&mut s, "shape.rectangle", json!({"x":20,"y":20,"width":120,"height":100}));
    let ellipse = create(&mut s, "shape.ellipse", json!({"x":80,"y":45,"width":90,"height":60}));
    let group = create(&mut s, "bool.subtract", json!({"ids":[bottom,ellipse]}));
    let original = s.doc().unwrap().clone();
    let before = outline(&original, group);
    assert_ne!(before.winding(Point::new(20.5, 20.5)) % 2, 0);
    assert_eq!(before.winding(Point::new(125.0, 75.0)) % 2, 0);
    assert!(before.elements().iter().any(|el| matches!(el, figcraft_geom::kurbo::PathEl::CurveTo(..))));
    set_radius(&mut s, group, 8.0);
    let after = outline(s.doc().unwrap(), group);
    assert_ne!(after, before);
    assert_eq!(after.winding(Point::new(20.5, 20.5)) % 2, 0);
    assert_eq!(after.winding(Point::new(125.0, 75.0)) % 2, 0);
    assert_ne!(after.winding(Point::new(40.0, 70.0)) % 2, 0);
    assert_eq!(s.doc().unwrap().node(bottom), original.node(bottom));
    assert_eq!(s.doc().unwrap().node(ellipse), original.node(ellipse));
    assert!(matches!(s.doc().unwrap().node(ellipse).unwrap().kind, NodeKind::Ellipse { .. }));
    let (back, _) = figcraft_format::load_bytes(&figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap()).unwrap();
    assert_eq!(outline(&back, group), after);
}

fn appearance_source(s: &mut Session, radius: Value) -> NodeId {
    create(
        s,
        "shape.rectangle",
        json!({"x":300,"y":20,"width":40,"height":40,"props":{"fill":"#2367AB","opacity":0.7,"cornerRadius":radius,"cornerSmoothing":0.4,"effects":[{"type":"layerBlur","radius":2}]}}),
    )
}

#[test]
fn boolean_radius_paste_properties_preserves_supported_appearance_from_independent_corners() {
    let (mut s, group, children) = scene();
    set_radius(&mut s, group, 12.0);
    let source = appearance_source(&mut s, json!([1, 2, 3, 4]));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    ex(&mut s, "edit.copyProperties", json!({"id":source}));
    ex(&mut s, "edit.pasteProperties", json!({"ids":[group]}));
    let copied = s.doc().unwrap().node(source).unwrap();
    let pasted = s.doc().unwrap().node(group).unwrap();
    assert_eq!(pasted.fills, copied.fills);
    assert_eq!(pasted.effects, copied.effects);
    assert_eq!(pasted.opacity, copied.opacity);
    assert_eq!(pasted.corners.radii, [12.0; 4], "unsupported independent corners do not replace the live uniform radius");
    assert_eq!(pasted.corners.smoothing, 0.0, "unsupported smoothing is not copied into a Boolean");
    assert_eq!(pasted.children, children);
    for id in children {
        assert_eq!(s.doc().unwrap().node(id), before.node(id));
    }
    assert_eq!(s.undo_depth().0, history.0 + 1);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn boolean_radius_paste_properties_accepts_a_uniform_source_radius() {
    let (mut s, group, _) = scene();
    let source = appearance_source(&mut s, json!(6));
    ex(&mut s, "edit.copyProperties", json!({"id":source}));
    ex(&mut s, "edit.pasteProperties", json!({"ids":[group]}));
    assert_eq!(s.doc().unwrap().node(group).unwrap().corners.radii, [6.0; 4]);
    assert!((outline(s.doc().unwrap(), group).area().abs() - expected_area(6.0)).abs() < 0.1);
    assert_eq!(s.doc().unwrap().node(group).unwrap().fills, s.doc().unwrap().node(source).unwrap().fills);
}

#[test]
fn boolean_radius_paste_properties_existing_unsupported_kind_control() {
    let mut s = Session::new();
    let target = create(&mut s, "shape.ellipse", json!({"x":10,"y":10,"width":60,"height":60}));
    let source = appearance_source(&mut s, json!([1, 2, 3, 4]));
    ex(&mut s, "edit.copyProperties", json!({"id":source}));
    ex(&mut s, "edit.pasteProperties", json!({"ids":[target]}));
    assert_eq!(s.doc().unwrap().node(target).unwrap().fills, s.doc().unwrap().node(source).unwrap().fills);
    assert_eq!(s.doc().unwrap().node(target).unwrap().effects, s.doc().unwrap().node(source).unwrap().effects);
    assert_eq!(s.doc().unwrap().node(target).unwrap().corners.radii, [0.0; 4]);
}
