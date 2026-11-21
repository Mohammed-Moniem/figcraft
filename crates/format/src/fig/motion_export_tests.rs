//! Original outward native Motion controls. Literal wire contracts are derived from the
//! sanitized synthetic observation; no private schema, file, GUID, artwork or account is copied.

use std::collections::BTreeSet;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use figcraft_doc::anim::motion::{AnimProperty, AnimValue, Ease, TimelineSettings, Track};
use figcraft_doc::color::{Color, Paint};
use figcraft_doc::geom::Vec2;
use figcraft_doc::{Collection, Document, Mode, NodeId, NodeKind, Style, StyleKind, Variable, VariableType, VariableValue};

use super::kiwi::{Kv, Schema, Type, encode};

fn scene() -> (Document, NodeId, NodeId) {
    let mut doc = Document::new("Original native Position export control");
    let page = doc.first_page().unwrap();
    let frame = doc.create(page, NodeKind::Frame, "Original frame", 0.0, 0.0, 240.0, 120.0).unwrap();
    let child = doc.create(frame, NodeKind::Rectangle, "Original moving rectangle", 10.0, 30.0, 40.0, 40.0).unwrap();
    doc.node_mut(child).unwrap().fills = vec![Paint::solid(Color::new(0.2, 0.4, 0.7, 1.0))];
    let mut track = Track::new(AnimProperty::Position);
    track.set_key(0, AnimValue::Vec2(Vec2::new(10.0, 30.0)), Some(Ease::Linear));
    track.set_key(1000, AnimValue::Vec2(Vec2::new(180.0, 30.0)), Some(Ease::Linear));
    doc.node_mut(child).unwrap().motion.tracks.push(track);
    doc.timelines.insert(page, TimelineSettings { duration_ms: 1000, fps: 60, looping: false });
    (doc, frame, child)
}

fn decoded(bytes: &[u8]) -> (Schema, Vec<u8>) {
    let archive = super::Archive::open(bytes).unwrap();
    super::decode_canvas(&archive.canvas).unwrap()
}

fn guid(value: &Kv<'_>) -> (u64, u64) {
    let uint = |name| match value.get(name).unwrap() {
        Kv::Uint(value) => *value,
        _ => panic!("GUID components must be typed unsigned integers"),
    };
    (uint("sessionID"), uint("localID"))
}

fn node_guid(node: &Kv<'_>) -> (u64, u64) {
    guid(node.get("guid").unwrap())
}

fn parent_guid(node: &Kv<'_>) -> (u64, u64) {
    guid(node.get("parentIndex").unwrap().get("guid").unwrap())
}

fn by_kind<'a>(nodes: &'a [Kv<'a>], kind: &str) -> Vec<&'a Kv<'a>> {
    nodes.iter().filter(|node| node.get("type").and_then(Kv::str) == Some(kind)).collect()
}

fn assert_field(schema: &Schema, definition: &str, field: &str, number: u32, primitive: Option<Type>) {
    let definition = &schema.defs[schema.find(definition).unwrap()];
    let actual = definition.fields.iter().find(|actual| actual.name == field).unwrap();
    assert_eq!(actual.value, number, "native field number of {field}");
    if let Some(primitive) = primitive {
        assert_eq!(actual.ty, Some(primitive));
    }
}

fn assert_native_position_schema(schema: &Schema) {
    assert_field(schema, "NodeChange", "parameterConsumptionMap", 445, None);
    assert_field(schema, "NodeChange", "timelinePosition", 506, Some(Type::Int64));
    assert_field(schema, "NodeChange", "keyframeValue", 507, None);
    assert_field(schema, "NodeChange", "timelineDuration", 512, Some(Type::Uint64));
    assert_field(schema, "NodeChange", "easingData", 535, None);
    assert_field(schema, "NodeChange", "motionPathBezierHandles", 598, None);
    assert_field(schema, "VariableAnyValue", "expressionValue", 6, None);
    assert_field(schema, "VariableAnyValue", "keyframeTrackParameterValue", 20, None);
    assert_field(schema, "VariableAnyValue", "vectorValue", 22, None);
    let types = &schema.defs[schema.find("NodeType").unwrap()];
    assert_eq!(types.fields.iter().find(|f| f.name == "KEYFRAME").unwrap().value, 56);
    assert_eq!(types.fields.iter().find(|f| f.name == "KEYFRAME_TRACK").unwrap().value, 57);
}

fn assert_native_position_resources<'a>(nodes: &'a [Kv<'a>], track: &Kv<'_>) -> &'a Kv<'a> {
    let canvases = by_kind(nodes, "CANVAS");
    let internal: Vec<_> = canvases.iter().filter(|node| node.get("internalOnly").and_then(Kv::bool) == Some(true)).collect();
    assert_eq!(internal.len(), 1, "Motion-only files need one hidden internal resource canvas");
    assert_eq!(parent_guid(track), node_guid(internal[0]));
    assert_eq!(node_guid(track).0, 3, "Motion resources use their disjoint session");
    let unique: BTreeSet<_> = nodes.iter().map(node_guid).collect();
    assert_eq!(unique.len(), nodes.len(), "no duplicate root/layer/asset/Motion GUID");
    let frame = nodes.iter().find(|node| node.get("name").and_then(Kv::str) == Some("Original frame")).unwrap();
    assert_eq!(frame.get("timelineDuration"), Some(&Kv::Uint(1_000_000)));
    assert!(canvases.iter().all(|node| node.get("timelineDuration").is_none()), "duration belongs to the native frame");
    let rectangle = nodes.iter().find(|node| node.get("name").and_then(Kv::str) == Some("Original moving rectangle")).unwrap();
    assert_eq!(rectangle.get("transform").unwrap().get("m02"), Some(&Kv::Float(10.0)));
    assert_eq!(rectangle.get("transform").unwrap().get("m12"), Some(&Kv::Float(30.0)));
    rectangle
}

fn assert_native_position_binding(rectangle: &Kv<'_>, track: &Kv<'_>) {
    let entries = rectangle.get("parameterConsumptionMap").unwrap().get("entries").unwrap().list();
    let entry = entries.iter().find(|entry| entry.get("variableField").and_then(Kv::str) == Some("MOTION_TRANSLATION_XY")).unwrap();
    let data = entry.get("variableData").unwrap();
    assert_eq!(data.get("dataType").and_then(Kv::str), Some("EXPRESSION"));
    assert_eq!(data.get("resolvedDataType").and_then(Kv::str), Some("VECTOR"));
    let expression = data.get("value").unwrap().get("expressionValue").unwrap();
    assert_eq!(expression.get("expressionFunction").and_then(Kv::str), Some("KEYFRAME"));
    let args = expression.get("expressionArguments").unwrap().list();
    assert_eq!(args.len(), 2);
    let fallback = args[0].get("value").unwrap().get("vectorValue").unwrap();
    assert_eq!((fallback.get("x"), fallback.get("y")), (Some(&Kv::Float(0.0)), Some(&Kv::Float(0.0))));
    let parameter = &args[1].get("value").unwrap().get("keyframeTrackParameterValue").unwrap().get("parameters").unwrap().list()[0];
    assert_eq!(parameter.get("type").and_then(Kv::str), Some("MANUAL"));
    let reference = parameter.get("value").unwrap().get("manual").unwrap().get("keyframeTrackId").unwrap().get("guid").unwrap();
    assert_eq!(guid(reference), node_guid(track));
}

fn assert_native_position_keys(mut keys: Vec<&Kv<'_>>, track: &Kv<'_>) {
    keys.sort_by_key(|node| match node.get("timelinePosition") {
        None => 0,
        Some(Kv::Int(value)) => *value,
        _ => panic!("native key time must be signed Int64"),
    });
    for (key, time, offset) in [(keys[0], 0, 0.0), (keys[1], 1_000_000, 170.0)] {
        assert_eq!(parent_guid(key), node_guid(track));
        assert_eq!(node_guid(key).0, 3);
        if time != 0 || key.get("timelinePosition").is_some() {
            assert_eq!(key.get("timelinePosition"), Some(&Kv::Int(time)));
        }
        let payload = key.get("keyframeValue").unwrap();
        assert_eq!(payload.get("valueType").and_then(Kv::str), Some("VECTOR"));
        let vector = payload.get("value").unwrap().get("vectorValue").unwrap();
        assert_eq!((vector.get("x"), vector.get("y")), (Some(&Kv::Float(offset)), Some(&Kv::Float(0.0))));
        let easing = key.get("easingData").unwrap();
        assert_eq!(easing.get("easingType").and_then(Kv::str), Some("LINEAR"));
        let curve = easing.get("easingValue").unwrap().get("bezierEasing").unwrap();
        for (name, value) in [("p1x", 0.0), ("p1y", 0.0), ("p2x", 1.0), ("p2y", 1.0)] {
            assert_eq!(curve.get(name), Some(&Kv::Float(value)));
        }
        let spatial = key.get("motionPathBezierHandles").unwrap();
        for name in ["p1x", "p1y", "p2x", "p2y"] {
            assert_eq!(spatial.get(name), Some(&Kv::Float(0.0)));
        }
        assert!(key.get("interpolationType").is_none());
    }
}

#[test]
fn native_position_export_has_literal_editable_native_graph_and_preserves_source() {
    let (doc, _, _) = scene();
    let before = crate::to_json(&doc).unwrap();
    let (bytes, warnings) = super::export(&doc, None).unwrap();
    let (schema, body) = decoded(&bytes);
    let message = schema.decode_message(schema.find("Message").unwrap(), &body).unwrap();
    let nodes = message.get("nodeChanges").unwrap().list();
    let tracks = by_kind(nodes, "KEYFRAME_TRACK");
    assert_eq!(tracks.len(), 1, "actual exported bytes need an editable native track, not a self-reader-only result");
    let keys = by_kind(nodes, "KEYFRAME");
    assert_eq!(keys.len(), 2);
    assert_eq!(nodes.len(), 8, "root + page/frame/rectangle + internal canvas/track/two keys");
    assert_native_position_schema(&schema);
    let rectangle = assert_native_position_resources(nodes, tracks[0]);
    assert_native_position_binding(rectangle, tracks[0]);
    assert_native_position_keys(keys, tracks[0]);
    assert!(
        warnings.iter().any(|w| w.contains("Motion") && w.contains("unverified") && w.contains("fps")),
        "unproved defaults remain explicit: {warnings:?}"
    );
    assert!(!warnings.iter().any(|w| w
        == "Motion keyframes, clips and timeline settings were left out of the .fig export; save a .figcraft copy to preserve editable animation"));
    let (again, _) = super::export(&doc, None).unwrap();
    let (again_schema, again_body) = decoded(&again);
    assert_eq!(again_schema.decode_message(again_schema.find("Message").unwrap(), &again_body).unwrap(), message);
    assert_eq!(crate::to_json(&doc).unwrap(), before);
}

#[test]
fn native_position_export_static_node_budget_has_a_positive_boundary_control() {
    let (mut doc, _, child) = scene();
    doc.node_mut(child).unwrap().motion = Default::default();
    doc.timelines.clear();
    assert!(super::export::export_with_test_node_budget(&doc, 4).is_ok(), "four static node changes fit: document/page/frame/rectangle");
    assert!(super::export::export_with_test_node_budget(&doc, 3).is_err(), "count root as well as the three visible IDs before output allocation");
}

#[test]
fn native_position_export_shares_asset_canvas_and_preserves_static_variable_rows() {
    let (mut doc, _, child) = scene();
    doc.styles.push(Style {
        id: "original-fill".into(),
        name: "Original fill".into(),
        description: String::new(),
        kind: StyleKind::Paint { paints: vec![Paint::solid(Color::BLACK)] },
        bound: Default::default(),
    });
    doc.node_mut(child).unwrap().styles.fill = Some("original-fill".into());
    doc.variables.collections.push(Collection {
        id: "original-set".into(),
        name: "Original values".into(),
        modes: vec![Mode { id: "original-mode".into(), name: "Default".into() }],
        extends: None,
    });
    doc.variables.variables.push(Variable {
        id: "original-radius".into(),
        name: "Original radius".into(),
        collection: "original-set".into(),
        kind: VariableType::Number,
        values: [("original-mode".into(), VariableValue::Number(12.0))].into_iter().collect(),
        description: String::new(),
        scopes: None,
        hidden_from_publishing: false,
        extends: None,
        overrides: Default::default(),
        design_token_type: None,
        code_syntax: Default::default(),
    });
    doc.node_mut(child).unwrap().bound.insert("cornerRadius".into(), "original-radius".into());
    let before = crate::to_json(&doc).unwrap();
    let (bytes, _) = super::export(&doc, None).unwrap();
    let (schema, body) = decoded(&bytes);
    let root = schema.decode_message(schema.find("Message").unwrap(), &body).unwrap();
    let nodes = root.get("nodeChanges").unwrap().list();
    let rectangle = nodes.iter().find(|node| node.get("name").and_then(Kv::str) == Some("Original moving rectangle")).unwrap();
    let variables = by_kind(nodes, "VARIABLE");
    assert_eq!(variables.len(), 1);
    let static_rows = rectangle.get("variableConsumptionMap").unwrap().get("entries").unwrap().list();
    let radius = static_rows.iter().find(|row| row.get("variableField").and_then(Kv::str) == Some("CORNER_RADIUS")).unwrap();
    let alias = radius.get("variableData").unwrap().get("value").unwrap().get("alias").unwrap().get("guid").unwrap();
    assert_eq!(guid(alias), node_guid(variables[0]), "field307 keeps its original static-variable binding");
    assert!(
        rectangle
            .get("parameterConsumptionMap")
            .unwrap()
            .get("entries")
            .unwrap()
            .list()
            .iter()
            .any(|row| row.get("variableField").and_then(Kv::str) == Some("MOTION_TRANSLATION_XY")),
        "new field445 coexists with static field307"
    );
    let internal: Vec<_> = nodes.iter().filter(|node| node.get("internalOnly").and_then(Kv::bool) == Some(true)).collect();
    assert_eq!(internal.len(), 1);
    assert_eq!(by_kind(nodes, "KEYFRAME_TRACK").len(), 1);
    assert_eq!(nodes.iter().map(node_guid).collect::<BTreeSet<_>>().len(), nodes.len());
    assert_eq!(crate::to_json(&doc).unwrap(), before);
}

#[test]
fn native_position_export_encoder_budget_precedes_invalid_tail_and_checks_exact_boundary() {
    let schema = Schema::parse("message Probe string payload=1, uint tail=2").unwrap();
    let root = schema.find("Probe").unwrap();
    let valid = Kv::Object(vec![("payload", Kv::Str("alpha".into())), ("tail", Kv::Uint(3))]);
    let expected = encode::message(&schema, root, &valid).unwrap();
    assert_eq!(encode::message_with_test_cap(&schema, root, &valid, expected.len()).unwrap(), expected);
    assert!(
        encode::message_with_test_cap(&schema, root, &valid, expected.len() - 1).is_err(),
        "budget+1 must refuse, despite otherwise valid schema values"
    );
    let invalid_tail = Kv::Object(vec![("payload", Kv::Str("x".repeat(4096))), ("tail", Kv::Array(Vec::new()))]);
    assert!(encode::message(&schema, root, &invalid_tail).is_err(), "the uncapped control really has an invalid uint tail");
    let error = encode::message_with_test_cap(&schema, root, &invalid_tail, 32).unwrap_err().to_string();
    assert!(error.contains("budget") || error.contains("limit"), "refuse the oversized prefix before allocating/walking the invalid tail: {error}");
}

#[test]
fn native_position_export_cycle_child() {
    if std::env::var_os("FIGCRAFT_NATIVE_EXPORT_CYCLE_CHILD").is_none() {
        return;
    }
    let (mut doc, frame, _) = scene();
    doc.node_mut(frame).unwrap().children.push(frame);
    let refused = match super::export(&doc, None) {
        Err(error) => error.to_string().contains("cycl") || error.to_string().contains("duplicate"),
        Ok((_, warnings)) => warnings.iter().any(|warning| warning.contains("cycl") || warning.contains("duplicate")),
    };
    assert!(refused, "cyclic assignment must terminate with an explicit source-tree reason");
    std::process::exit(42); // A zero-test/wrong-filter child must never make the timeout oracle pass.
}

#[test]
fn native_position_export_cyclic_assignment_is_bounded_in_a_child_process() {
    struct Cleanup(Child);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Cleanup(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fig::motion_export_tests::native_position_export_cycle_child", "--nocapture"])
            .env("FIGCRAFT_NATIVE_EXPORT_CYCLE_CHILD", "1")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert_eq!(status.code(), Some(42), "child must actually execute and refuse the cyclic graph");
            break;
        }
        if Instant::now() >= deadline {
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            panic!("actual native export did not terminate cyclic assignment within three seconds; child was killed and joined");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn native_position_export_outline_budget_precedes_the_actual_generator() {
    let mut node = figcraft_doc::Node::new(NodeId(90), NodeKind::Line, "Original lopsided line", 100.0, 0.0);
    node.fills.clear();
    node.strokes = vec![Paint::solid(Color::BLACK)];
    node.stroke.weight = 4.0;
    let mut profile = figcraft_doc::WidthProfile::new([(0.0, 1.0), (1.0, 1.0)]);
    for point in &mut profile.points {
        point.bias = 0.5;
    }
    node.stroke.width_profile = Some(profile);
    let positive = super::export::outline_with_test_budget(&node, 32 * 1024 * 1024);
    assert_eq!(positive, (true, 1, false), "the source really enters the area generator with a sufficient budget");
    let refused = super::export::outline_with_test_budget(&node, 64 * 1024);
    assert_eq!(refused, (false, 0, true), "small budget refuses before area generation, not after observing an oversized area");
}

#[test]
fn native_position_export_duplicate_asset_and_mode_identifiers_are_refused() {
    let (mut doc, _, child) = scene();
    doc.node_mut(child).unwrap().motion = Default::default();
    doc.timelines.clear();
    let style = Style {
        id: "original-style".into(),
        name: "Original style".into(),
        description: String::new(),
        kind: StyleKind::Paint { paints: vec![Paint::solid(Color::BLACK)] },
        bound: Default::default(),
    };
    doc.styles.push(style.clone());
    assert!(super::export(&doc, None).is_ok(), "known-positive unique assets remain supported");
    doc.styles.push(style);
    assert!(
        super::export(&doc, None).unwrap_err().to_string().contains("duplicate"),
        "deduplicated maps must not hide duplicated emitted asset rows"
    );
    doc.styles.pop();
    doc.variables.collections.push(Collection {
        id: "original-set".into(),
        name: "Original set".into(),
        modes: vec![Mode { id: "m".into(), name: "Default".into() }],
        extends: None,
    });
    assert!(super::export(&doc, None).is_ok());
    let mode = doc.variables.collections[0].modes[0].clone();
    doc.variables.collections[0].modes.push(mode);
    assert!(super::export(&doc, None).unwrap_err().to_string().contains("duplicate"));
    doc.variables.collections[0].modes.pop();
    let set = doc.variables.collections[0].clone();
    doc.variables.collections.push(set);
    assert!(super::export(&doc, None).unwrap_err().to_string().contains("duplicate"));
}

#[test]
fn native_position_export_encoder_counts_unicode_nul_nested_empty_bytes_and_varints_exactly() {
    let schema = Schema::parse(
        "enum Choice ZERO=0 MANY=300\nstruct Pair uint x, string label\nmessage Probe byte[] data=1, Choice kind=2, Pair pair=3, Probe[] children=4",
    )
    .unwrap();
    let object = |fields| Kv::Object(fields);
    let pair = |x, text: &str| object(vec![("x", Kv::Uint(x)), ("label", Kv::Str(text.into()))]);
    let child = object(vec![("data", Kv::Bytes(vec![0, 255])), ("kind", Kv::Enum("ZERO")), ("pair", pair(0, "")), ("children", Kv::Array(vec![]))]);
    let value =
        object(vec![("data", Kv::Bytes(vec![])), ("kind", Kv::Enum("MANY")), ("pair", pair(128, "\0λ🙂\0")), ("children", Kv::Array(vec![child]))]);
    let expected = vec![1, 0, 2, 172, 2, 3, 128, 1, 206, 187, 240, 159, 153, 130, 0, 4, 1, 1, 2, 0, 255, 2, 0, 3, 0, 0, 4, 0, 0, 0];
    let root = schema.find("Probe").unwrap();
    assert_eq!(encode::message_with_test_cap(&schema, root, &value, expected.len()).unwrap(), expected);
    assert!(encode::message_with_test_cap(&schema, root, &value, expected.len() - 1).unwrap_err().to_string().contains("budget"));
}

#[test]
fn native_position_export_canvas_refuses_compressible_source_before_compression_and_caps_growth() {
    let payload = vec![0u8; 4096];
    assert!(super::export::canvas_within(b"schema", &payload, 8192).is_ok());
    assert!(super::export::canvas_within(b"schema", &payload, 4096).is_err(), "high compression must not bypass the uncompressed preflight");
    assert!(super::export::canvas_within(b"s", b"m", 16).is_err(), "the compressed headers/chunks also need a checked output cap");
}

#[test]
fn native_position_export_total_node_budget_includes_motion_resources_and_the_shared_canvas() {
    let (doc, _, _) = scene();
    assert!(super::export::export_with_test_node_budget(&doc, 8).is_ok());
    assert!(super::export::export_with_test_node_budget(&doc, 7).is_err());
}

#[test]
fn native_position_export_unsupported_tracks_have_precise_counted_reasons_without_phantom_resources() {
    type Change = fn(&mut Document, NodeId, NodeId);
    let changes: [(Change, &str); 10] = [
        (|d, _, c| d.node_mut(c).unwrap().visible = false, "hidden, locked"),
        (|d, _, c| d.node_mut(c).unwrap().locked = true, "hidden, locked"),
        (|d, _, c| d.node_mut(c).unwrap().motion.tracks[0].visible = false, "hidden track"),
        (|d, _, c| d.node_mut(c).unwrap().motion.tracks[0].keys[0].easing = Ease::Hold, "easing"),
        (|d, _, c| d.node_mut(c).unwrap().motion.tracks[0].keys[0].value = AnimValue::Scalar(0.0), "not a Vec2"),
        (|d, _, c| d.node_mut(c).unwrap().motion.tracks[0].keys.swap(0, 1), "span"),
        (|d, _, c| d.node_mut(c).unwrap().motion.tracks[0].keys[1].value = AnimValue::Vec2(Vec2::new(1_000_000.02, 30.0)), "precision"),
        (|d, _, _| d.timelines.values_mut().next().unwrap().duration_ms = 2000, "span"),
        (|d, _, _| d.timelines.values_mut().next().unwrap().looping = true, "fps/playback"),
        (|d, _, _| d.timelines.clear(), "missing declared"),
    ];
    for (change, reason) in changes {
        let (mut doc, frame, child) = scene();
        change(&mut doc, frame, child);
        let before = crate::to_json(&doc).unwrap();
        let (bytes, warnings) = super::export(&doc, None).unwrap();
        let (schema, body) = decoded(&bytes);
        let message = schema.decode_message(schema.find("Message").unwrap(), &body).unwrap();
        let nodes = message.get("nodeChanges").unwrap().list();
        assert!(by_kind(nodes, "KEYFRAME_TRACK").is_empty(), "{reason}");
        assert!(!nodes.iter().any(|n| n.get("internalOnly").and_then(Kv::bool) == Some(true)), "no phantom canvas for omitted graph");
        assert!(warnings.iter().any(|w| w.contains("omitted 1 tracks") && w.contains(reason)), "{warnings:?}");
        assert_eq!(crate::to_json(&doc).unwrap(), before);
    }
}

#[test]
fn native_position_export_multiple_animated_frame_owners_are_not_chosen_arbitrarily() {
    let (mut doc, _, child) = scene();
    let motion = doc.node(child).unwrap().motion.clone();
    let page = doc.first_page().unwrap();
    let frame = doc.create(page, NodeKind::Frame, "Second frame", 0.0, 0.0, 240.0, 120.0).unwrap();
    let second = doc.create(frame, NodeKind::Rectangle, "Second moving rectangle", 10.0, 30.0, 40.0, 40.0).unwrap();
    doc.node_mut(second).unwrap().motion = motion;
    let (bytes, warnings) = super::export(&doc, None).unwrap();
    let (schema, body) = decoded(&bytes);
    let message = schema.decode_message(schema.find("Message").unwrap(), &body).unwrap();
    assert!(by_kind(message.get("nodeChanges").unwrap().list(), "KEYFRAME_TRACK").is_empty());
    assert!(warnings.iter().any(|w| w.contains("omitted 2 tracks") && w.contains("multiple animated frame owners")), "{warnings:?}");
}

#[test]
fn native_position_export_text_runs_paints_stops_vector_and_blob_generators_share_preallocation_caps() {
    use figcraft_doc::color::{ColorStop, Gradient, GradientKind, PaintKind};
    use figcraft_doc::{Region, Segment, StyleRun, TextData, TextStyle, VectorNetwork, Vertex};
    let mut doc = Document::new("Original generation-budget controls");
    let page = doc.first_page().unwrap();
    let mut text = TextData::new("A".repeat(4096));
    text.runs = (0..4)
        .map(|i| StyleRun { start: i * 1024, end: (i + 1) * 1024, style: TextStyle { font_size: 18.0 + i as f64, ..Default::default() } })
        .collect();
    let text_id = doc.create(page, NodeKind::Text(text), "Styled original text", 0.0, 0.0, 100.0, 100.0).unwrap();
    let probe = |id, kind, bytes, work| super::export::generation_with_test_budget(&doc, id, kind, bytes, work);
    let (failed, data, _) = probe(text_id, "text", 16 * 1024 * 1024, 4_000_000);
    assert!(!failed);
    let data = data.get("textData").unwrap();
    assert_eq!(data.get("characterStyleIDs").unwrap().list().len(), 4096);
    assert_eq!(data.get("styleOverrideTable").unwrap().list().len(), 4);
    let (failed, data, _) = probe(text_id, "text", 256 * 1024, 4_000_000);
    assert!(failed);
    assert!(data.get("textData").is_none(), "refuse before character/style arrays are built");
    let (failed, _, _) = probe(text_id, "text", 16 * 1024 * 1024, 16);
    assert!(failed, "the same source must be charged for comparisons, not only byte output");

    let paint_id = doc.create(page, NodeKind::Rectangle, "Gradient", 0.0, 0.0, 10.0, 10.0).unwrap();
    let mut gradient = Gradient::default_for(GradientKind::Linear, Color::BLACK);
    gradient.stops = (0..128).map(|i| ColorStop { position: i as f32 / 127.0, color: Color::new(i as f32 / 127.0, 0.2, 0.4, 1.0) }).collect();
    doc.node_mut(paint_id).unwrap().fills = vec![Paint { kind: PaintKind::Gradient(gradient), ..Paint::solid(Color::BLACK) }];
    let (failed, data, _) = super::export::generation_with_test_budget(&doc, paint_id, "paint", 16 * 1024 * 1024, 4_000_000);
    assert!(!failed);
    assert_eq!(data.get("fillPaints").unwrap().list()[0].get("stops").unwrap().list().len(), 128);
    let (failed, data, _) = super::export::generation_with_test_budget(&doc, paint_id, "paint", 96 * 1024, 4_000_000);
    assert!(failed);
    assert!(data.get("fillPaints").unwrap().list().is_empty());

    let network = VectorNetwork {
        vertices: (0..64).map(|i| Vertex::new(i as f64, (i % 2) as f64)).collect(),
        segments: (0..64).map(|i| Segment::line(i, (i + 1) % 64)).collect(),
        regions: vec![Region { loops: vec![(0..64).collect()], ..Default::default() }],
    };
    let vector = doc.create(page, NodeKind::Vector { network }, "Editable original vector", 0.0, 0.0, 64.0, 10.0).unwrap();
    let (failed, data, blobs) = super::export::generation_with_test_budget(&doc, vector, "vector", 16 * 1024 * 1024, 4_000_000);
    assert!(!failed);
    assert!(data.get("vectorData").is_some());
    assert!(blobs >= 1);
    let (failed, data, blobs) = super::export::generation_with_test_budget(&doc, vector, "vector", 64 * 1024, 4_000_000);
    assert!(failed);
    assert!(data.get("vectorData").is_none());
    assert_eq!(blobs, 0);
    let (failed, data, blobs) = super::export::generation_with_test_budget(&doc, vector, "blob", 64 * 1024, 4_000_000);
    assert!(!failed);
    assert!(data.get("fillGeometry").is_some());
    assert_eq!(blobs, 1);
    let (failed, data, blobs) = super::export::generation_with_test_budget(&doc, vector, "blob", 1024, 4_000_000);
    assert!(failed);
    assert!(data.get("fillGeometry").is_none());
    assert_eq!(blobs, 0, "no blob allocated/admitted after failed reservation");
}

#[test]
fn native_position_export_instance_inline_generation_charges_shared_budget_across_instances() {
    use figcraft_doc::TextData;
    let mut doc = Document::new("Original repeated instance budget control");
    let page = doc.first_page().unwrap();
    let main = doc
        .create(
            page,
            NodeKind::Component { description: String::new(), props: Default::default(), prop_order: vec![] },
            "Original component",
            0.0,
            0.0,
            100.0,
            100.0,
        )
        .unwrap();
    let sources: Vec<_> = (0..8)
        .map(|i| doc.create(main, NodeKind::Text(TextData::new("main".repeat(64))), &format!("Source {i}"), 0.0, 0.0, 40.0, 20.0).unwrap())
        .collect();
    let mut instances = Vec::new();
    for i in 0..8 {
        let inst = doc
            .create(page, NodeKind::Instance { component: main, props: Default::default() }, &format!("Instance {i}"), 0.0, 0.0, 100.0, 100.0)
            .unwrap();
        for source in &sources {
            let child = doc.create(inst, NodeKind::Text(TextData::new("copy".repeat(64))), "Original override", 0.0, 0.0, 40.0, 20.0).unwrap();
            doc.node_mut(child).unwrap().source = Some(*source);
        }
        instances.push(inst);
    }
    // This cap fits the temporary comparison buffers for one instance. Once the prior
    // emitted rows remain in memory, the same cap must reject the complete subtree.
    let shared_bytes = 1024 * 1024;
    let (failed, data, _) = super::export::generation_with_test_budget(&doc, instances[0], "instance", shared_bytes, 4_000_000);
    assert!(!failed, "a single known-positive instance fits");
    assert!(data.get("symbolData").is_some());
    let (failed, _, _) = super::export::generation_with_test_budget(&doc, page, "subtree", shared_bytes, 4_000_000);
    assert!(failed, "per-instance resets must not permit repeated inline amplification");
    let (failed, data, _) = super::export::generation_with_test_budget(&doc, page, "subtree", 64 * 1024 * 1024, 4_000_000);
    assert!(!failed);
    assert!(data.get("nodes").unwrap().list().iter().filter(|n| n.get("type").and_then(Kv::str) == Some("INSTANCE")).count() == 8);
}

#[test]
fn native_position_export_motion_and_static_component_parameter_maps_coexist_with_unique_aux_ids() {
    use figcraft_doc::PropDef;
    let (mut doc, _, _) = scene();
    let page = doc.first_page().unwrap();
    let main = doc
        .create(
            page,
            NodeKind::Component {
                description: String::new(),
                props: std::collections::BTreeMap::from([("Shown".into(), PropDef::Boolean { default: true })]),
                prop_order: vec!["Shown".into()],
            },
            "Original native K component",
            0.0,
            150.0,
            100.0,
            100.0,
        )
        .unwrap();
    let child = doc.create(main, NodeKind::Rectangle, "Bound original rectangle", 0.0, 0.0, 20.0, 20.0).unwrap();
    doc.node_mut(child).unwrap().prop_refs.insert("visible".into(), "Shown".into());
    let (bytes, _) = super::export(&doc, None).unwrap();
    let (schema, body) = decoded(&bytes);
    let msg = schema.decode_message(schema.find("Message").unwrap(), &body).unwrap();
    let nodes = msg.get("nodeChanges").unwrap().list();
    assert_eq!(by_kind(nodes, "KEYFRAME_TRACK").len(), 1);
    let bound = nodes.iter().find(|n| n.get("name").and_then(Kv::str) == Some("Bound original rectangle")).unwrap();
    let entries = bound.get("parameterConsumptionMap").unwrap().get("entries").unwrap().list();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].get("variableField").and_then(Kv::str), Some("VISIBLE"));
    assert_eq!(entries[0].get("variableData").unwrap().get("dataType").and_then(Kv::str), Some("PROP_REF"));
    assert_eq!(bound.get("componentPropRefs").unwrap().list().len(), 1);
    let component = by_kind(nodes, "SYMBOL")[0];
    let definition = &component.get("componentPropDefs").unwrap().list()[0];
    assert_eq!(guid(definition.get("id").unwrap()).0, 2);
    assert_eq!(nodes.iter().map(node_guid).collect::<BTreeSet<_>>().len(), nodes.len());
}

#[test]
fn native_position_export_image_hash_work_is_precharged_and_same_source_id_is_cached() {
    use figcraft_doc::ImageData;
    use figcraft_doc::color::ImageScaleMode;
    let mut doc = Document::new("Original image work-budget control");
    let page = doc.first_page().unwrap();
    let node = doc.create(page, NodeKind::Rectangle, "Image references", 0.0, 0.0, 10.0, 10.0).unwrap();
    let image = ImageData { bytes: std::sync::Arc::new(vec![42; 512 * 1024]), width: 1, height: 1 };
    doc.images.insert("original-a".into(), image.clone());
    doc.images.insert("original-b".into(), image);
    doc.node_mut(node).unwrap().fills = (0..16).map(|_| Paint::image("original-a", ImageScaleMode::Fill)).collect();
    let (failed, data, cached) = super::export::generation_with_test_budget(&doc, node, "paint", 16 * 1024 * 1024, 10_000);
    assert!(!failed, "repeated references to one ID must hash its payload only once");
    assert_eq!(cached, 1);
    assert_eq!(data.get("fillPaints").unwrap().list().len(), 16);
    doc.node_mut(node).unwrap().fills = vec![Paint::image("original-a", ImageScaleMode::Fill), Paint::image("original-b", ImageScaleMode::Fill)];
    let (failed, _, cached) = super::export::generation_with_test_budget(&doc, node, "paint", 16 * 1024 * 1024, 10_000);
    assert!(failed, "distinct IDs with identical bytes still charge every required hash before deduplication");
    assert_eq!(cached, 1, "the second source was refused before hashing/cache admission");
}

fn assert_dimension_refused(owner: bool, width: bool, value: f64) {
    let (mut doc, frame, child) = scene();
    let node = doc.node_mut(if owner { frame } else { child }).unwrap();
    if width {
        node.width = value;
    } else {
        node.height = value;
    }
    let before = crate::to_json(&doc).unwrap();
    let (bytes, warnings) = super::export(&doc, None).unwrap();
    let (schema, body) = decoded(&bytes);
    let message = schema.decode_message(schema.find("Message").unwrap(), &body).unwrap();
    assert!(
        by_kind(message.get("nodeChanges").unwrap().list(), "KEYFRAME_TRACK").is_empty(),
        "owner={owner},width={width},value={value}: lossy or nonpositive Float32 dimensions must omit Motion"
    );
    assert!(warnings.iter().any(|w| w.contains("omitted 1 tracks") && w.contains("dimension")), "{warnings:?}");
    assert_eq!(crate::to_json(&doc).unwrap(), before);
}

macro_rules! dimension_refusal {
    ($name:ident, $owner:expr, $width:expr, $value:expr) => {
        #[test]
        fn $name() {
            assert_dimension_refused($owner, $width, $value);
        }
    };
}
dimension_refusal!(native_position_export_dimension_target_width_precision, false, true, 1_000_000.02);
dimension_refusal!(native_position_export_dimension_target_width_range, false, true, 1_000_000_001.0);
dimension_refusal!(native_position_export_dimension_target_width_zero, false, true, 0.0);
dimension_refusal!(native_position_export_dimension_target_width_negative, false, true, -1.0);
dimension_refusal!(native_position_export_dimension_target_width_underflow, false, true, 1e-300);
dimension_refusal!(native_position_export_dimension_target_height_precision, false, false, 1_000_000.02);
dimension_refusal!(native_position_export_dimension_target_height_range, false, false, 1_000_000_001.0);
dimension_refusal!(native_position_export_dimension_target_height_zero, false, false, 0.0);
dimension_refusal!(native_position_export_dimension_target_height_negative, false, false, -1.0);
dimension_refusal!(native_position_export_dimension_target_height_underflow, false, false, 1e-300);
dimension_refusal!(native_position_export_dimension_frame_width_precision, true, true, 1_000_000.02);
dimension_refusal!(native_position_export_dimension_frame_width_range, true, true, 1_000_000_001.0);
dimension_refusal!(native_position_export_dimension_frame_width_zero, true, true, 0.0);
dimension_refusal!(native_position_export_dimension_frame_width_negative, true, true, -1.0);
dimension_refusal!(native_position_export_dimension_frame_width_underflow, true, true, 1e-300);
dimension_refusal!(native_position_export_dimension_frame_height_precision, true, false, 1_000_000.02);
dimension_refusal!(native_position_export_dimension_frame_height_range, true, false, 1_000_000_001.0);
dimension_refusal!(native_position_export_dimension_frame_height_zero, true, false, 0.0);
dimension_refusal!(native_position_export_dimension_frame_height_negative, true, false, -1.0);
dimension_refusal!(native_position_export_dimension_frame_height_underflow, true, false, 1e-300);
dimension_refusal!(native_position_export_dimension_target_width_subnormal, false, true, 1e-44);
dimension_refusal!(native_position_export_dimension_target_height_subnormal, false, false, 1e-44);
dimension_refusal!(native_position_export_dimension_frame_width_subnormal, true, true, 1e-44);
dimension_refusal!(native_position_export_dimension_frame_height_subnormal, true, false, 1e-44);

#[test]
fn native_position_export_dimension_minimum_normal_wire_positive() {
    let value = f64::from(f32::MIN_POSITIVE);
    for owner in [false, true] {
        for width in [false, true] {
            let (mut doc, frame, child) = scene();
            let node = doc.node_mut(if owner { frame } else { child }).unwrap();
            if width {
                node.width = value;
            } else {
                node.height = value;
            }
            let name = node.name.clone();
            let (bytes, warnings) = super::export(&doc, None).unwrap();
            let (schema, body) = decoded(&bytes);
            let message = schema.decode_message(schema.find("Message").unwrap(), &body).unwrap();
            let nodes = message.get("nodeChanges").unwrap().list();
            assert_eq!(by_kind(nodes, "KEYFRAME_TRACK").len(), 1, "{warnings:?}");
            let emitted = nodes.iter().find(|node| node.get("name").and_then(Kv::str) == Some(name.as_str())).unwrap();
            let dimension = emitted.get("size").unwrap().get(if width { "x" } else { "y" }).and_then(Kv::f64).unwrap();
            assert!(dimension > 0.0, "the smallest normal dimension must stay positive on the actual wire");
            assert_eq!(dimension, value);
        }
    }
}

#[test]
fn native_position_export_keeps_positive_dimensions_within_the_declared_float32_tolerance() {
    for owner in [false, true] {
        for width in [false, true] {
            for value in [40.0001, 1_000_000_000.0] {
                let (mut doc, frame, child) = scene();
                let node = doc.node_mut(if owner { frame } else { child }).unwrap();
                if width {
                    node.width = value;
                } else {
                    node.height = value;
                }
                let (bytes, warnings) = super::export(&doc, None).unwrap();
                let (schema, body) = decoded(&bytes);
                let message = schema.decode_message(schema.find("Message").unwrap(), &body).unwrap();
                assert_eq!(by_kind(message.get("nodeChanges").unwrap().list(), "KEYFRAME_TRACK").len(), 1, "{warnings:?}");
            }
        }
    }
}

// Original current-composition controls: layer fallback makes sharp vertices styled too.
fn fallback_budget_vector(count: usize, radius: f64) -> (Document, NodeId) {
    use figcraft_doc::{Segment, VectorNetwork, Vertex};
    let mut doc = Document::new("Original sharp-point fallback budget fixture");
    let page = doc.first_page().unwrap();
    let positive = (count / 8).clamp(1, 16);
    let vertices = (0..count)
        .map(|i| {
            let mut v = Vertex::new(i as f64, (i % 2) as f64);
            if i < positive {
                v.radius = (i + 1) as f64;
            }
            v
        })
        .collect();
    let network = VectorNetwork { vertices, segments: (1..count).map(|i| Segment::line(i - 1, i)).collect(), regions: vec![] };
    let id = doc.create(page, NodeKind::Vector { network }, "Mixed radius vector", 0.0, 0.0, count as f64, 2.0).unwrap();
    doc.node_mut(id).unwrap().corners.radii = [radius; 4];
    (doc, id)
}

#[test]
fn issue_2114_export_sharp_fallback_styles_are_precharged_before_style_search() {
    let (doc, id) = fallback_budget_vector(256, 16.0);
    let (failed, data, blobs) = super::export::generation_with_test_budget(&doc, id, "vector", 16 * 1024 * 1024, 60_000);
    assert!(failed, "all nonempty sharp-fallback settings participate in the comparison-work reservation");
    assert!(data.get("vectorData").is_none(), "the reservation must precede network/style generation");
    assert_eq!(blobs, 0);

    let (plain, id) = fallback_budget_vector(256, 0.0);
    let (failed, data, blobs) = super::export::generation_with_test_budget(&plain, id, "vector", 16 * 1024 * 1024, 60_000);
    assert!(!failed, "plain sharp points still need no fallback settings or style-table search");
    assert!(data.get("vectorData").is_some());
    assert_eq!(blobs, 1);
}

#[test]
fn issue_2114_export_small_fallback_vector_keeps_explicit_zero_settings() {
    let (doc, id) = fallback_budget_vector(8, 16.0);
    let (failed, data, blobs) = super::export::generation_with_test_budget(&doc, id, "vector", 16 * 1024 * 1024, 60_000);
    assert!(!failed);
    assert_eq!(blobs, 1);
    let styles = data.get("vectorData").unwrap().get("styleOverrideTable").unwrap().list();
    assert!(styles.iter().any(|s| matches!(s.get("cornerRadius"), Some(Kv::Float(v)) if *v == 0.0)));
}
