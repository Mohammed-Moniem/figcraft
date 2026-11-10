//! Original minimal native-shaped fixtures, authored from the sanitized observed contract.
//! No private Figma file, complete vendor schema, account metadata or original GUID is copied.

use figcraft_doc::anim::motion::{AnimProperty, AnimValue, Ease, MotionPlayer, base_position};
use figcraft_doc::geom::Vec2;
use figcraft_doc::{Document, NodeId};
use serde_json::json;

use super::kiwi::{Kv, Schema, encode};

const SCHEMA: &str = "
enum NodeType DOCUMENT=1 CANVAS=2 FRAME=4 ROUNDED_RECTANGLE=12 KEYFRAME=56 KEYFRAME_TRACK=57 ANIMATION_PRESET_INSTANCE=58
enum NodePhase CREATED=0 REMOVED=2
enum DataType EXPRESSION=5 KEYFRAME_TRACK_PARAMETER_DATA=21 VECTOR=24
enum ResolvedType KEYFRAME_TRACK_PARAMETER_DATA=14 VECTOR=17
enum VariableField OPACITY=31 MOTION_TRANSLATION_XY=69 MOTION_SCALE_XY=70
enum ParameterType MANUAL=1 ANIMATION_PRESET=2
enum ValueType FLOAT=0 VECTOR=4
enum EasingType LINEAR=3 CUSTOM_CUBIC=7
enum InterpolationType HOLD=0 BEZIER=1
struct GUID uint sessionID, uint localID
struct Vector float x, float y
struct Matrix float m00, float m01, float m02, float m10, float m11, float m12
message ParentIndex GUID guid=1, string position=2
message TrackId GUID guid=1
message Manual TrackId keyframeTrackId=1
message AnyParameter Manual manual=1
message Parameter AnyParameter value=1, ParameterType type=2
message Parameters Parameter[] parameters=1
message AnyVariable Expression expressionValue=6, Parameters keyframeTrackParameterValue=20, Vector vectorValue=22
message VariableData AnyVariable value=1, DataType dataType=2, ResolvedType resolvedDataType=3
enum Function KEYFRAME=19
message Expression Function expressionFunction=1, VariableData[] expressionArguments=2
message MapEntry VariableData variableData=2, VariableField variableField=3
message VariableDataMap MapEntry[] entries=1
message AnyValue float floatValue=1, Vector vectorValue=4
message ValueData AnyValue value=1, ValueType valueType=2
message Handles float p1x=1, float p1y=2, float p2x=3, float p2y=4
message AnyEasing Handles bezierEasing=2
message Easing EasingType easingType=1, AnyEasing easingValue=2
message Paint VariableData colorVar=21
message Effect VariableData radiusVar=9
message NodeChange GUID guid=1, NodePhase phase=2, ParentIndex parentIndex=3, NodeType type=4, string name=5, bool visible=6, bool locked=7, float opacity=8, Vector size=11, Matrix transform=12, Paint[] fillPaints=38, Effect[] effects=43, bool internalOnly=142, VariableDataMap parameterConsumptionMap=445, InterpolationType interpolationType=505, int64 timelinePosition=506, ValueData keyframeValue=507, uint64 timelineDuration=512, Easing easingData=535, Handles motionPathBezierHandles=598
message Message NodeChange[] nodeChanges=4
";

fn o(fields: Vec<(&'static str, Kv<'static>)>) -> Kv<'static> {
    Kv::Object(fields)
}
fn e(value: &'static str) -> Kv<'static> {
    Kv::Enum(value)
}
fn f(value: f32) -> Kv<'static> {
    Kv::Float(value)
}
fn g(id: u64) -> Kv<'static> {
    o(vec![("sessionID", Kv::Uint(7)), ("localID", Kv::Uint(id))])
}
fn xy(x: f32, y: f32) -> Kv<'static> {
    o(vec![("x", f(x)), ("y", f(y))])
}
fn at(x: f32, y: f32) -> Kv<'static> {
    o(vec![("m00", f(1.0)), ("m01", f(0.0)), ("m02", f(x)), ("m10", f(0.0)), ("m11", f(1.0)), ("m12", f(y))])
}
fn handles(values: [f32; 4]) -> Kv<'static> {
    o(["p1x", "p1y", "p2x", "p2y"].into_iter().zip(values).map(|(key, value)| (key, f(value))).collect())
}
fn set(node: &mut Kv<'static>, name: &'static str, value: Kv<'static>) {
    let Kv::Object(fields) = node else { panic!("test node must be an object") };
    if let Some((_, old)) = fields.iter_mut().find(|(key, _)| *key == name) {
        *old = value;
    } else {
        fields.push((name, value));
    }
}
fn remove(node: &mut Kv<'static>, name: &str) {
    let Kv::Object(fields) = node else { panic!("test node must be an object") };
    fields.retain(|(key, _)| *key != name);
}
fn node(id: u64, parent: Option<u64>, kind: &'static str, name: &str, mut extra: Vec<(&'static str, Kv<'static>)>) -> Kv<'static> {
    let mut fields = vec![
        ("guid", g(id)),
        ("phase", e("CREATED")),
        ("type", e(kind)),
        ("name", Kv::Str(name.into())),
        ("visible", Kv::Bool(true)),
        ("opacity", f(1.0)),
    ];
    if let Some(parent) = parent {
        fields.push(("parentIndex", o(vec![("guid", g(parent)), ("position", Kv::Str(format!("{id:04}")))])));
    }
    fields.append(&mut extra);
    o(fields)
}
fn binding(property: &'static str, track: u64, parameter_type: &'static str) -> Kv<'static> {
    binding_with_fallback(property, track, parameter_type, [0.0, 0.0])
}
fn binding_with_fallback(property: &'static str, track: u64, parameter_type: &'static str, fallback: [f32; 2]) -> Kv<'static> {
    let vector =
        o(vec![("dataType", e("VECTOR")), ("resolvedDataType", e("VECTOR")), ("value", o(vec![("vectorValue", xy(fallback[0], fallback[1]))]))]);
    let parameter = o(vec![("type", e(parameter_type)), ("value", o(vec![("manual", o(vec![("keyframeTrackId", o(vec![("guid", g(track))]))]))]))]);
    let reference = o(vec![
        ("dataType", e("KEYFRAME_TRACK_PARAMETER_DATA")),
        ("resolvedDataType", e("KEYFRAME_TRACK_PARAMETER_DATA")),
        ("value", o(vec![("keyframeTrackParameterValue", o(vec![("parameters", Kv::Array(vec![parameter]))]))])),
    ]);
    let expression = o(vec![("expressionFunction", e("KEYFRAME")), ("expressionArguments", Kv::Array(vec![vector, reference]))]);
    let data = o(vec![("dataType", e("EXPRESSION")), ("resolvedDataType", e("VECTOR")), ("value", o(vec![("expressionValue", expression)]))]);
    o(vec![("entries", Kv::Array(vec![o(vec![("variableField", e(property)), ("variableData", data)])]))])
}
fn key(id: u64, time_us: Option<i64>, offset_x: f32) -> Kv<'static> {
    let mut fields = vec![
        ("keyframeValue", o(vec![("valueType", e("VECTOR")), ("value", o(vec![("vectorValue", xy(offset_x, 0.0))]))])),
        ("easingData", o(vec![("easingType", e("LINEAR")), ("easingValue", o(vec![("bezierEasing", handles([0.0, 0.0, 1.0, 1.0]))]))])),
        ("motionPathBezierHandles", handles([0.0; 4])),
    ];
    if let Some(time) = time_us {
        fields.push(("timelinePosition", Kv::Int(time)));
    }
    node(id, Some(40), "KEYFRAME", "Original synthetic key", fields)
}
fn nodes() -> Vec<Kv<'static>> {
    vec![
        node(0, None, "DOCUMENT", "Original native Motion control", vec![]),
        node(10, Some(0), "CANVAS", "Design", vec![]),
        node(11, Some(0), "CANVAS", "Original internal resources", vec![("internalOnly", Kv::Bool(true))]),
        node(
            20,
            Some(10),
            "FRAME",
            "Viewport",
            vec![("size", xy(240.0, 120.0)), ("transform", at(0.0, 0.0)), ("timelineDuration", Kv::Uint(1_000_000))],
        ),
        node(
            30,
            Some(20),
            "ROUNDED_RECTANGLE",
            "Moving rectangle",
            vec![
                ("size", xy(40.0, 40.0)),
                ("transform", at(10.0, 30.0)),
                ("parameterConsumptionMap", binding("MOTION_TRANSLATION_XY", 40, "MANUAL")),
            ],
        ),
        node(40, Some(11), "KEYFRAME_TRACK", "Original position track", vec![]),
        key(41, None, 0.0),
        key(42, Some(1_000_000), 170.0),
    ]
}
fn bytes(nodes: Vec<Kv<'static>>) -> Vec<u8> {
    let schema = Schema::parse(SCHEMA).unwrap();
    let message = o(vec![("nodeChanges", Kv::Array(nodes))]);
    let encoded = encode::message(&schema, schema.find("Message").unwrap(), &message).unwrap();
    super::export::canvas(&encode::schema(&schema), &encoded).unwrap()
}
fn import(nodes: Vec<Kv<'static>>) -> (Document, Vec<String>) {
    super::import(&bytes(nodes), "Original native control").unwrap()
}
fn moving(doc: &Document) -> NodeId {
    doc.nodes().find(|node| node.name == "Moving rectangle").unwrap().id
}
fn assert_unsupported(nodes: Vec<Kv<'static>>, reason: &str) {
    let (doc, warnings) = import(nodes);
    assert!(doc.node(moving(&doc)).unwrap().motion.is_empty(), "unsupported track must not be presented as preserved");
    assert!(
        warnings.iter().any(|warning| warning.contains("Motion") && warning.to_ascii_lowercase().contains(reason)),
        "expected counted Motion reason {reason:?}: {warnings:?}"
    );
}

#[test]
fn native_motion_linear_xy_import_retains_editable_keys_clock_and_independent_poses() {
    let (doc, warnings) = import(nodes());
    let page = doc.first_page().unwrap();
    let id = moving(&doc);
    let node = doc.node(id).unwrap();
    let track = node.motion.track(AnimProperty::Position).expect("genuine native KEYFRAME binding must import as editable Position");
    assert_eq!(track.keys.iter().map(|key| key.time_ms).collect::<Vec<_>>(), vec![0, 1000]);
    assert_eq!(track.keys[0].value, AnimValue::Vec2(Vec2::new(10.0, 30.0)));
    assert_eq!(track.keys[1].value, AnimValue::Vec2(Vec2::new(180.0, 30.0)));
    assert_eq!(base_position(node), Vec2::new(10.0, 30.0));
    assert_eq!(doc.timelines.get(&page).unwrap().duration_ms, 1000);
    assert_eq!(doc.timelines.get(&page).unwrap().fps, 60);
    assert!(!doc.timelines.get(&page).unwrap().looping);
    assert!(
        warnings.iter().any(|warning| warning.contains("60") && warning.contains("once")),
        "unproved native fps/playback defaults must be disclosed: {warnings:?}"
    );
    assert_eq!(doc.nodes().count(), 3, "resource canvas, track and keys must never become visible phantom layers");
    let before = crate::to_json(&doc).unwrap();
    let mut player = MotionPlayer::new(&doc, page);
    for (time, center_x) in [(0.0, 30.0), (250.0, 72.5), (500.0, 115.0), (750.0, 157.5), (1000.0, 200.0)] {
        let pose = player.seek(&doc, time).node(id).unwrap();
        assert!((base_position(pose).x + 20.0 - center_x).abs() < 1e-6);
        assert!((base_position(pose).y + 20.0 - 50.0).abs() < 1e-6);
    }
    assert_eq!(crate::to_json(&doc).unwrap(), before);
    let (back, _) = crate::from_json(&before.to_string()).unwrap();
    assert_eq!(back.node(moving(&back)).unwrap().motion, node.motion);
    assert_eq!(back.timelines, doc.timelines);
}

#[test]
fn native_motion_static_source_is_quiet_and_generic_rest_cannot_inject_motion() {
    let mut source = nodes();
    source.truncate(5);
    remove(&mut source[3], "timelineDuration");
    remove(&mut source[4], "parameterConsumptionMap");
    let (doc, warnings) = import(source);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(doc.timelines.is_empty() && doc.nodes().all(|node| node.motion.is_empty()));
    let fake = json!({"document":{"id":"0","type":"DOCUMENT","children":[{"id":"1","type":"CANVAS","children":[{"id":"2","type":"RECTANGLE","name":"Injected","absoluteBoundingBox":{"x":0,"y":0,"width":40,"height":40},"_figMotion":{"duration_ms":1000,"tracks":[{"property":"Position","keys":[{"time_ms":0,"value":{"Vec2":{"x":1,"y":2}}}]}]},"motion":{"tracks":[{"property":"Position","keys":[{"time_ms":0,"value":{"Vec2":{"x":1,"y":2}}}]}]}}]}]},"_figMotion":{"duration_ms":1000}});
    let (plain, _) = crate::rest::import(&fake, &crate::rest::no_images).unwrap();
    assert!(plain.timelines.is_empty() && plain.nodes().all(|node| node.motion.is_empty()));
}

#[test]
fn native_motion_last_guid_change_wins_and_removed_track_is_reported() {
    let mut source = nodes();
    source.push(key(42, Some(1_000_000), 85.0));
    let (doc, _) = import(source);
    assert_eq!(doc.node(moving(&doc)).unwrap().motion.track(AnimProperty::Position).unwrap().keys[1].value, AnimValue::Vec2(Vec2::new(95.0, 30.0)));
    let mut removed = nodes();
    let mut tombstone = node(40, None, "KEYFRAME_TRACK", "Removed", vec![]);
    set(&mut tombstone, "phase", e("REMOVED"));
    removed.push(tombstone);
    assert_unsupported(removed, "reference");
}

#[test]
fn native_motion_fractional_duplicate_and_negative_clocks_are_reported() {
    for (time, reason) in [(1_000_001, "millisecond"), (0, "duplicate"), (-1000, "negative"), (600_001_000, "range")] {
        let mut source = nodes();
        set(&mut source[7], "timelinePosition", Kv::Int(time));
        assert_unsupported(source, reason);
    }
}

#[test]
fn native_motion_unknown_hidden_spatial_and_nonfinite_tracks_are_reported() {
    let mut property = nodes();
    // Scale is now a known property. OPACITY keeps this original unknown-property oracle meaningful.
    set(&mut property[4], "parameterConsumptionMap", binding("OPACITY", 40, "MANUAL"));
    assert_unsupported(property, "property");
    let mut hidden = nodes();
    set(&mut hidden[5], "visible", Kv::Bool(false));
    assert_unsupported(hidden, "hidden");
    let mut dangling = nodes();
    set(&mut dangling[4], "parameterConsumptionMap", binding("MOTION_TRANSLATION_XY", 999, "MANUAL"));
    assert_unsupported(dangling, "reference");
    let mut spatial = nodes();
    set(&mut spatial[6], "motionPathBezierHandles", handles([1.0, 0.0, 0.0, 0.0]));
    assert_unsupported(spatial, "spatial");
    let mut invalid = nodes();
    set(&mut invalid[7], "keyframeValue", o(vec![("valueType", e("VECTOR")), ("value", o(vec![("vectorValue", xy(f32::INFINITY, 0.0))]))]));
    assert_unsupported(invalid, "finite");
}

#[test]
fn native_motion_preset_interpolation_resource_cycles_and_ambiguous_owners_are_reported() {
    let mut preset = nodes();
    set(&mut preset[4], "parameterConsumptionMap", binding("MOTION_TRANSLATION_XY", 40, "ANIMATION_PRESET"));
    assert_unsupported(preset, "preset");
    let mut interpolation = nodes();
    set(&mut interpolation[6], "interpolationType", e("HOLD"));
    assert_unsupported(interpolation, "interpolation");
    let mut cycle = nodes();
    set(&mut cycle[5], "parentIndex", o(vec![("guid", g(40)), ("position", Kv::Str("a".into()))]));
    assert_unsupported(cycle, "resource");
    let mut multiple = nodes();
    multiple.push(node(
        21,
        Some(10),
        "FRAME",
        "Other viewport",
        vec![("size", xy(240.0, 120.0)), ("transform", at(0.0, 0.0)), ("timelineDuration", Kv::Uint(1_000_000))],
    ));
    multiple.push(node(
        31,
        Some(21),
        "ROUNDED_RECTANGLE",
        "Other rectangle",
        vec![("size", xy(40.0, 40.0)), ("transform", at(10.0, 30.0)), ("parameterConsumptionMap", binding("MOTION_TRANSLATION_XY", 40, "MANUAL"))],
    ));
    assert_unsupported(multiple, "ambiguous");
}

#[test]
fn native_motion_paint_and_effect_bindings_are_counted_without_silent_preservation() {
    for (field, member, reason) in [("fillPaints", "colorVar", "paint"), ("effects", "radiusVar", "effect")] {
        let mut source = nodes();
        let original = binding("MOTION_TRANSLATION_XY", 40, "MANUAL");
        let data = original.get("entries").unwrap().list()[0].get("variableData").unwrap().clone();
        remove(&mut source[4], "parameterConsumptionMap");
        set(&mut source[4], field, Kv::Array(vec![o(vec![(member, data)])]));
        assert_unsupported(source, reason);
    }
}

#[test]
fn native_motion_resource_and_shared_track_amplification_are_bounded_before_editable_keys() {
    let mut overlong = nodes();
    overlong.truncate(6);
    overlong.extend((0..=10_000).map(|i| key(1000 + i, Some(i as i64 * 1000), i as f32)));
    assert_unsupported(overlong, "budget");
    let mut fanout = nodes();
    fanout.truncate(6);
    for i in 0..200 {
        fanout.push(node(
            1000 + i,
            Some(20),
            "ROUNDED_RECTANGLE",
            "Original fanout rectangle",
            vec![
                ("size", xy(40.0, 40.0)),
                ("transform", at(10.0, 30.0)),
                ("parameterConsumptionMap", binding("MOTION_TRANSLATION_XY", 40, "MANUAL")),
            ],
        ));
    }
    fanout.extend((0..=500).map(|i| key(5000 + i, if i == 0 { None } else { Some(i as i64 * 2000) }, i as f32)));
    let (doc, warnings) = import(fanout);
    assert!(doc.nodes().all(|node| node.motion.is_empty()));
    assert!(warnings.iter().any(|warning| warning.contains("Motion") && warning.contains("expansion budget")), "{warnings:?}");
}

#[test]
fn native_motion_locked_and_nondefault_opacity_resources_are_reported() {
    for index in [5, 6] {
        let mut locked = nodes();
        set(&mut locked[index], "locked", Kv::Bool(true));
        assert_unsupported(locked, "locked");
        let mut translucent = nodes();
        set(&mut translucent[index], "opacity", f(0.5));
        assert_unsupported(translucent, "opacity");
    }
    let mut defaults = nodes();
    for index in [5, 6, 7] {
        set(&mut defaults[index], "locked", Kv::Bool(false));
        remove(&mut defaults[index], "opacity");
    }
    let (doc, _) = import(defaults);
    assert!(doc.node(moving(&doc)).unwrap().motion.track(AnimProperty::Position).is_some());
}

// Original minimal Scale control from the separately audited native/SVG numeric contract.
// Both keys deliberately use the same observed curve; this does not establish curve ownership.
fn scale_key(id: u64, time_us: Option<i64>, ratio: f32) -> Kv<'static> {
    let mut value = key(id, time_us, 0.0);
    set(&mut value, "keyframeValue", o(vec![("valueType", e("VECTOR")), ("value", o(vec![("vectorValue", xy(ratio, ratio))]))]));
    set(
        &mut value,
        "easingData",
        o(vec![("easingType", e("CUSTOM_CUBIC")), ("easingValue", o(vec![("bezierEasing", handles([0.5, 0.0, 0.5, 1.0]))]))]),
    );
    remove(&mut value, "motionPathBezierHandles");
    value
}

fn scale_nodes() -> Vec<Kv<'static>> {
    let mut source = nodes();
    set(&mut source[4], "parameterConsumptionMap", binding_with_fallback("MOTION_SCALE_XY", 40, "MANUAL", [1.0, 1.0]));
    source[6] = scale_key(41, None, 1.0);
    source[7] = scale_key(42, Some(1_000_000), 2.0);
    source
}

#[test]
fn native_motion_scale_xy_import_retains_ratios_curve_clock_and_source() {
    let (doc, warnings) = import(scale_nodes());
    let id = moving(&doc);
    let node = doc.node(id).unwrap();
    let track = node.motion.track(AnimProperty::Scale).expect("genuine native Scale binding must retain editable ratio keys");
    assert!(node.motion.track(AnimProperty::Position).is_none(), "Scale ratios must not become layout positions");
    assert_eq!(track.keys.iter().map(|key| key.time_ms).collect::<Vec<_>>(), vec![0, 1000]);
    assert_eq!(track.keys[0].value, AnimValue::Vec2(Vec2::new(1.0, 1.0)));
    assert_eq!(track.keys[1].value, AnimValue::Vec2(Vec2::new(2.0, 2.0)));
    assert!(track.keys.iter().all(|key| key.easing == Ease::CustomBezier([0.5, 0.0, 0.5, 1.0])));
    assert_eq!(node.transform.as_coeffs(), [1.0, 0.0, 0.0, 1.0, 10.0, 30.0]);
    assert_eq!((node.width, node.height), (40.0, 40.0));
    assert_eq!(doc.nodes().count(), 3, "native track resources must not become visible layers");
    let page = doc.first_page().unwrap();
    let timeline = doc.timelines.get(&page).expect("Scale must retain its explicit native frame clock");
    assert_eq!((timeline.duration_ms, timeline.fps, timeline.looping), (1000, 60, false));
    assert!(warnings.iter().any(|warning| warning.contains("60") && warning.contains("once")), "{warnings:?}");
    let before = crate::to_json(&doc).unwrap();
    let (back, _) = crate::from_json(&before.to_string()).unwrap();
    assert_eq!(crate::to_json(&back).unwrap(), before, "save/reopen must preserve editable Scale and static source");
}

#[test]
fn native_motion_scale_xy_import_matches_native_svg_corners_and_clipping() {
    let (doc, _) = import(scale_nodes());
    let id = moving(&doc);
    assert!(doc.node(id).unwrap().motion.track(AnimProperty::Scale).is_some(), "genuine native Scale must be preserved before pose comparison");
    let before = crate::to_json(&doc).unwrap();
    let mut player = MotionPlayer::new(&doc, doc.first_page().unwrap());
    // Literal numeric oracle from independent SVG cubic inversion, not the model easing helper.
    let samples = [
        (0.0, [10.0, 30.0, 50.0, 70.0]),
        (250.0, [7.882149, 27.882149, 52.117851, 72.117851]),
        (500.0, [0.0, 20.0, 60.0, 80.0]),
        (750.0, [-7.882149, 12.117851, 67.882149, 87.882149]),
        (1000.0, [-10.0, 10.0, 70.0, 90.0]),
    ];
    for (time, expected) in samples {
        let pose = player.seek(&doc, time).node(id).unwrap();
        let c = pose.transform.as_coeffs();
        let corners = [(0.0, 0.0), (pose.width, 0.0), (pose.width, pose.height), (0.0, pose.height)]
            .map(|(x, y)| [c[0] * x + c[2] * y + c[4], c[1] * x + c[3] * y + c[5]]);
        let expected_corners = [[expected[0], expected[1]], [expected[2], expected[1]], [expected[2], expected[3]], [expected[0], expected[3]]];
        for (actual, expected) in corners.iter().zip(expected_corners) {
            assert!(
                (actual[0] - expected[0]).abs() <= 1.0 / 1024.0 && (actual[1] - expected[1]).abs() <= 1.0 / 1024.0,
                "time={time}: {actual:?} != {expected:?}"
            );
        }
        let bounds = [corners[0][0], corners[0][1], corners[2][0], corners[2][1]];
        let clipped = [bounds[0].max(0.0), bounds[1].max(0.0), bounds[2].min(240.0), bounds[3].min(120.0)];
        let expected_clipped = [expected[0].max(0.0), expected[1].max(0.0), expected[2].min(240.0), expected[3].min(120.0)];
        for (actual, expected) in clipped.into_iter().zip(expected_clipped) {
            assert!((actual - expected).abs() <= 1.0 / 1024.0, "clipped time={time}: {actual} != {expected}");
        }
        let center = [c[0] * 20.0 + c[2] * 20.0 + c[4], c[1] * 20.0 + c[3] * 20.0 + c[5]];
        assert!((center[0] - 30.0).abs() <= 1.0 / 1024.0 && (center[1] - 50.0).abs() <= 1.0 / 1024.0);
        assert_eq!((pose.width, pose.height), (40.0, 40.0));
    }
    assert_eq!(crate::to_json(&doc).unwrap(), before, "seeking must not mutate the saved source");
}

#[test]
fn native_motion_scale_xy_fallback_and_uniform_ratio_limits_have_positive_control() {
    let mut maximum = scale_nodes();
    maximum[7] = scale_key(42, Some(1_000_000), 100.0);
    let (doc, warnings) = import(maximum);
    let track = doc.node(moving(&doc)).unwrap().motion.track(AnimProperty::Scale).expect("the admitted scale limit must be usable");
    assert_eq!(track.keys[1].value, AnimValue::Vec2(Vec2::new(100.0, 100.0)), "{warnings:?}");

    let mut fallback = scale_nodes();
    set(&mut fallback[4], "parameterConsumptionMap", binding("MOTION_SCALE_XY", 40, "MANUAL"));
    assert_unsupported(fallback, "fallback");
    for (x, y, reason) in [
        (0.0, 0.0, "scale"),
        (-1.0, -1.0, "scale"),
        (100.00001, 100.00001, "scale"),
        (1.0, 2.0, "uniform"),
        (f32::INFINITY, f32::INFINITY, "finite"),
        (f32::NAN, f32::NAN, "finite"),
    ] {
        let mut source = scale_nodes();
        set(&mut source[7], "keyframeValue", o(vec![("valueType", e("VECTOR")), ("value", o(vec![("vectorValue", xy(x, y))]))]));
        if x.is_nan() || y.is_nan() {
            // The wire encoder correctly refuses NaN; test the native parser's own guard directly.
            assert_unsupported_native_metadata(source, reason);
        } else {
            assert_unsupported(source, reason);
        }
    }
    let mut wrong_tag = scale_nodes();
    set(&mut wrong_tag[7], "keyframeValue", o(vec![("valueType", e("FLOAT")), ("value", o(vec![("vectorValue", xy(2.0, 2.0))]))]));
    assert_unsupported(wrong_tag, "union");
}

#[test]
fn native_motion_scale_xy_requires_exact_explicit_curve_and_absent_spatial_handles() {
    for curve in [[0.4, 0.0, 0.5, 1.0], [0.5, 0.0, 0.5, 1.1], [f32::NAN, 0.0, 0.5, 1.0]] {
        let mut source = scale_nodes();
        set(&mut source[6], "easingData", o(vec![("easingType", e("CUSTOM_CUBIC")), ("easingValue", o(vec![("bezierEasing", handles(curve))]))]));
        if curve.iter().any(|value| value.is_nan()) {
            // Preserve the parser's finite-curve check without asking the encoder to emit NaN.
            assert_unsupported_native_metadata(source, "easing");
        } else {
            assert_unsupported(source, "easing");
        }
    }
    let mut missing = scale_nodes();
    remove(&mut missing[6], "easingData");
    assert_unsupported(missing, "easing");
    let mut wrong_kind = scale_nodes();
    set(
        &mut wrong_kind[7],
        "easingData",
        o(vec![("easingType", e("LINEAR")), ("easingValue", o(vec![("bezierEasing", handles([0.5, 0.0, 0.5, 1.0]))]))]),
    );
    assert_unsupported(wrong_kind, "easing");
    let mut spatial = scale_nodes();
    set(&mut spatial[6], "motionPathBezierHandles", handles([0.0; 4]));
    assert_unsupported(spatial, "spatial");
    let mut interpolation = scale_nodes();
    set(&mut interpolation[6], "interpolationType", e("HOLD"));
    assert_unsupported(interpolation, "interpolation");

    let mut position_curve = nodes();
    set(
        &mut position_curve[6],
        "easingData",
        o(vec![("easingType", e("CUSTOM_CUBIC")), ("easingValue", o(vec![("bezierEasing", handles([0.5, 0.0, 0.5, 1.0]))]))]),
    );
    assert_unsupported(position_curve, "easing");
    let mut position_fallback = nodes();
    set(&mut position_fallback[4], "parameterConsumptionMap", binding_with_fallback("MOTION_TRANSLATION_XY", 40, "MANUAL", [1.0, 1.0]));
    assert_unsupported(position_fallback, "fallback");
    let mut position_spatial = nodes();
    remove(&mut position_spatial[6], "motionPathBezierHandles");
    assert_unsupported(position_spatial, "spatial");
}

// Opaque metadata deliberately bypasses this test's required-fields-only wire schema. It enters
// the same typed native sidecar from Kv, never ordinary REST, and tests presence-based refusals.
fn assert_unsupported_native_metadata(source: Vec<Kv<'static>>, reason: &str) {
    let mut static_source = nodes();
    static_source.truncate(5);
    remove(&mut static_source[3], "timelineDuration");
    remove(&mut static_source[4], "parameterConsumptionMap");
    let (mut doc, static_notes) = import(static_source);
    assert!(static_notes.is_empty());
    let target = moving(&doc);
    let frame = doc.node(target).unwrap().parent.unwrap();
    let ids = std::collections::BTreeMap::from([("7:30".into(), target), ("7:20".into(), frame)]);
    let message = o(vec![("nodeChanges", Kv::Array(source))]);
    let native = super::motion::NativeMotion::read(&message, 200_000);
    let warnings = native.apply(&mut doc, &ids);
    assert!(doc.nodes().all(|node| node.motion.is_empty()), "unproved native metadata must not produce an editable track");
    assert!(warnings.iter().any(|warning| warning.contains("Motion") && warning.to_ascii_lowercase().contains(reason)), "{reason}: {warnings:?}");
}

#[test]
fn native_motion_scale_xy_rejects_unproved_origins_transforms_and_key_metadata() {
    for index in [3, 4] {
        for (field, value, reason) in [
            ("transformOrigin", xy(0.5, 0.5), "origin"),
            ("motionTransform", o(vec![("scale", xy(1.0, 1.0))]), "transform"),
            ("transformModifiers", Kv::Array(vec![]), "modifiers"),
            ("linkMotionScaleAspectRatio", Kv::Bool(true), "transform"),
        ] {
            let mut source = scale_nodes();
            set(&mut source[index], field, value);
            assert_unsupported_native_metadata(source, reason);
        }
        let mut locked = scale_nodes();
        set(&mut locked[index], "locked", Kv::Bool(true));
        assert_unsupported(locked, "locked");
        let mut nonidentity = scale_nodes();
        let mut transform = at(10.0, 30.0);
        set(&mut transform, "m00", f(2.0));
        set(&mut nonidentity[index], "transform", transform);
        assert_unsupported(nonidentity, "nonidentity");
    }
    let mut unknown_key = scale_nodes();
    set(&mut unknown_key[6], "futureOriginalMetadata", Kv::Bool(true));
    assert_unsupported_native_metadata(unknown_key, "metadata");
}

#[test]
fn native_motion_scale_xy_key_and_static_extents_are_bounded_before_preservation() {
    let mut positive = scale_nodes();
    set(&mut positive[4], "transform", at(0.0, 0.0));
    set(&mut positive[4], "size", xy(1_000_000.0, 40.0));
    positive[7] = scale_key(42, Some(1_000_000), 100.0);
    let (doc, warnings) = import(positive);
    assert!(doc.node(moving(&doc)).unwrap().motion.track(AnimProperty::Scale).is_some(), "{warnings:?}");
    // Native scale extent exactly 1e9 passes the sidecar's bound, but REST has a stricter 1e6
    // static size cap. Its geometry mismatch must still refuse preservation after projection.
    let mut boundary = scale_nodes();
    set(&mut boundary[4], "transform", at(0.0, 0.0));
    set(&mut boundary[4], "size", xy(20_000_000.0, 40.0));
    boundary[7] = scale_key(42, Some(1_000_000), 99.0);
    assert_unsupported(boundary, "geometry");
    let mut scaled = scale_nodes();
    set(&mut scaled[4], "transform", at(0.0, 0.0));
    set(&mut scaled[4], "size", xy(20_000_000.0, 40.0));
    scaled[7] = scale_key(42, Some(1_000_000), 100.0);
    assert_unsupported(scaled, "extent");
    let mut static_extent = scale_nodes();
    set(&mut static_extent[4], "transform", at(1_000_000_000.0, 30.0));
    assert_unsupported(static_extent, "extent");
}

#[test]
fn native_motion_scale_xy_multiple_property_and_animated_ancestor_bindings_are_reported() {
    let mut multiple = scale_nodes();
    let scale = multiple[4].get("parameterConsumptionMap").unwrap().get("entries").unwrap().list()[0].clone();
    let position = binding("MOTION_TRANSLATION_XY", 40, "MANUAL").get("entries").unwrap().list()[0].clone();
    set(&mut multiple[4], "parameterConsumptionMap", o(vec![("entries", Kv::Array(vec![scale, position]))]));
    assert_unsupported(multiple, "multiple");
    let mut ancestor = scale_nodes();
    set(&mut ancestor[3], "parameterConsumptionMap", binding("MOTION_TRANSLATION_XY", 40, "MANUAL"));
    assert_unsupported(ancestor, "ancestor");
}

fn scale_fanout(targets: u64, key_count: u64) -> Vec<Kv<'static>> {
    let mut source = scale_nodes();
    source.truncate(6);
    for i in 1..targets {
        source.push(node(
            1000 + i,
            Some(20),
            "ROUNDED_RECTANGLE",
            "Original shared scale target",
            vec![
                ("size", xy(40.0, 40.0)),
                ("transform", at(10.0, 30.0)),
                ("parameterConsumptionMap", binding_with_fallback("MOTION_SCALE_XY", 40, "MANUAL", [1.0, 1.0])),
            ],
        ));
    }
    for i in 0..key_count {
        let time = if i + 1 == key_count { 1_000_000 } else { i as i64 * 2000 };
        source.push(scale_key(5000 + i, (i != 0).then_some(time), 1.0 + i as f32 / key_count as f32));
    }
    source
}

#[test]
fn native_motion_scale_xy_shared_resource_expansion_has_exact_positive_boundary() {
    let (doc, warnings) = import(scale_fanout(200, 500));
    let tracks: Vec<_> = doc.nodes().filter_map(|node| node.motion.track(AnimProperty::Scale)).collect();
    assert_eq!(tracks.len(), 200, "{warnings:?}");
    assert_eq!(tracks.iter().map(|track| track.keys.len()).sum::<usize>(), 100_000);
    let (over, warnings) = import(scale_fanout(201, 500));
    assert!(over.nodes().all(|node| node.motion.is_empty()));
    assert!(warnings.iter().any(|warning| warning.contains("expansion budget")), "{warnings:?}");
}

fn assert_malformed_scale_ancestor_is_not_preserved(index: usize) {
    let (positive, warnings) = import(scale_nodes());
    assert!(
        positive.node(moving(&positive)).unwrap().motion.track(AnimProperty::Scale).is_some(),
        "known-positive Scale must remain supported: {warnings:?}"
    );
    let mut source = scale_nodes();
    let position = binding("MOTION_TRANSLATION_XY", 40, "MANUAL");
    let expression = position.get("entries").unwrap().list()[0].get("variableData").unwrap().clone();
    set(
        &mut source[index],
        "parameterConsumptionMap",
        o(vec![("entries", Kv::Str("Original malformed map".into())), ("originalOpaqueData", expression)]),
    );
    assert_unsupported_native_metadata(source, "ancestor");
}

#[test]
fn native_motion_scale_xy_malformed_frame_parameter_map_cannot_hide_animated_ancestor() {
    assert_malformed_scale_ancestor_is_not_preserved(3);
}

#[test]
fn native_motion_scale_xy_malformed_page_parameter_map_cannot_hide_animated_ancestor() {
    assert_malformed_scale_ancestor_is_not_preserved(1);
}

fn add_original_binding_metadata(value: &mut Kv<'static>, path: &[&str]) {
    let Some((head, tail)) = path.split_first() else {
        set(value, "originalUnknownTiming", Kv::Bool(true));
        return;
    };
    let next = match value {
        Kv::Object(fields) => fields.iter_mut().find(|(name, _)| name == head).map(|(_, value)| value).unwrap(),
        Kv::Array(values) => values.get_mut(head.parse::<usize>().unwrap()).unwrap(),
        _ => panic!("the original test metadata path must refer to a container"),
    };
    add_original_binding_metadata(next, tail);
}

fn assert_unknown_scale_wrapper_is_not_preserved(path: &[&str]) {
    let (positive, warnings) = import(scale_nodes());
    assert!(
        positive.node(moving(&positive)).unwrap().motion.track(AnimProperty::Scale).is_some(),
        "known-positive Scale must remain supported: {warnings:?}"
    );
    let mut source = scale_nodes();
    let mut full_path = vec!["parameterConsumptionMap", "entries", "0", "variableData"];
    full_path.extend_from_slice(path);
    add_original_binding_metadata(&mut source[4], &full_path);
    assert_unsupported_native_metadata(source, "metadata");
}

#[test]
fn native_motion_scale_xy_unknown_variable_data_metadata_cannot_change_semantics() {
    assert_unknown_scale_wrapper_is_not_preserved(&[]);
}

#[test]
fn native_motion_scale_xy_unknown_fallback_metadata_cannot_change_semantics() {
    assert_unknown_scale_wrapper_is_not_preserved(&["value", "expressionValue", "expressionArguments", "0"]);
}

#[test]
fn native_motion_scale_xy_unknown_reference_metadata_cannot_change_semantics() {
    assert_unknown_scale_wrapper_is_not_preserved(&["value", "expressionValue", "expressionArguments", "1"]);
}

#[test]
fn native_motion_scale_xy_unknown_parameter_container_metadata_cannot_change_semantics() {
    assert_unknown_scale_wrapper_is_not_preserved(&["value", "expressionValue", "expressionArguments", "1", "value", "keyframeTrackParameterValue"]);
}

#[test]
fn native_motion_scale_xy_unknown_manual_parameter_metadata_cannot_change_semantics() {
    assert_unknown_scale_wrapper_is_not_preserved(&[
        "value",
        "expressionValue",
        "expressionArguments",
        "1",
        "value",
        "keyframeTrackParameterValue",
        "parameters",
        "0",
    ]);
}

fn scale_map_with_unknown_sibling(index: usize, extra: Kv<'static>) -> Vec<Kv<'static>> {
    let mut source = scale_nodes();
    let mut map = source[index].get("parameterConsumptionMap").cloned().unwrap_or_else(|| o(vec![("entries", Kv::Array(vec![]))]));
    set(&mut map, "originalUnknownSibling", extra);
    set(&mut source[index], "parameterConsumptionMap", map);
    source
}

fn assert_unknown_scale_map_sibling_is_not_preserved(index: usize) {
    let (positive, warnings) = import(scale_nodes());
    assert!(
        positive.node(moving(&positive)).unwrap().motion.track(AnimProperty::Scale).is_some(),
        "known-positive Scale must remain supported: {warnings:?}"
    );
    let position = binding("MOTION_TRANSLATION_XY", 40, "MANUAL");
    let expression = position.get("entries").unwrap().list()[0].get("variableData").unwrap().clone();
    let reference = o(vec![("keyframeTrackId", o(vec![("guid", g(40))]))]);
    for extra in [expression, reference, Kv::Bool(true)] {
        assert_unsupported_native_metadata(scale_map_with_unknown_sibling(index, extra), "metadata");
    }
}

#[test]
fn native_motion_scale_xy_unknown_sibling_of_valid_target_map_cannot_hide_metadata() {
    assert_unknown_scale_map_sibling_is_not_preserved(4);
}

#[test]
fn native_motion_scale_xy_unknown_sibling_of_valid_frame_map_cannot_hide_animated_ancestor() {
    assert_unknown_scale_map_sibling_is_not_preserved(3);
}

#[test]
fn native_motion_scale_xy_unknown_sibling_of_valid_page_map_cannot_hide_animated_ancestor() {
    assert_unknown_scale_map_sibling_is_not_preserved(1);
}

#[test]
fn native_motion_scale_xy_unknown_map_siblings_share_the_existing_inspection_budget() {
    let (positive, warnings) = import(scale_nodes());
    assert!(positive.node(moving(&positive)).unwrap().motion.track(AnimProperty::Scale).is_some(), "{warnings:?}");
    let mut nested = o(vec![("keyframeTrackId", o(vec![("guid", g(40))]))]);
    for _ in 0..40 {
        nested = o(vec![("originalNestedMetadata", nested)]);
    }
    assert_unsupported_native_metadata(scale_map_with_unknown_sibling(1, nested), "budget");
}
