//! Original in-memory .jam archives: metadata-only saved paths, never product samples or assets.

use super::*;
use figcraft_doc::{Document, StrokeCap};

fn saved_schema() -> Schema {
    let text = SCHEMA.replace("Vector relativePosition\n", "Vector relativePosition, ConnectorMagnet magnet\n").replacen(
        "\nmessage Blob ",
        ", ConnectorControlPoint[] connectorControlPoints, ConnectorControlPoint[] connectorBezierControlPoints\n\
             enum ConnectorMagnet NONE AUTO TOP LEFT BOTTOM RIGHT CENTER\n\
             message ConnectorControlPoint Vector position, Vector axis\n\
             message Blob ",
        1,
    );
    Schema::parse(&text).unwrap()
}

fn control(x: f64, y: f64, across: bool) -> Kv<'static> {
    o(vec![("position", xy(x, y)), ("axis", xy(if across { 1.0 } else { 0.0 }, if across { 0.0 } else { 1.0 }))])
}

fn field_mut<'a, 's>(value: &'a mut Kv<'s>, name: &str) -> Option<&'a mut Kv<'s>> {
    let Kv::Object(fields) = value else { return None };
    fields.iter_mut().find(|(key, _)| *key == name).map(|(_, field)| field)
}

fn controls() -> Vec<Kv<'static>> {
    vec![control(70.0, 65.0, true), control(140.0, 130.0, false)]
}

fn saved_message(points: Option<Vec<Kv<'static>>>, labelled: bool, nested: bool, style: &'static str) -> Kv<'static> {
    let parent = if nested { 2 } else { 1 };
    let mut fields = vec![
        ("strokePaints", arr(vec![solid(0.0, 0.0, 0.0)])),
        ("strokeWeight", f(3.0)),
        ("dashPattern", arr(vec![f(7.0), f(3.0)])),
        ("connectorStartCap", e("ROUND")),
        ("connectorEndCap", e("ARROW_EQUILATERAL")),
        ("connectorLineStyle", e(style)),
        ("connectorStart", o(vec![("endpointNodeID", g(10)), ("magnet", e("BOTTOM")), ("relativePosition", xy(0.5, 1.0))])),
        ("connectorEnd", o(vec![("endpointNodeID", g(11)), ("magnet", e("LEFT")), ("relativePosition", xy(0.0, 0.5))])),
        // Saved control points and endpoints are in the parent space, not this transform.
        ("transform", at(-900.0, 1700.0)),
    ];
    if let Some(points) = points {
        fields.push(("connectorControlPoints", arr(points)));
    }
    if labelled {
        fields.push(("textData", o(vec![("characters", s("Original saved route"))])));
        fields.push(("fontSize", f(12.0)));
    }
    let mut nodes = vec![node(0, None, "DOCUMENT", "Original fixture", vec![]), node(1, Some((0, "!")), "CANVAS", "Synthetic board", vec![])];
    if nested {
        nodes.push(node(2, Some((1, "!")), "FRAME", "Translated parent", vec![("transform", at(-320.0, 75.0)), ("size", xy(800.0, 800.0))]));
    }
    nodes.extend([
        node(10, Some((parent, "!")), "RECTANGLE", "North box", vec![("size", xy(80.0, 60.0)), ("transform", at(-40.0, -80.0))]),
        node(11, Some((parent, "\"")), "RECTANGLE", "East box", vec![("size", xy(100.0, 60.0)), ("transform", at(260.0, 180.0))]),
        node(12, Some((parent, "#")), "CONNECTOR", "Saved route", fields),
    ]);
    o(vec![("nodeChanges", arr(nodes)), ("blobs", arr(vec![]))])
}

fn jam(points: Option<Vec<Kv<'static>>>, labelled: bool, nested: bool, style: &'static str) -> Vec<u8> {
    let mut payload = canvas(&saved_schema(), &saved_message(points, labelled, nested, style));
    payload[..8].copy_from_slice(JAM_HEADER);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("canvas.fig", options).unwrap();
    zip.write_all(&payload).unwrap();
    zip.start_file("meta.json", options).unwrap();
    zip.write_all(br#"{"file_name":"Original saved-connector fixture"}"#).unwrap();
    zip.finish().unwrap().into_inner()
}

fn line(doc: &Document, labelled: bool) -> &figcraft_doc::Node {
    let root = named(doc, "Saved route");
    if labelled { doc.node(root.children[0]).unwrap() } else { root }
}

fn points(doc: &Document, labelled: bool) -> Vec<(f64, f64)> {
    let line = line(doc, labelled);
    let NodeKind::Vector { network } = &line.kind else { panic!("connector line is a vector") };
    let world = doc.world_transform(line.id);
    network.vertices.iter().map(|v| world * figcraft_doc::geom::Point::new(v.x, v.y)).map(|p| (p.x, p.y)).collect()
}

fn routed(warnings: &[String]) -> bool {
    warnings.iter().any(|w| w.contains("FigCraft's own routing"))
}

fn keep_requested_original_fixture(bytes: &[u8]) {
    let Some(path) = std::env::var_os("FIGCRAFT_TEST_SAVED_ELBOW_FIXTURE_OUT") else { return };
    // Native acceptance can request the exact original fixture; never replace an existing file.
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap();
    file.write_all(bytes).unwrap();
}

#[test]
fn saved_elbow_metadata_preserves_internal_runs_and_line_style() {
    let (_, plain_warnings) = import(&jam(None, false, false, "ELBOWED"), "Control").unwrap();
    assert!(routed(&plain_warnings));
    let fixture = jam(Some(controls()), false, false, "ELBOWED");
    keep_requested_original_fixture(&fixture);
    let (doc, warnings) = import(&fixture, "Saved").unwrap();
    let path = points(&doc, false);
    assert_eq!(path.len(), 5, "two saved internal runs produce four segments: {path:?}");
    assert_eq!(path[0], (0.0, -20.0), "saved artwork keeps the original border endpoint independently of native automatic gaps");
    assert_eq!(path[4], (260.0, 210.0));
    assert_eq!(&path[1..4], &[(0.0, 65.0), (140.0, 65.0), (140.0, 210.0)]);
    assert!(!routed(&warnings), "a preserved saved path must not claim regenerated routing: {warnings:?}");
    let line = line(&doc, false);
    assert_eq!(line.stroke.weight, 3.0);
    assert_eq!(line.stroke.dashes, [7.0, 3.0]);
    assert_eq!(line.stroke.cap_start, StrokeCap::Round);
    assert_eq!(line.stroke.cap_end, StrokeCap::TriangleArrow);
    assert_eq!(line.strokes.len(), 1);
    let (reopened, _) = crate::load_bytes(&crate::save_bytes(&doc, None).unwrap()).unwrap();
    assert_eq!(points(&reopened, false), path, "native save/open keeps the imported editable vector");
}

#[test]
fn the_same_saved_runs_import_from_a_fig_canvas() {
    let fixture = canvas(&saved_schema(), &saved_message(Some(controls()), false, false, "ELBOWED"));
    let (doc, warnings) = import(&fixture, "Saved fig route").unwrap();
    let path = points(&doc, false);
    assert_eq!(path.len(), 5);
    assert_eq!(&path[1..4], &[(0.0, 65.0), (140.0, 65.0), (140.0, 210.0)]);
    assert!(!routed(&warnings), "{warnings:?}");
}

#[test]
fn saved_runs_between_free_endpoints_keep_their_exact_points() {
    let mut message = saved_message(Some(controls()), false, false, "ELBOWED");
    let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
    let connector = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("Saved route")).unwrap();
    *field_mut(connector, "connectorStart").unwrap() = o(vec![("position", xy(0.0, -20.0))]);
    *field_mut(connector, "connectorEnd").unwrap() = o(vec![("position", xy(260.0, 210.0))]);
    let (doc, warnings) = import(&canvas(&saved_schema(), &message), "Saved free route").unwrap();
    assert_eq!(points(&doc, false), [(0.0, -20.0), (0.0, 65.0), (140.0, 65.0), (140.0, 210.0), (260.0, 210.0)]);
    assert!(!routed(&warnings), "{warnings:?}");
}

#[test]
fn saved_elbow_metadata_keeps_parent_coordinates_and_label_midpoint() {
    let (doc, warnings) = import(&jam(Some(controls()), true, true, "ELBOWED"), "Nested").unwrap();
    let path = points(&doc, true);
    assert_eq!(path.len(), 5, "labelled path uses the same saved runs: {path:?}");
    assert_eq!(&path[1..4], &[(-320.0, 140.0), (-180.0, 140.0), (-180.0, 285.0)]);
    assert!(!routed(&warnings), "{warnings:?}");
    let root = named(&doc, "Saved route");
    assert_eq!(root.children.len(), 2);
    let label = doc.node(root.children[1]).unwrap();
    let centre = doc.world_bounds(label.id).unwrap().center();
    let lengths: Vec<f64> = path.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).collect();
    let mut remaining = lengths.iter().sum::<f64>() / 2.0;
    let mut middle = path[0];
    for (segment, length) in path.windows(2).zip(lengths) {
        if remaining <= length {
            let t = remaining / length;
            middle = (segment[0].0 + (segment[1].0 - segment[0].0) * t, segment[0].1 + (segment[1].1 - segment[0].1) * t);
            break;
        }
        remaining -= length;
    }
    assert!((centre.x - middle.0).abs() < 1e-5 && (centre.y - middle.1).abs() < 1e-5, "label {centre:?}, saved-path middle {middle:?}");
}

#[test]
fn saved_elbow_metadata_at_the_run_limit_remains_bounded_and_is_not_routed_again() {
    let data = (0..62).map(|i| control(140.0 + (i / 2) as f64, 65.0 + (i / 2) as f64, i % 2 == 0)).collect();
    let (doc, warnings) = import(&jam(Some(data), false, false, "ELBOWED"), "Bounded saved route").unwrap();
    assert_eq!(points(&doc, false).len(), 65, "62 internal runs plus the first and last are64 segments");
    assert!(!routed(&warnings), "a bounded valid path is preserved: {warnings:?}");
}

#[test]
fn malformed_saved_elbow_metadata_uses_the_existing_route_with_a_truthful_note() {
    let (plain, _) = import(&jam(None, false, false, "ELBOWED"), "Control").unwrap();
    let expected = points(&plain, false);
    let invalid = [
        vec![],
        vec![o(vec![("position", xy(70.0, 65.0)), ("axis", xy(1.0, 1.0))])],
        vec![o(vec![("position", xy(70.0, 65.0)), ("axis", xy(0.0, 0.0))])],
        vec![o(vec![("axis", xy(1.0, 0.0))])],
        vec![control(70.0, 65.0, true), control(140.0, 130.0, true)],
        vec![control(70.0, 65.0, false)],
        vec![control(70.0, -60.0, true), control(140.0, 130.0, false)],
        vec![control(70.0, 65.0, true), control(320.0, 130.0, false)],
        (0..65).map(|i| control(70.0 + i as f64, 65.0 + i as f64, i % 2 == 0)).collect(),
    ];
    for (index, value) in invalid.into_iter().enumerate() {
        let (doc, warnings) = import(&jam(Some(value), false, false, "ELBOWED"), "Invalid metadata").unwrap();
        assert_eq!(points(&doc, false), expected, "invalid case {index} keeps the existing route");
        assert!(routed(&warnings), "invalid case {index}: {warnings:?}");
    }
}

#[test]
fn nonfinite_decoded_metadata_falls_back_without_serializing_nonfinite_geometry() {
    // Our fixture encoder rejects NaN. Exercise the mapper's decoded-value boundary directly.
    let message = saved_message(Some(vec![o(vec![("position", xy(f64::NAN, 65.0)), ("axis", xy(1.0, 0.0))])]), false, false, "ELBOWED");
    let (rest, warnings, networks) = super::super::map::to_rest(&message, "Hostile decoded metadata", crate::rest::MAX_NODES);
    let (doc, _) = crate::rest::import_with_networks(&rest, &|_| None, networks, crate::rest::MAX_NODES).unwrap();
    let (plain, _) = import(&jam(None, false, false, "ELBOWED"), "Control").unwrap();
    assert_eq!(points(&doc, false), points(&plain, false));
    assert!(routed(&warnings));
    assert!(points(&doc, false).iter().all(|(x, y)| x.is_finite() && y.is_finite()));
}

#[test]
fn elbow_control_metadata_does_not_change_straight_or_curved_routes() {
    for style in ["STRAIGHT", "CURVED"] {
        let (plain, _) = import(&jam(None, false, false, style), "Control").unwrap();
        let (metadata, warnings) = import(&jam(Some(controls()), false, false, style), "Other style").unwrap();
        assert_eq!(points(&metadata, false), points(&plain, false), "{style}");
        assert_eq!(routed(&warnings), style == "CURVED");
    }
}

#[test]
fn saved_attached_elbow_endpoints_are_source_geometry_not_native_board_gaps() {
    let (doc, warnings) = import(&jam(Some(controls()), false, false, "ELBOWED"), "Saved endpoint contract").unwrap();
    assert_eq!(points(&doc, false), [(0.0, -20.0), (0.0, 65.0), (140.0, 65.0), (140.0, 210.0), (260.0, 210.0)]);
    assert!(!routed(&warnings), "{warnings:?}");
}

#[test]
fn a_saved_two_point_outward_run_keeps_its_path_and_label() {
    let controls = vec![control(70.0, -18.0, true), control(140.0, 130.0, false)];
    let (doc, warnings) = import(&jam(Some(controls), true, false, "ELBOWED"), "Short saved outward run").unwrap();
    let expected = [(0.0, -20.0), (0.0, -18.0), (140.0, -18.0), (140.0, 210.0), (260.0, 210.0)];
    assert_eq!(points(&doc, true), expected, "a valid short saved run must not be regenerated");
    assert!(!routed(&warnings), "{warnings:?}");
    let root = named(&doc, "Saved route");
    let centre = doc.world_bounds(root.children[1]).unwrap().center();
    // The490-unit original polyline's halfway point is103 units into its228-unit vertical run.
    assert!((centre.x - 140.0).abs() < 1e-5 && (centre.y - 85.0).abs() < 1e-5, "{centre:?}");
    let (reopened, _) = crate::load_bytes(&crate::save_bytes(&doc, None).unwrap()).unwrap();
    assert_eq!(points(&reopened, true), expected);
}

#[test]
fn saved_attached_relative_positions_keep_off_centre_endpoints() {
    let mut message = saved_message(Some(controls()), false, false, "ELBOWED");
    let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
    let connector = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("Saved route")).unwrap();
    let start = field_mut(connector, "connectorStart").unwrap();
    *field_mut(start, "relativePosition").unwrap() = xy(0.25, 1.0);
    let end = field_mut(connector, "connectorEnd").unwrap();
    *field_mut(end, "relativePosition").unwrap() = xy(0.0, 0.25);
    let (doc, warnings) = import(&canvas(&saved_schema(), &message), "Saved off-centre endpoints").unwrap();
    assert_eq!(points(&doc, false), [(-20.0, -20.0), (-20.0, 65.0), (140.0, 65.0), (140.0, 195.0), (260.0, 195.0)]);
    assert!(!routed(&warnings), "{warnings:?}");
}

#[test]
fn metadata_absent_imports_and_native_routes_still_use_the_automatic_gap() {
    use figcraft_doc::{RouteEnd, Routing, Side, route_points};
    let from = RouteEnd::Box(figcraft_doc::geom::Rect::new(-40.0, -80.0, 40.0, -20.0), Side::Bottom);
    let to = RouteEnd::Box(figcraft_doc::geom::Rect::new(260.0, 180.0, 360.0, 240.0), Side::Left);
    let native = route_points(from, to, Routing::Straight).unwrap();
    assert_eq!(native, [figcraft_doc::geom::Point::new(0.0, -16.0), figcraft_doc::geom::Point::new(256.0, 210.0)]);
    let (doc, warnings) = import(&jam(None, false, false, "ELBOWED"), "Unspecified path").unwrap();
    let path = points(&doc, false);
    assert_eq!(path[0], (0.0, -16.0));
    assert_eq!(*path.last().unwrap(), (256.0, 210.0));
    assert!(routed(&warnings));
}

#[test]
fn saved_attached_runs_without_relative_positions_keep_resolved_border_points() {
    let mut message = saved_message(Some(controls()), false, false, "ELBOWED");
    let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
    let connector = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("Saved route")).unwrap();
    *field_mut(connector, "connectorStart").unwrap() = o(vec![("endpointNodeID", g(10)), ("magnet", e("BOTTOM"))]);
    *field_mut(connector, "connectorEnd").unwrap() = o(vec![("endpointNodeID", g(11)), ("magnet", e("LEFT"))]);
    let (doc, warnings) = import(&canvas(&saved_schema(), &message), "Resolved original borders").unwrap();
    assert_eq!(points(&doc, false), [(0.0, -20.0), (0.0, 65.0), (140.0, 65.0), (140.0, 210.0), (260.0, 210.0)]);
    assert!(!routed(&warnings), "{warnings:?}");
}

fn transformed_message(points: Option<Vec<Kv<'static>>>, coefficients: [f64; 4]) -> Kv<'static> {
    let mut message = saved_message(points, false, false, "ELBOWED");
    let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
    let start = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("North box")).unwrap();
    let mut transform = at(-40.0, -80.0);
    for (name, value) in ["m00", "m01", "m10", "m11"].into_iter().zip(coefficients) {
        *field_mut(&mut transform, name).unwrap() = f(value);
    }
    *field_mut(start, "transform").unwrap() = transform;
    message
}

#[test]
fn positive_axis_scaled_attached_endpoints_keep_original_coordinates() {
    let message = transformed_message(Some(controls()), [2.0, 0.0, 0.0, 0.5]);
    let (doc, warnings) = import(&canvas(&saved_schema(), &message), "Scaled source borders").unwrap();
    assert_eq!(points(&doc, false), [(40.0, -50.0), (40.0, 65.0), (140.0, 65.0), (140.0, 210.0), (260.0, 210.0)]);
    assert!(!routed(&warnings), "{warnings:?}");
}

#[test]
fn unsupported_attached_transforms_use_truthful_regeneration() {
    for (name, transform) in [
        ("rotated", [0.0, -1.0, 1.0, 0.0]),
        ("sheared", [1.0, 0.5, 0.0, 1.0]),
        ("reflected", [-1.0, 0.0, 0.0, 1.0]),
        ("singular", [0.0, 0.0, 0.0, 1.0]),
    ] {
        let (plain, _) = import(&canvas(&saved_schema(), &transformed_message(None, transform)), "Fallback control").unwrap();
        let (saved, warnings) =
            import(&canvas(&saved_schema(), &transformed_message(Some(controls()), transform)), "Unsupported endpoint transform").unwrap();
        assert_eq!(points(&saved, false), points(&plain, false), "{name} transform keeps the existing generated fallback");
        assert!(routed(&warnings), "{name}: unsupported transformed metadata must not claim preservation: {warnings:?}");
    }
}

// Original cubic coordinates, not copied from any published route or project attachment.
fn bezier_controls() -> Vec<Kv<'static>> {
    vec![o(vec![("position", xy(120.0, -80.0))]), o(vec![("position", xy(240.0, 300.0)), ("axis", xy(0.25, -0.75))])]
}

fn bezier_message(controls: Option<Vec<Kv<'static>>>, free: bool, labelled: bool, nested: bool) -> Kv<'static> {
    let mut message = saved_message(None, labelled, nested, "CURVED");
    let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
    let connector = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("Saved route")).unwrap();
    if free {
        *field_mut(connector, "connectorStart").unwrap() = o(vec![("position", xy(0.0, 0.0))]);
        *field_mut(connector, "connectorEnd").unwrap() = o(vec![("position", xy(360.0, 220.0))]);
    }
    if let Some(controls) = controls {
        let Kv::Object(fields) = connector else { panic!() };
        fields.push(("connectorBezierControlPoints", arr(controls)));
    }
    message
}

fn bezier_archive(message: &Kv<'_>) -> Vec<u8> {
    let mut payload = canvas(&saved_schema(), message);
    payload[..8].copy_from_slice(JAM_HEADER);
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("canvas.fig", options).unwrap();
    zip.write_all(&payload).unwrap();
    zip.finish().unwrap().into_inner()
}

fn cubic(doc: &Document, labelled: bool) -> [(f64, f64); 4] {
    let line = line(doc, labelled);
    let NodeKind::Vector { network } = &line.kind else { panic!("connector line is a vector") };
    assert_eq!(network.vertices.len(), 2);
    assert_eq!(network.segments.len(), 1);
    let segment = network.segments[0];
    let (from, to) = (network.vertices[segment.a].point(), network.vertices[segment.b].point());
    let world = doc.world_transform(line.id);
    [from, from + figcraft_doc::geom::Vec2::from(segment.ta), to + figcraft_doc::geom::Vec2::from(segment.tb), to].map(|p| {
        let p = world * p;
        (p.x, p.y)
    })
}

fn close_cubic(actual: [(f64, f64); 4], expected: [(f64, f64); 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual.0 - expected.0).abs() < 1e-4 && (actual.1 - expected.1).abs() < 1e-4, "{actual:?} != {expected:?}");
    }
}

#[test]
fn saved_bezier_metadata_preserves_two_ordered_handles_style_and_native_roundtrip() {
    let message = bezier_message(Some(bezier_controls()), true, false, false);
    keep_requested_original_bezier_fixture(&bezier_archive(&message));
    let expected = [(0.0, 0.0), (120.0, -80.0), (240.0, 300.0), (360.0, 220.0)];
    for fixture in [canvas(&saved_schema(), &message), bezier_archive(&message)] {
        let (doc, warnings) = import(&fixture, "Original cubic").unwrap();
        close_cubic(cubic(&doc, false), expected);
        assert!(!routed(&warnings), "preserved controls must not claim regenerated routing: {warnings:?}");
        let line = line(&doc, false);
        assert_eq!(line.stroke.weight, 3.0);
        assert_eq!(line.stroke.dashes, [7.0, 3.0]);
        assert_eq!(line.stroke.cap_start, StrokeCap::Round);
        assert_eq!(line.stroke.cap_end, StrokeCap::TriangleArrow);
        assert_eq!(line.strokes.len(), 1);
        assert!(line.board.is_none(), "imported artwork remains a plain editable vector");
        let (reopened, _) = crate::load_bytes(&crate::save_bytes(&doc, None).unwrap()).unwrap();
        close_cubic(cubic(&reopened, false), expected);
    }
}

#[test]
fn saved_bezier_metadata_uses_original_attached_endpoints_without_native_gaps() {
    let message = bezier_message(Some(bezier_controls()), false, false, false);
    let (doc, warnings) = import(&bezier_archive(&message), "Original attached cubic").unwrap();
    close_cubic(cubic(&doc, false), [(0.0, -20.0), (120.0, -80.0), (240.0, 300.0), (260.0, 210.0)]);
    assert!(!routed(&warnings), "{warnings:?}");
}

#[test]
fn saved_bezier_metadata_keeps_parent_space_and_arc_length_label_midpoint() {
    let message = bezier_message(Some(bezier_controls()), true, true, true);
    let (doc, warnings) = import(&bezier_archive(&message), "Translated cubic").unwrap();
    close_cubic(cubic(&doc, true), [(-320.0, 75.0), (-200.0, -5.0), (-80.0, 375.0), (40.0, 295.0)]);
    assert!(!routed(&warnings), "{warnings:?}");
    let root = named(&doc, "Saved route");
    let centre = doc.world_bounds(root.children[1]).unwrap().center();
    // Central symmetry makes the arc-length midpoint the cubic's t=.5 point.
    assert!((centre.x + 140.0).abs() < 0.05 && (centre.y - 185.0).abs() < 0.05, "{centre:?}");
    let (reopened, _) = crate::load_bytes(&crate::save_bytes(&doc, None).unwrap()).unwrap();
    close_cubic(cubic(&reopened, true), cubic(&doc, true));
}

#[test]
fn malformed_saved_bezier_metadata_keeps_existing_generated_geometry_and_warning() {
    let (plain, _) = import(&bezier_archive(&bezier_message(None, true, false, false)), "Control").unwrap();
    for controls in [
        vec![],
        vec![o(vec![("position", xy(120.0, -80.0))])],
        vec![o(vec![("axis", xy(1.0, 0.0))]), o(vec![("position", xy(240.0, 300.0))])],
        (0..65).map(|_| o(vec![("position", xy(120.0, -80.0))])).collect(),
    ] {
        let (doc, warnings) = import(&bezier_archive(&bezier_message(Some(controls), true, false, false)), "Malformed curve").unwrap();
        close_cubic(cubic(&doc, false), cubic(&plain, false));
        assert!(routed(&warnings), "{warnings:?}");
    }
}

fn saved_connector_mut<'a>(message: &'a mut Kv<'static>) -> &'a mut Kv<'static> {
    let Kv::Array(nodes) = field_mut(message, "nodeChanges").unwrap() else { panic!() };
    nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("Saved route")).unwrap()
}

fn decoded_bezier(message: &Kv<'_>) -> (Document, Vec<String>) {
    let (rest, warnings, networks) = super::super::map::to_rest(message, "Decoded original cubic", crate::rest::MAX_NODES);
    let (doc, _) = crate::rest::import_with_networks(&rest, &|_| None, networks, crate::rest::MAX_NODES).unwrap();
    (doc, warnings)
}

#[test]
fn saved_bezier_metadata_rejects_nonfinite_wrong_shaped_and_oversized_decoded_controls() {
    let (plain, _) = decoded_bezier(&bezier_message(None, true, false, false));
    let invalid = [
        ("nonfinite position", vec![o(vec![("position", xy(f64::NAN, 0.0))]), o(vec![("position", xy(240.0, 300.0))])]),
        ("infinite position", vec![o(vec![("position", xy(120.0, f64::INFINITY))]), o(vec![("position", xy(240.0, 300.0))])]),
        ("nonfinite axis", vec![o(vec![("position", xy(120.0, -80.0)), ("axis", xy(f64::INFINITY, 0.0))]), o(vec![("position", xy(240.0, 300.0))])]),
        ("wrong axis shape", vec![o(vec![("position", xy(120.0, -80.0)), ("axis", s("invalid"))]), o(vec![("position", xy(240.0, 300.0))])]),
        ("wrong record shape", vec![s("invalid"), o(vec![("position", xy(240.0, 300.0))])]),
        ("coordinate cap", vec![o(vec![("position", xy(1e8, 0.0))]), o(vec![("position", xy(240.0, 300.0))])]),
        ("output size cap", vec![o(vec![("position", xy(-8e6, -8e6))]), o(vec![("position", xy(8e6, 8e6))])]),
        ("three controls", vec![o(vec![("position", xy(120.0, -80.0))]); 3]),
    ];
    for (name, controls) in invalid {
        let (doc, warnings) = decoded_bezier(&bezier_message(Some(controls), true, false, false));
        close_cubic(cubic(&doc, false), cubic(&plain, false));
        assert!(routed(&warnings), "{name}: {warnings:?}");
    }
    // Validate tangent deltas too: both absolute handles are within the coordinate cap.
    let mut message = bezier_message(Some(vec![o(vec![("position", xy(-9e6, 0.0))]), o(vec![("position", xy(240.0, 300.0))])]), true, false, false);
    *field_mut(saved_connector_mut(&mut message), "connectorStart").unwrap() = o(vec![("position", xy(9e6, 0.0))]);
    let (doc, warnings) = decoded_bezier(&message);
    assert!(routed(&warnings), "an out-of-range tangent cannot claim preservation: {warnings:?}");
    assert!(cubic(&doc, false).into_iter().all(|(x, y)| x.is_finite() && y.is_finite()));
}

#[test]
fn saved_bezier_metadata_keeps_relative_positions_resolved_borders_and_scaled_endpoints() {
    for (name, relative, scale, expected) in [
        ("off centre", Some((0.25, 1.0)), false, [(-20.0, -20.0), (120.0, -80.0), (240.0, 300.0), (260.0, 210.0)]),
        ("resolved border", None, false, [(0.0, -20.0), (120.0, -80.0), (240.0, 300.0), (260.0, 210.0)]),
        ("positive scale", Some((0.5, 1.0)), true, [(40.0, -50.0), (120.0, -80.0), (240.0, 300.0), (260.0, 210.0)]),
    ] {
        let mut message = bezier_message(Some(bezier_controls()), false, false, false);
        let mut endpoint = vec![("endpointNodeID", g(10)), ("magnet", e("BOTTOM"))];
        if let Some((x, y)) = relative {
            endpoint.push(("relativePosition", xy(x, y)));
        }
        *field_mut(saved_connector_mut(&mut message), "connectorStart").unwrap() = o(endpoint);
        if scale {
            let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
            let start = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("North box")).unwrap();
            let transform = field_mut(start, "transform").unwrap();
            *field_mut(transform, "m00").unwrap() = f(2.0);
            *field_mut(transform, "m11").unwrap() = f(0.5);
        }
        let (doc, warnings) = import(&bezier_archive(&message), name).unwrap();
        close_cubic(cubic(&doc, false), expected);
        assert!(!routed(&warnings), "{name}: {warnings:?}");
    }
}

#[test]
fn saved_bezier_metadata_rejects_unsupported_attached_transforms_truthfully() {
    for coefficients in [[0.0, -1.0, 1.0, 0.0], [1.0, 0.5, 0.0, 1.0], [-1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]] {
        let mut message = bezier_message(None, false, false, false);
        let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
        let start = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("North box")).unwrap();
        let transform = field_mut(start, "transform").unwrap();
        for (name, value) in ["m00", "m01", "m10", "m11"].into_iter().zip(coefficients) {
            *field_mut(transform, name).unwrap() = f(value);
        }
        let (plain, _) = import(&bezier_archive(&message), "Fallback control").unwrap();
        let Kv::Object(fields) = saved_connector_mut(&mut message) else { panic!() };
        fields.push(("connectorBezierControlPoints", arr(bezier_controls())));
        let (doc, warnings) = import(&bezier_archive(&message), "Unsupported attached transform").unwrap();
        close_cubic(cubic(&doc, false), cubic(&plain, false));
        assert!(routed(&warnings), "{coefficients:?}: {warnings:?}");
    }
}

#[test]
fn saved_bezier_metadata_does_not_override_an_accepted_vector_snapshot_or_straight_line() {
    use super::super::network::{self, Network, Segment, Vertex};
    let snapshot = network::encode(&Network {
        vertices: vec![Vertex { style: 0, x: 10.0, y: 20.0 }, Vertex { style: 0, x: 90.0, y: 60.0 }],
        segments: vec![Segment { style: 0, a: 0, b: 1, ta: (15.0, -10.0), tb: (-20.0, 25.0) }],
        regions: vec![],
    });
    let mut message = bezier_message(None, true, false, false);
    let Kv::Object(fields) = saved_connector_mut(&mut message) else { panic!() };
    fields.push(("vectorData", o(vec![("vectorNetworkBlob", u(0)), ("normalizedSize", xy(100.0, 100.0))])));
    fields.push(("size", xy(100.0, 100.0)));
    *field_mut(&mut message, "blobs").unwrap() = arr(vec![o(vec![("bytes", Kv::Bytes(snapshot))])]);
    let (plain, plain_warnings) = import(&bezier_archive(&message), "Snapshot control").unwrap();
    assert!(!routed(&plain_warnings));
    let Kv::Object(fields) = saved_connector_mut(&mut message) else { panic!() };
    fields.push(("connectorBezierControlPoints", arr(bezier_controls())));
    let (doc, warnings) = import(&bezier_archive(&message), "Snapshot with metadata").unwrap();
    close_cubic(cubic(&doc, false), cubic(&plain, false));
    assert!(!routed(&warnings), "{warnings:?}");

    for controls in [None, Some(bezier_controls())] {
        let mut message = bezier_message(controls, true, false, false);
        *field_mut(saved_connector_mut(&mut message), "connectorLineStyle").unwrap() = e("STRAIGHT");
        let (doc, warnings) = import(&bezier_archive(&message), "Straight control").unwrap();
        assert_eq!(points(&doc, false), [(0.0, 0.0), (360.0, 220.0)]);
        let NodeKind::Vector { network } = &line(&doc, false).kind else { panic!() };
        assert!(network.segments.iter().all(figcraft_doc::Segment::is_straight));
        assert!(!routed(&warnings), "{warnings:?}");
    }
}

#[test]
fn saved_paths_do_not_claim_preservation_after_an_invalid_attached_transform_defaults_to_identity() {
    for style in ["ELBOWED", "CURVED"] {
        for transform in [
            o(vec![("m00", f(f64::NAN)), ("m01", f(0.0)), ("m02", f(-40.0)), ("m10", f(0.0)), ("m11", f(1.0)), ("m12", f(-80.0))]),
            s("invalid transform"),
        ] {
            let mut message = if style == "ELBOWED" {
                saved_message(Some(controls()), false, false, style)
            } else {
                bezier_message(Some(bezier_controls()), false, false, false)
            };
            let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
            let start = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("North box")).unwrap();
            *field_mut(start, "transform").unwrap() = transform;
            let (doc, warnings) = decoded_bezier(&message);
            assert!(routed(&warnings), "{style}: an invalid source transform cannot claim a saved path was preserved: {warnings:?}");
            assert!(points(&doc, false).into_iter().all(|(x, y)| x.is_finite() && y.is_finite()));
        }
    }
}

#[test]
fn saved_bezier_metadata_accepts_absent_and_explicit_identity_endpoint_transforms() {
    for absent in [false, true] {
        let mut message = bezier_message(Some(bezier_controls()), false, false, false);
        let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
        let start = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("North box")).unwrap();
        if absent {
            let Kv::Object(fields) = start else { panic!() };
            fields.retain(|(key, _)| *key != "transform");
        } else {
            *field_mut(start, "transform").unwrap() = at(0.0, 0.0);
        }
        let (doc, warnings) = import(&bezier_archive(&message), "Valid identity endpoint").unwrap();
        close_cubic(cubic(&doc, false), [(40.0, 60.0), (120.0, -80.0), (240.0, 300.0), (260.0, 210.0)]);
        assert!(!routed(&warnings), "absent={absent}: {warnings:?}");
    }
}

#[test]
fn saved_bezier_metadata_refuses_an_output_transform_outside_the_rest_handoff_limits() {
    for (translation, linear) in [(2e7, 1.0), (75.0, 2e6)] {
        let mut message = bezier_message(Some(bezier_controls()), true, false, true);
        let Kv::Array(nodes) = field_mut(&mut message, "nodeChanges").unwrap() else { panic!() };
        let parent = nodes.iter_mut().find(|n| n.get("name").and_then(Kv::str) == Some("Translated parent")).unwrap();
        // Groups pass their transform to the children's REST coordinates; frames keep it on
        // the enclosing node. Exercise the actual composed handoff rather than guessing space.
        *field_mut(parent, "type").unwrap() = e("GROUP");
        *field_mut(parent, "transform").unwrap() = at(translation, 75.0);
        *field_mut(field_mut(parent, "transform").unwrap(), "m00").unwrap() = f(linear);
        let (_, warnings) = decoded_bezier(&message);
        assert!(routed(&warnings), "a rejected output transform must not claim preservation: {warnings:?}");
    }
}

fn keep_requested_original_bezier_fixture(bytes: &[u8]) {
    let Some(path) = std::env::var_os("FIGCRAFT_TEST_SAVED_BEZIER_FIXTURE_OUT") else { return };
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap();
    file.write_all(bytes).unwrap();
}
