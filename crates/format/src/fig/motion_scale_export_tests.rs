//! Original Scale export controls from the sanitized native/SVG functional contract.
//! No private file, vendor schema dump, identifier, asset or account metadata is copied.

use std::collections::BTreeSet;

use figcraft_doc::anim::motion::{AnimProperty, AnimValue, Clip, ClipKind, ClipMode, Ease, TimelineSettings, Track};
use figcraft_doc::color::{Color, Gradient, GradientKind, Paint};
use figcraft_doc::geom::{Affine, Vec2};
use figcraft_doc::{Document, Effect, Node, NodeId, NodeKind, Style, StyleKind};

use super::kiwi::{Kv, Schema, Type};

const ERROR_PX: f64 = 1.0 / 1024.0;

fn scale_track(end: f64) -> Track {
    let mut track = Track::new(AnimProperty::Scale);
    let curve = Ease::CustomBezier([0.5, 0.0, 0.5, 1.0]);
    track.set_key(0, AnimValue::Vec2(Vec2::new(1.0, 1.0)), Some(curve));
    track.set_key(1000, AnimValue::Vec2(Vec2::new(end, end)), Some(curve));
    track
}

fn scene() -> (Document, NodeId, NodeId) {
    let mut doc = Document::new("Original outward Scale control");
    let page = doc.first_page().unwrap();
    let frame = doc.create(page, NodeKind::Frame, "Original scale frame", 0.0, 0.0, 240.0, 120.0).unwrap();
    doc.node_mut(frame).unwrap().clip = true;
    let child = doc.create(frame, NodeKind::Rectangle, "Original scale rectangle", 10.0, 30.0, 40.0, 40.0).unwrap();
    let rectangle = doc.node_mut(child).unwrap();
    rectangle.fills = vec![Paint::solid(Color::from_rgba8(217, 217, 217, 255))];
    rectangle.motion.tracks.push(scale_track(2.0));
    doc.timelines.insert(page, TimelineSettings { duration_ms: 1000, fps: 60, looping: false });
    (doc, frame, child)
}

fn graph(doc: &Document, check: impl FnOnce(&Schema, &Kv<'_>, &[String])) {
    let (bytes, warnings) = super::export(doc, None).unwrap();
    let archive = super::Archive::open(&bytes).unwrap();
    let (schema, body) = super::decode_canvas(&archive.canvas).unwrap();
    let message = schema.decode_message(schema.find("Message").unwrap(), &body).unwrap();
    check(&schema, &message, &warnings);
}

fn id(value: &Kv<'_>) -> (u64, u64) {
    let integer = |field| match value.get(field).unwrap() {
        Kv::Uint(value) => *value,
        _ => panic!("native GUID must use unsigned fields"),
    };
    (integer("sessionID"), integer("localID"))
}

fn node_id(node: &Kv<'_>) -> (u64, u64) {
    id(node.get("guid").unwrap())
}

fn parent_id(node: &Kv<'_>) -> (u64, u64) {
    id(node.get("parentIndex").unwrap().get("guid").unwrap())
}

fn named<'a>(nodes: &'a [Kv<'a>], name: &str) -> &'a Kv<'a> {
    nodes.iter().find(|node| node.get("name").and_then(Kv::str) == Some(name)).unwrap()
}

fn float(value: &Kv<'_>, field: &str) -> f64 {
    match value.get(field).unwrap() {
        Kv::Float(value) => f64::from(*value),
        _ => panic!("native geometry must be Float32"),
    }
}

fn schema_field(schema: &Schema, definition: &str, name: &str, number: u32, primitive: Option<Type>) {
    let definition = &schema.defs[schema.find(definition).unwrap()];
    let field = definition.fields.iter().find(|field| field.name == name).unwrap();
    assert_eq!(field.value, number, "literal native schema field {name}");
    if let Some(primitive) = primitive {
        assert_eq!(field.ty, Some(primitive));
    }
}

fn binding_track<'a>(node: &'a Kv<'a>, nodes: &'a [Kv<'a>], field: &str, fallback: f32) -> &'a Kv<'a> {
    let entries = node.get("parameterConsumptionMap").unwrap().get("entries").unwrap().list();
    let matches: Vec<_> = entries.iter().filter(|entry| entry.get("variableField").and_then(Kv::str) == Some(field)).collect();
    assert_eq!(matches.len(), 1, "one unambiguous {field} row, preserving all other rows");
    let data = matches[0].get("variableData").unwrap();
    assert_eq!(data.get("dataType").and_then(Kv::str), Some("EXPRESSION"));
    assert_eq!(data.get("resolvedDataType").and_then(Kv::str), Some("VECTOR"));
    let expression = data.get("value").unwrap().get("expressionValue").unwrap();
    assert_eq!(expression.get("expressionFunction").and_then(Kv::str), Some("KEYFRAME"));
    let args = expression.get("expressionArguments").unwrap().list();
    assert_eq!(args.len(), 2);
    assert_eq!(args[0].get("dataType").and_then(Kv::str), Some("VECTOR"));
    assert_eq!(args[0].get("resolvedDataType").and_then(Kv::str), Some("VECTOR"));
    let value = args[0].get("value").unwrap().get("vectorValue").unwrap();
    assert_eq!((value.get("x"), value.get("y")), (Some(&Kv::Float(fallback)), Some(&Kv::Float(fallback))));
    assert_eq!(args[1].get("dataType").and_then(Kv::str), Some("KEYFRAME_TRACK_PARAMETER_DATA"));
    assert_eq!(args[1].get("resolvedDataType").and_then(Kv::str), Some("KEYFRAME_TRACK_PARAMETER_DATA"));
    let parameters = args[1].get("value").unwrap().get("keyframeTrackParameterValue").unwrap().get("parameters").unwrap().list();
    assert_eq!(parameters.len(), 1);
    assert_eq!(parameters[0].get("type").and_then(Kv::str), Some("MANUAL"));
    let reference = parameters[0].get("value").unwrap().get("manual").unwrap().get("keyframeTrackId").unwrap().get("guid").unwrap();
    let track = nodes.iter().find(|node| node_id(node) == id(reference)).unwrap();
    assert_eq!(track.get("type").and_then(Kv::str), Some("KEYFRAME_TRACK"));
    assert_eq!(node_id(track).0, 3);
    track
}

fn keys<'a>(nodes: &'a [Kv<'a>], track: &Kv<'_>) -> Vec<&'a Kv<'a>> {
    let mut keys: Vec<_> =
        nodes.iter().filter(|node| node.get("type").and_then(Kv::str) == Some("KEYFRAME") && parent_id(node) == node_id(track)).collect();
    keys.sort_by_key(|node| match node.get("timelinePosition") {
        None => 0,
        Some(Kv::Int(time)) => *time,
        _ => panic!("native key clock must be Int64 microseconds"),
    });
    keys
}

fn assert_scale_graph<'a>(schema: &Schema, nodes: &'a [Kv<'a>]) -> (&'a Kv<'a>, Vec<&'a Kv<'a>>) {
    assert!(
        nodes.iter().any(|node| node.get("type").and_then(Kv::str) == Some("KEYFRAME_TRACK")),
        "actual emitted bytes must contain editable native Scale"
    );
    schema_field(schema, "VariableField", "MOTION_SCALE_XY", 70, None);
    schema_field(schema, "EasingType", "CUSTOM_CUBIC", 7, None);
    schema_field(schema, "VariableDataType", "VECTOR", 24, None);
    schema_field(schema, "VariableResolvedDataType", "VECTOR", 17, None);
    schema_field(schema, "VariableDataType", "KEYFRAME_TRACK_PARAMETER_DATA", 21, None);
    schema_field(schema, "VariableResolvedDataType", "KEYFRAME_TRACK_PARAMETER_DATA", 14, None);
    schema_field(schema, "ExpressionFunction", "KEYFRAME", 19, None);
    schema_field(schema, "KeyframeTrackParameterType", "MANUAL", 1, None);
    schema_field(schema, "KeyframeValueType", "VECTOR", 4, None);
    schema_field(schema, "NodeType", "KEYFRAME_TRACK", 57, None);
    schema_field(schema, "NodeType", "KEYFRAME", 56, None);
    schema_field(schema, "NodeChange", "parameterConsumptionMap", 445, None);
    schema_field(schema, "NodeChange", "timelinePosition", 506, Some(Type::Int64));
    schema_field(schema, "NodeChange", "keyframeValue", 507, None);
    schema_field(schema, "NodeChange", "timelineDuration", 512, Some(Type::Uint64));
    schema_field(schema, "NodeChange", "easingData", 535, None);
    let rectangle = named(nodes, "Original scale rectangle");
    let track = binding_track(rectangle, nodes, "MOTION_SCALE_XY", 1.0);
    let key_nodes = keys(nodes, track);
    assert_eq!(key_nodes.len(), 2);
    assert_eq!(named(nodes, "Original scale frame").get("timelineDuration"), Some(&Kv::Uint(1_000_000)));
    for (index, key) in key_nodes.iter().enumerate() {
        assert_eq!(node_id(key).0, 3);
        assert_eq!(key.get("timelinePosition"), Some(&Kv::Int(index as i64 * 1_000_000)));
        let value = key.get("keyframeValue").unwrap();
        assert_eq!(value.get("valueType").and_then(Kv::str), Some("VECTOR"));
        let ratio = value.get("value").unwrap().get("vectorValue").unwrap();
        assert!(float(ratio, "x") > 0.0 && float(ratio, "x") <= 100.0);
        assert_eq!(float(ratio, "x"), float(ratio, "y"));
        let easing = key.get("easingData").unwrap();
        assert_eq!(easing.get("easingType").and_then(Kv::str), Some("CUSTOM_CUBIC"));
        let curve = easing.get("easingValue").unwrap().get("bezierEasing").unwrap();
        for (field, value) in [("p1x", 0.5), ("p1y", 0.0), ("p2x", 0.5), ("p2y", 1.0)] {
            assert_eq!(float(curve, field), value);
        }
        assert!(key.get("motionPathBezierHandles").is_none(), "Scale has no Position spatial path payload");
        assert!(key.get("interpolationType").is_none() && key.get("keyframeOperation").is_none());
    }
    let internal: Vec<_> = nodes.iter().filter(|node| node.get("internalOnly").and_then(Kv::bool) == Some(true)).collect();
    assert_eq!(internal.len(), 1, "assets and all properties share one internal canvas");
    assert_eq!(parent_id(track), node_id(internal[0]));
    assert_eq!(nodes.iter().map(node_id).collect::<BTreeSet<_>>().len(), nodes.len());
    (rectangle, key_nodes)
}

#[test]
fn native_scale_export_has_literal_manual_ratio_graph_and_preserves_source() {
    let (doc, _, _) = scene();
    let before = crate::to_json(&doc).unwrap();
    graph(&doc, |schema, message, warnings| {
        let nodes = message.get("nodeChanges").unwrap().list();
        let (rectangle, keys) = assert_scale_graph(schema, nodes);
        assert_eq!(nodes.len(), 8);
        let values: Vec<_> =
            keys.iter().map(|key| float(key.get("keyframeValue").unwrap().get("value").unwrap().get("vectorValue").unwrap(), "x")).collect();
        assert_eq!(values, [1.0, 2.0], "native Scale uses ratios, never percentages or Position offsets");
        assert_eq!((float(rectangle.get("transform").unwrap(), "m02"), float(rectangle.get("transform").unwrap(), "m12")), (10.0, 30.0));
        assert_eq!((float(rectangle.get("size").unwrap(), "x"), float(rectangle.get("size").unwrap(), "y")), (40.0, 40.0));
        assert!(warnings.iter().any(|warning| warning.contains("fps") && warning.contains("unverified")), "{warnings:?}");
        assert!(!warnings.iter().any(|warning| warning.contains("omitted") && warning.contains("tracks")), "{warnings:?}");
    });
    assert_eq!(crate::to_json(&doc).unwrap(), before);
}

#[test]
fn native_scale_export_two_pages_keep_position_scale_assets_and_unique_resources() {
    let (mut doc, _, _) = scene();
    let page = doc.add_page("Original Position page");
    let frame = doc.create(page, NodeKind::Frame, "Original Position frame", 0.0, 0.0, 240.0, 120.0).unwrap();
    let child = doc.create(frame, NodeKind::Rectangle, "Original Position rectangle", 10.0, 30.0, 40.0, 40.0).unwrap();
    let mut position = Track::new(AnimProperty::Position);
    position.set_key(0, AnimValue::Vec2(Vec2::new(10.0, 30.0)), Some(Ease::Linear));
    position.set_key(1000, AnimValue::Vec2(Vec2::new(180.0, 30.0)), Some(Ease::Linear));
    doc.node_mut(child).unwrap().motion.tracks.push(position);
    doc.timelines.insert(page, TimelineSettings { duration_ms: 1000, fps: 60, looping: false });
    doc.styles.push(Style {
        id: "original-scale-style".into(),
        name: "Original solid".into(),
        description: String::new(),
        kind: StyleKind::Paint { paints: vec![Paint::solid(Color::BLACK)] },
        bound: Default::default(),
    });
    let before = crate::to_json(&doc).unwrap();
    graph(&doc, |schema, message, warnings| {
        let nodes = message.get("nodeChanges").unwrap().list();
        assert_eq!(
            nodes.iter().filter(|node| node.get("type").and_then(Kv::str) == Some("KEYFRAME_TRACK")).count(),
            2,
            "one actual editable track for each property"
        );
        assert_scale_graph(schema, nodes);
        let position = binding_track(named(nodes, "Original Position rectangle"), nodes, "MOTION_TRANSLATION_XY", 0.0);
        let keys = keys(nodes, position);
        assert_eq!(keys.len(), 2);
        assert_eq!(float(keys[1].get("keyframeValue").unwrap().get("value").unwrap().get("vectorValue").unwrap(), "x"), 170.0);
        assert!(keys.iter().all(|key| key.get("easingData").unwrap().get("easingType").and_then(Kv::str) == Some("LINEAR")
            && key.get("motionPathBezierHandles").is_some()));
        assert_eq!(named(nodes, "Original Position frame").get("timelineDuration"), Some(&Kv::Uint(1_000_000)));
        assert!(warnings.iter().any(|warning| warning.contains("unverified") && warning.contains("2 frame durations")), "{warnings:?}");
        graph(&doc, |_, again, _| assert_eq!(again, message, "repeat export must keep the deterministic decoded graph"));
    });
    assert_eq!(crate::to_json(&doc).unwrap(), before);
}

fn assert_world_corners(doc: &Document, frame: NodeId, child: NodeId) {
    graph(doc, |schema, message, _| {
        let nodes = message.get("nodeChanges").unwrap().list();
        let (encoded, keys) = assert_scale_graph(schema, nodes);
        let parent = named(nodes, "Original scale frame");
        let source = doc.node(child).unwrap();
        let owner = doc.node(frame).unwrap();
        let a = owner.transform.as_coeffs();
        let b = source.transform.as_coeffs();
        let fa = parent.get("transform").unwrap();
        let fb = encoded.get("transform").unwrap();
        let size = encoded.get("size").unwrap();
        let frame_size = parent.get("size").unwrap();
        for (base, extent, field, dimension) in [(a[4], owner.width, "m02", "x"), (a[5], owner.height, "m12", "y")] {
            for (source_edge, wire_edge) in [(base, float(fa, field)), (base + extent, float(fa, field) + float(frame_size, dimension))] {
                assert!(
                    (source_edge - wire_edge).abs() <= ERROR_PX && source_edge.abs() <= 1e9 && wire_edge.abs() <= 1e9,
                    "frame clip world edge lost precision"
                );
            }
        }
        for (source_key, key) in source.motion.tracks[0].keys.iter().zip(keys) {
            let AnimValue::Vec2(scale) = source_key.value else { panic!("original control uses ratios") };
            let ratio = key.get("keyframeValue").unwrap().get("value").unwrap().get("vectorValue").unwrap();
            for (field, dimension, base, local, owner_extent, extent, factor) in
                [("m02", "x", a[4], b[4], owner.width, source.width, scale.x), ("m12", "y", a[5], b[5], owner.height, source.height, scale.y)]
            {
                let wire_base = float(fa, field);
                let wire_local = float(fb, field);
                let wire_extent = float(size, dimension);
                let wire_factor = float(ratio, dimension);
                for sign in [-1.0, 1.0] {
                    let source_corner = base + local + extent * 0.5 + sign * extent * factor * 0.5;
                    let wire_corner = wire_base + wire_local + wire_extent * 0.5 + sign * wire_extent * wire_factor * 0.5;
                    assert!(source_corner.is_finite() && wire_corner.is_finite() && source_corner.abs() <= 1e9 && wire_corner.abs() <= 1e9);
                    assert!(
                        (source_corner - wire_corner).abs() <= ERROR_PX,
                        "full decoded Float32 world corner differs: source={source_corner}, decoded={wire_corner}"
                    );
                    if owner.clip {
                        let clipped = source_corner.clamp(base, base + owner_extent);
                        let wire_clipped = wire_corner.clamp(wire_base, wire_base + float(frame_size, dimension));
                        assert!((clipped - wire_clipped).abs() <= ERROR_PX, "frame-clipped corner lost precision");
                    }
                }
            }
        }
    });
}

fn omitted(doc: &Document, reason: &str) {
    graph(doc, |_, message, warnings| {
        let nodes = message.get("nodeChanges").unwrap().list();
        assert!(
            !nodes
                .iter()
                .flat_map(|node| node.get("parameterConsumptionMap").and_then(|map| map.get("entries")).map(Kv::list).unwrap_or_default())
                .any(|entry| entry.get("variableField").and_then(Kv::str) == Some("MOTION_SCALE_XY"))
        );
        assert!(warnings.iter().any(|warning| warning.contains("omitted") && warning.contains(reason)), "{reason}: {warnings:?}");
        assert!(warnings.iter().any(|warning| warning.contains(".figcraft")), "{warnings:?}");
    });
}

#[test]
fn native_scale_export_world_corner_precision_checks_ratio_dimension_and_both_translations() {
    let (doc, frame, child) = scene();
    assert_world_corners(&doc, frame, child);
    let (mut dimension, frame, child) = scene();
    dimension.node_mut(child).unwrap().width = 1000.000001;
    dimension.node_mut(child).unwrap().motion.tracks[0] = scale_track(100.0);
    assert_world_corners(&dimension, frame, child);
    dimension.node_mut(child).unwrap().width = 1000.00003;
    omitted(&dimension, "precision");
    let (mut ratio, frame, child) = scene();
    ratio.node_mut(child).unwrap().width = 1_000_000.0;
    ratio.node_mut(child).unwrap().motion.tracks[0] = scale_track(1.0000000001);
    assert_world_corners(&ratio, frame, child);
    ratio.node_mut(child).unwrap().motion.tracks[0] = scale_track(1.00000003);
    omitted(&ratio, "precision");
    let (mut translations, frame, child) = scene();
    translations.node_mut(frame).unwrap().transform = Affine::translate((16_384.000_2, 0.0));
    translations.node_mut(child).unwrap().transform = Affine::translate((16_384.000_2, 30.0));
    assert_world_corners(&translations, frame, child);
    translations.node_mut(frame).unwrap().transform = Affine::translate((16_384.000_8, 0.0));
    translations.node_mut(child).unwrap().transform = Affine::translate((16_384.000_8, 30.0));
    omitted(&translations, "precision");
}

#[test]
fn native_scale_export_normal_ratio_and_world_range_limits_have_positive_controls() {
    for ratio in [f64::from(f32::MIN_POSITIVE), 100.0] {
        let (mut doc, frame, child) = scene();
        doc.node_mut(child).unwrap().motion.tracks[0] = scale_track(ratio);
        assert_world_corners(&doc, frame, child);
    }
    for ratio in [0.0, -1.0, 100.000001, 1e-40, f64::INFINITY, f64::NAN] {
        let (mut doc, _, child) = scene();
        doc.node_mut(child).unwrap().motion.tracks[0] = scale_track(ratio);
        omitted(&doc, "scale");
    }
    let (mut world, frame, child) = scene();
    world.node_mut(frame).unwrap().transform = Affine::translate((999_999_744.0, 0.0));
    world.node_mut(child).unwrap().transform = Affine::translate((150.0, 30.0));
    assert_world_corners(&world, frame, child);
    world.node_mut(child).unwrap().transform = Affine::translate((200.0, 30.0));
    omitted(&world, "range");
}

#[test]
fn native_scale_export_unsupported_decoration_curve_and_composition_are_omitted() {
    let (doc, frame, child) = scene();
    assert_world_corners(&doc, frame, child);
    type Change = fn(&mut Node);
    let changes: [(Change, &str); 12] = [
        (|node| node.corners.radii = [2.0; 4], "decoration"),
        (|node| node.corners.smoothing = 0.2, "decoration"),
        (|node| node.strokes.push(Paint::solid(Color::BLACK)), "decoration"),
        (|node| node.effects.push(Effect::drop_shadow()), "decoration"),
        (|node| node.fills.push(Paint::solid(Color::BLACK)), "decoration"),
        (|node| node.fills.clear(), "decoration"),
        (|node| node.fills[0] = Paint::gradient(Gradient::default_for(GradientKind::Linear, Color::BLACK)), "decoration"),
        (|node| node.motion.tracks[0].keys[0].easing = Ease::Linear, "easing"),
        (|node| node.motion.tracks[0].keys[1].value = AnimValue::Vec2(Vec2::new(2.0, 3.0)), "uniform"),
        (|node| node.motion.tracks[0].visible = false, "hidden track"),
        (|node| node.motion.tracks[0].keys[1].value = AnimValue::Scalar(2.0), "Vec2"),
        (|node| node.motion.clips.push(Clip::new(ClipKind::Opacity, ClipMode::In)), "composition"),
    ];
    for (change, reason) in changes {
        let (mut doc, _, child) = scene();
        change(doc.node_mut(child).unwrap());
        let before = crate::to_json(&doc).unwrap();
        omitted(&doc, reason);
        assert_eq!(crate::to_json(&doc).unwrap(), before);
    }
}

#[test]
fn native_scale_export_resource_budget_counts_scale_and_mixed_properties_keep_position_admission() {
    let (doc, _, _) = scene();
    assert!(super::export::export_with_test_node_budget(&doc, 8).is_ok());
    assert!(super::export::export_with_test_node_budget(&doc, 7).is_err(), "count the emitted Scale track and keys before allocation");
    let (mut duplicate, _, child) = scene();
    let second = duplicate.node(child).unwrap().motion.tracks[0].clone();
    duplicate.node_mut(child).unwrap().motion.tracks.push(second);
    omitted(&duplicate, "composition");
    let (mut mixed, _, child) = scene();
    let mut position = Track::new(AnimProperty::Position);
    position.set_key(0, AnimValue::Vec2(Vec2::new(10.0, 30.0)), Some(Ease::Linear));
    position.set_key(1000, AnimValue::Vec2(Vec2::new(180.0, 30.0)), Some(Ease::Linear));
    mixed.node_mut(child).unwrap().motion.tracks.push(position);
    omitted(&mixed, "composition");
    graph(&mixed, |_, message, warnings| {
        binding_track(
            named(message.get("nodeChanges").unwrap().list(), "Original scale rectangle"),
            message.get("nodeChanges").unwrap().list(),
            "MOTION_TRANSLATION_XY",
            0.0,
        );
        assert!(
            warnings.iter().any(|warning| warning.contains("1 Position") && warning.contains("0 Scale") && warning.contains("omitted 1 tracks")),
            "{warnings:?}"
        );
    });
}

#[test]
fn native_scale_export_combined_frame_clip_world_edges_require_the_same_precision() {
    let (mut doc, frame, child) = scene();
    doc.node_mut(child).unwrap().width = 20_000.0;
    doc.node_mut(frame).unwrap().transform = Affine::translate((16_384.000_2, 0.0));
    doc.node_mut(frame).unwrap().width = 16_384.000_2;
    assert_world_corners(&doc, frame, child);
    doc.node_mut(frame).unwrap().transform = Affine::translate((16_384.000_8, 0.0));
    doc.node_mut(frame).unwrap().width = 16_384.000_8;
    // Each independent Float32 cast passes, but the actually clipped right edge loses .0016px.
    let value = 16_384.000_8;
    assert!((value - f64::from(value as f32)).abs() < ERROR_PX);
    assert!((value + value - 2.0 * f64::from(value as f32)).abs() > ERROR_PX);
    omitted(&doc, "precision");
}

#[test]
fn native_scale_export_rejects_unrepresented_page_transforms_without_changing_position_rules() {
    let (doc, frame, child) = scene();
    assert_world_corners(&doc, frame, child);
    for transform in [Affine::translate((1.0, 0.0)), Affine::scale(2.0)] {
        let (mut doc, _, _) = scene();
        let page = doc.first_page().unwrap();
        doc.node_mut(page).unwrap().transform = transform;
        omitted(&doc, "page transform");
    }
}

#[test]
fn native_scale_export_rejects_unresolved_geometry_composition_but_keeps_detached_static_k() {
    let (mut detached, frame, child) = scene();
    let page = detached.first_page().unwrap();
    let static_node = detached.create(page, NodeKind::Rectangle, "Original detached static K", 300.0, 0.0, 40.0, 40.0).unwrap();
    detached.node_mut(static_node).unwrap().variable_scale = Some(2.0);
    assert_world_corners(&detached, frame, child);
    for owner in [false, true] {
        let (mut alias, frame, child) = scene();
        alias.node_mut(if owner { frame } else { child }).unwrap().bound.insert("width".into(), "original-unresolved-alias".into());
        omitted(&alias, "composition");
        let (mut k, frame, child) = scene();
        k.node_mut(if owner { frame } else { child }).unwrap().variable_scale = Some(2.0);
        omitted(&k, "composition");
        let (mut derived, frame, child) = scene();
        derived.node_mut(if owner { frame } else { child }).unwrap().scale_derived_overrides.insert("width".into());
        omitted(&derived, "composition");
    }
    let (mut source_owned, frame, child) = scene();
    source_owned.node_mut(frame).unwrap().source = Some(child);
    omitted(&source_owned, "composition");
}
