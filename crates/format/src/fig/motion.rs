//! Bounded inward native Motion subset, derived from original synthetic black-box controls.
//! A typed sidecar survives the native→REST projection, never arbitrary REST metadata.

use std::collections::{BTreeMap, BTreeSet};

use figcraft_doc::anim::motion::{AnimProperty, AnimValue, Ease, Keyframe, TimelineSettings, Track, base_position};
use figcraft_doc::geom::Vec2;
use figcraft_doc::{Document, NodeId, NodeKind};

use super::kiwi::Kv;

const MAX_GRAPH: usize = 100_000;
const MAX_BINDINGS: usize = 10_000;
const MAX_SCALE_CONTEXTS: usize = MAX_BINDINGS * 3;
const MAX_KEYS: usize = 10_000;
const MAX_TOTAL_KEYS: usize = 100_000;
const MAX_VISITS: usize = 1_000_000;
const MAX_DEPTH: usize = 32;
const MAX_COORD: f64 = 1e9;
const MAX_SCALE: f64 = 100.0;

type Reason = &'static str;
type Nodes<'a> = BTreeMap<Guid, &'a Kv<'a>>;

#[derive(Clone, Copy)]
enum MotionProperty {
    Position,
    Scale,
}

impl MotionProperty {
    fn fallback(self) -> Vec2 {
        match self {
            Self::Position => Vec2::ZERO,
            Self::Scale => Vec2::new(1.0, 1.0),
        }
    }

    fn model(self) -> AnimProperty {
        match self {
            Self::Position => AnimProperty::Position,
            Self::Scale => AnimProperty::Scale,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Guid(u32, u32);

impl Guid {
    fn read(value: &Kv<'_>) -> Option<Self> {
        if !object_has_only(value, &["sessionID", "localID"]) {
            return None;
        }
        let uint = |name| match value.get(name)? {
            Kv::Uint(value) => u32::try_from(*value).ok(),
            _ => None,
        };
        Some(Self(uint("sessionID")?, uint("localID")?))
    }

    fn rest(self) -> String {
        format!("{}:{}", self.0, self.1)
    }
}

#[derive(Default)]
struct Notes(BTreeMap<Reason, usize>);

impl Notes {
    fn add(&mut self, reason: Reason, count: usize) {
        if count > 0 {
            let entry = self.0.entry(reason).or_default();
            *entry = entry.saturating_add(count);
        }
    }

    fn finish(self) -> Vec<String> {
        self.0.into_iter().map(|(reason, count)| format!("Motion import: {count} {reason}")).collect()
    }
}

struct Pending {
    target: String,
    owner: String,
    base: Vec2,
    size: Vec2,
    duration_ms: u32,
    property: MotionProperty,
    keys: Vec<Keyframe>,
}

#[derive(Default)]
pub(crate) struct NativeMotion {
    pending: Vec<Pending>,
    notes: Notes,
}

#[derive(Default)]
struct Scan {
    visits: usize,
    limited: bool,
    references: BTreeSet<Guid>,
    reference_markers: usize,
}

impl Scan {
    /// Iterative bounded traversal, with capacity charged before pushing array/object children.
    fn expressions(&mut self, root: &Kv<'_>) -> usize {
        self.expressions_at(root, 0)
    }

    fn expressions_at(&mut self, root: &Kv<'_>, initial_depth: usize) -> usize {
        let mut count = 0usize;
        let mut work = vec![(root, initial_depth)];
        while let Some((value, depth)) = work.pop() {
            self.visits = self.visits.saturating_add(1);
            if self.visits > MAX_VISITS || depth > MAX_DEPTH {
                self.limited = true;
                break;
            }
            if value.get("expressionFunction").and_then(Kv::str) == Some("KEYFRAME") {
                count += 1;
            }
            if value.get("keyframeTrackId").is_some()
                || value.get("animationPreset").is_some()
                || value.get("type").and_then(Kv::str) == Some("ANIMATION_PRESET")
            {
                self.reference_markers = self.reference_markers.saturating_add(1);
            }
            if let Some(reference) = value.get("keyframeTrackId").and_then(|id| id.get("guid")).and_then(Guid::read) {
                if !self.references.contains(&reference) && self.references.len() >= MAX_BINDINGS {
                    self.limited = true;
                    break;
                }
                self.references.insert(reference);
            }
            let left = MAX_VISITS.saturating_sub(self.visits).saturating_sub(work.len());
            match value {
                Kv::Array(values) if values.len() <= left => work.extend(values.iter().map(|v| (v, depth + 1))),
                Kv::Object(fields) if fields.len() <= left => work.extend(fields.iter().map(|(_, v)| (v, depth + 1))),
                Kv::Array(_) | Kv::Object(_) => {
                    self.limited = true;
                    break;
                }
                _ => {}
            }
        }
        count
    }
}

fn parent(node: &Kv<'_>) -> Option<Guid> {
    node.get("parentIndex")?.get("guid").and_then(Guid::read)
}

fn kind<'a>(node: &'a Kv<'_>) -> Option<&'a str> {
    node.get("type").and_then(Kv::str)
}

fn native_fields<'a>(node: &'a Kv<'a>) -> impl Iterator<Item = (&'a str, &'a Kv<'a>)> {
    let fields = match node {
        Kv::Object(fields) => fields.as_slice(),
        _ => &[],
    };
    fields
        .iter()
        .filter(|(name, _)| {
            matches!(
                *name,
                "parameterConsumptionMap"
                    | "variableConsumptionMap"
                    | "fillPaints"
                    | "strokePaints"
                    | "effects"
                    | "symbolData"
                    | "derivedSymbolData"
                    | "keyframeValueRef"
                    | "variableData"
                    | "animationPresetData"
            ) || name.ends_with("Var")
        })
        .map(|(name, value)| (*name, value))
}

fn has_timeline(node: &Kv<'_>) -> bool {
    [
        "timelineDuration",
        "timelineOffset",
        "timelineDisabled",
        "timelineDefinitions",
        "timelineAssignments",
        "playbackStyle",
        "timelineKeyframeSnappingFps",
        "quantizeTimelinePlayback",
    ]
    .into_iter()
    .any(|name| node.get(name).is_some())
}

fn motion_hint(node: &Kv<'_>) -> bool {
    matches!(kind(node), Some("KEYFRAME" | "KEYFRAME_TRACK" | "ANIMATION_PRESET_INSTANCE")) || has_timeline(node)
}

/// Plain paint channels and static variable aliases cannot contain an animation expression.
/// Avoid charging their colors/GUIDs against the animation budget on large static imports.
fn discover(node: &Kv<'_>, scan: &mut Scan) -> bool {
    let mut found = false;
    let variable = |data: &Kv<'_>, scan: &mut Scan| {
        let Some(value) = data.get("value") else { return false };
        if value.get("expressionValue").is_none() && value.get("keyframeTrackParameterValue").is_none() && value.get("animationPreset").is_none() {
            return false;
        }
        scan.expressions(value) > 0 || scan.reference_markers > 0
    };
    for (field, value) in native_fields(node) {
        match field {
            "fillPaints" | "strokePaints" | "effects" => {
                for item in value.list() {
                    if let Kv::Object(fields) = item {
                        for (_, data) in fields.iter().filter(|(name, _)| name.ends_with("Var")) {
                            found |= variable(data, scan);
                        }
                    }
                }
            }
            "parameterConsumptionMap" | "variableConsumptionMap" if matches!(value.get("entries"), Some(Kv::Array(_))) => {
                for entry in value.get("entries").map(Kv::list).unwrap_or_default() {
                    if let Some(data) = entry.get("variableData") {
                        found |= variable(data, scan);
                    }
                }
            }
            "variableData" | "keyframeValueRef" => found |= variable(value, scan),
            _ => found |= scan.expressions(value) > 0 || scan.reference_markers > 0,
        }
        if found || scan.limited {
            break;
        }
    }
    found
}

fn vector(value: &Kv<'_>) -> Option<Vec2> {
    if !object_has_only(value, &["x", "y"]) {
        return None;
    }
    let read = |name| float(value.get(name)?).filter(|value| value.abs() <= MAX_COORD);
    Some(Vec2::new(read("x")?, read("y")?))
}

fn float(value: &Kv<'_>) -> Option<f64> {
    match value {
        Kv::Float(value) if value.is_finite() => Some(f64::from(*value)),
        _ => None,
    }
}

fn identity_translation(node: &Kv<'_>) -> Option<Vec2> {
    let value = node.get("transform")?;
    for (name, expected) in [("m00", 1.0), ("m01", 0.0), ("m10", 0.0), ("m11", 1.0)] {
        if value.get(name).and_then(float) != Some(expected) {
            return None;
        }
    }
    let x = value.get("m02").and_then(float)?;
    let y = value.get("m12").and_then(float)?;
    (x.abs() <= MAX_COORD && y.abs() <= MAX_COORD).then_some(Vec2::new(x, y))
}

fn object_has_only(value: &Kv<'_>, allowed: &[&str]) -> bool {
    matches!(value, Kv::Object(fields) if fields.len() <= allowed.len() && fields.iter().enumerate().all(|(i, (name, _))| allowed.contains(name) && !fields[..i].iter().any(|(previous, _)| previous == name)))
}

fn scale_wrapper_fields(property: MotionProperty, value: &Kv<'_>, allowed: &[&str]) -> Result<(), Reason> {
    if matches!(property, MotionProperty::Scale) && !object_has_only(value, allowed) {
        return Err("binding(s) omitted: unknown Scale binding metadata unsupported");
    }
    Ok(())
}

fn manual_reference(data: &Kv<'_>, property: MotionProperty) -> Result<Guid, Reason> {
    let malformed = "binding(s) omitted: malformed or unsupported KEYFRAME expression";
    scale_wrapper_fields(property, data, &["dataType", "resolvedDataType", "value"])?;
    if data.get("dataType").and_then(Kv::str) != Some("EXPRESSION") || data.get("resolvedDataType").and_then(Kv::str) != Some("VECTOR") {
        return Err(malformed);
    }
    let value = data.get("value").ok_or(malformed)?;
    if !object_has_only(value, &["expressionValue"]) {
        return Err(malformed);
    }
    let expression = value.get("expressionValue").ok_or(malformed)?;
    if !object_has_only(expression, &["expressionFunction", "expressionArguments"])
        || expression.get("expressionFunction").and_then(Kv::str) != Some("KEYFRAME")
    {
        return Err(malformed);
    }
    let args = expression.get("expressionArguments").map(Kv::list).ok_or(malformed)?;
    let [fallback, reference] = args else { return Err(malformed) };
    scale_wrapper_fields(property, fallback, &["dataType", "resolvedDataType", "value"])?;
    scale_wrapper_fields(property, reference, &["dataType", "resolvedDataType", "value"])?;
    if fallback.get("dataType").and_then(Kv::str) != Some("VECTOR")
        || fallback.get("resolvedDataType").and_then(Kv::str) != Some("VECTOR")
        || fallback.get("value").and_then(|v| v.get("vectorValue")).and_then(vector) != Some(property.fallback())
        || !fallback.get("value").is_some_and(|v| object_has_only(v, &["vectorValue"]))
    {
        return Err("binding(s) omitted: unsupported nonneutral or malformed fallback vector");
    }
    if reference.get("dataType").and_then(Kv::str) != Some("KEYFRAME_TRACK_PARAMETER_DATA")
        || reference.get("resolvedDataType").and_then(Kv::str) != Some("KEYFRAME_TRACK_PARAMETER_DATA")
    {
        return Err(malformed);
    }
    let value = reference.get("value").ok_or(malformed)?;
    if !object_has_only(value, &["keyframeTrackParameterValue"]) {
        return Err(malformed);
    }
    let container = value.get("keyframeTrackParameterValue").ok_or(malformed)?;
    scale_wrapper_fields(property, container, &["parameters"])?;
    let parameters = container.get("parameters").map(Kv::list).ok_or(malformed)?;
    let [parameter] = parameters else { return Err("binding(s) omitted: multiple or missing manual track references") };
    scale_wrapper_fields(property, parameter, &["type", "value"])?;
    if parameter.get("type").and_then(Kv::str) == Some("ANIMATION_PRESET") {
        return Err("binding(s) omitted: animation preset parameters unsupported");
    }
    if parameter.get("type").and_then(Kv::str) != Some("MANUAL") {
        return Err(malformed);
    }
    let any = parameter.get("value").ok_or(malformed)?;
    if !object_has_only(any, &["manual"]) {
        return Err("binding(s) omitted: named/style/preset track parameters unsupported");
    }
    let manual = any.get("manual").ok_or(malformed)?;
    if !object_has_only(manual, &["keyframeTrackId"]) {
        return Err("binding(s) omitted: named timeline parameters unsupported");
    }
    let id = manual.get("keyframeTrackId").ok_or(malformed)?;
    if !object_has_only(id, &["guid"]) {
        return Err("binding(s) omitted: external track references unsupported");
    }
    id.get("guid").and_then(Guid::read).ok_or("binding(s) omitted: invalid track reference")
}

fn owner<'a>(target: &Kv<'a>, nodes: &Nodes<'a>) -> Result<(Guid, Guid, Vec2, Vec2), Reason> {
    if !matches!(kind(target), Some("ROUNDED_RECTANGLE" | "RECTANGLE")) {
        return Err("binding(s) omitted: target is not a direct owned rectangle");
    }
    if target.get("visible").and_then(Kv::bool) != Some(true) {
        return Err("binding(s) omitted: hidden or unspecified target visibility");
    }
    let base = identity_translation(target).ok_or("binding(s) omitted: nonidentity or nonfinite target transform")?;
    let size =
        target.get("size").and_then(vector).filter(|v| v.x > 0.0 && v.y > 0.0).ok_or("binding(s) omitted: nonfinite or invalid rectangle size")?;
    let frame_id = parent(target).ok_or("binding(s) omitted: missing frame ownership reference")?;
    let frame = nodes.get(&frame_id).copied().ok_or("binding(s) omitted: missing frame ownership reference")?;
    if kind(frame) != Some("FRAME")
        || frame.get("isStateGroup").and_then(Kv::bool) == Some(true)
        || frame.get("resizeToFit").and_then(Kv::bool) == Some(true)
    {
        return Err("binding(s) omitted: nested/component/instance ownership unsupported");
    }
    if frame.get("visible").and_then(Kv::bool) != Some(true) {
        return Err("binding(s) omitted: hidden frame");
    }
    if identity_translation(frame).is_none() {
        return Err("binding(s) omitted: nonidentity frame transform");
    }
    if !frame.get("size").and_then(vector).is_some_and(|size| size.x > 0.0 && size.y > 0.0) {
        return Err("binding(s) omitted: nonfinite or invalid frame size");
    }
    if [target, frame].into_iter().any(|node| node.get("stackMode").and_then(Kv::str).is_some_and(|mode| mode != "NONE")) {
        return Err("binding(s) omitted: auto-layout ownership unsupported");
    }
    let page_id = parent(frame).ok_or("binding(s) omitted: missing page ownership reference")?;
    let page = nodes.get(&page_id).copied().ok_or("binding(s) omitted: missing page ownership reference")?;
    if kind(page) != Some("CANVAS") || page.get("internalOnly").and_then(Kv::bool) == Some(true) {
        return Err("binding(s) omitted: nested or internal timeline ownership unsupported");
    }
    if page.get("visible").and_then(Kv::bool) == Some(false) {
        return Err("binding(s) omitted: hidden page");
    }
    let root = parent(page).and_then(|id| nodes.get(&id).copied());
    if root.is_none_or(|node| kind(node) != Some("DOCUMENT")) {
        return Err("binding(s) omitted: cyclic or unclear document ownership");
    }
    if root.is_some_and(|node| node.get("visible").and_then(Kv::bool) == Some(false)) {
        return Err("binding(s) omitted: hidden document");
    }
    Ok((frame_id, page_id, base, size))
}

fn duration(frame: &Kv<'_>) -> Result<u32, Reason> {
    if frame.get("timelineOffset").is_some()
        || frame.get("timelineDisabled").is_some()
        || frame.get("timelineDefinitions").is_some()
        || frame.get("timelineAssignments").is_some()
    {
        return Err("timeline(s) omitted: offset/disabled/named/nested timeline unsupported");
    }
    if frame.get("playbackStyle").is_some() || frame.get("timelineKeyframeSnappingFps").is_some() || frame.get("quantizeTimelinePlayback").is_some() {
        return Err("timeline(s) omitted: explicit native playback/fps/quantization unsupported");
    }
    let us = match frame.get("timelineDuration") {
        Some(Kv::Uint(value)) => *value,
        _ => return Err("timeline(s) omitted: missing or noninteger duration"),
    };
    if us % 1000 != 0 {
        return Err("timeline(s) omitted: duration is not whole milliseconds");
    }
    let ms = us / 1000;
    if ms == 0 || ms > u64::from(TimelineSettings::MAX_DURATION_MS) {
        return Err("timeline(s) omitted: duration outside 1..600000 milliseconds");
    }
    u32::try_from(ms).map_err(|_| "timeline(s) omitted: duration overflow")
}

fn key_time(node: &Kv<'_>) -> Result<u32, Reason> {
    let us = match node.get("timelinePosition") {
        None => 0,
        Some(Kv::Int(value)) => *value,
        _ => return Err("binding(s) omitted: noninteger keyframe clock"),
    };
    if us < 0 {
        return Err("binding(s) omitted: negative keyframe clock");
    }
    if us % 1000 != 0 {
        return Err("binding(s) omitted: keyframe clock is not whole milliseconds");
    }
    if us > i64::from(TimelineSettings::MAX_DURATION_MS) * 1000 {
        return Err("binding(s) omitted: keyframe clock outside supported range");
    }
    u32::try_from(us / 1000).map_err(|_| "binding(s) omitted: keyframe clock overflow")
}

fn editable_resource(node: &Kv<'_>) -> Result<(), Reason> {
    if node.get("locked").is_some_and(|value| value.bool() != Some(false)) {
        return Err("binding(s) omitted: locked or unspecified resource editability unsupported");
    }
    if node.get("opacity").is_some_and(|value| float(value) != Some(1.0)) {
        return Err("binding(s) omitted: nondefault or nonfinite resource opacity unsupported");
    }
    Ok(())
}

fn key_vector(node: &Kv<'_>) -> Result<Vec2, Reason> {
    editable_resource(node)?;
    if !object_has_only(
        node,
        &[
            "guid",
            "phase",
            "parentIndex",
            "type",
            "name",
            "visible",
            "opacity",
            "editInfo",
            "locked",
            "timelinePosition",
            "keyframeValue",
            "easingData",
            "motionPathBezierHandles",
        ],
    ) {
        return Err("binding(s) omitted: explicit interpolation/operation/reference or unknown key metadata unsupported");
    }
    if node.get("visible").and_then(Kv::bool) != Some(true) {
        return Err("binding(s) omitted: hidden keyframes");
    }
    for name in
        ["interpolationType", "keyframeOperation", "timelinePositionType", "keyframeValueRef", "interpolationColorSpace", "interpolationHueMethod"]
    {
        if node.get(name).is_some() {
            return Err("binding(s) omitted: explicit interpolation/operation/reference unsupported");
        }
    }
    let value = node.get("keyframeValue").ok_or("binding(s) omitted: missing vector keyframe value")?;
    if !object_has_only(value, &["valueType", "value"])
        || value.get("valueType").and_then(Kv::str) != Some("VECTOR")
        || !value.get("value").is_some_and(|v| object_has_only(v, &["vectorValue"]))
    {
        return Err("binding(s) omitted: mismatched vector keyframe union tag");
    }
    value.get("value").and_then(|v| v.get("vectorValue")).and_then(vector).ok_or("binding(s) omitted: nonfinite or out-of-range vector value")
}

fn exact_curve(node: &Kv<'_>, kind: &str, expected: [f64; 4]) -> Result<(), Reason> {
    let easing = node.get("easingData").ok_or("binding(s) omitted: missing explicit supported easing")?;
    let curve = easing.get("easingValue").filter(|v| object_has_only(v, &["bezierEasing"])).and_then(|v| v.get("bezierEasing"));
    if !object_has_only(easing, &["easingType", "easingValue"])
        || easing.get("easingType").and_then(Kv::str) != Some(kind)
        || !curve.is_some_and(|v| {
            object_has_only(v, &["p1x", "p1y", "p2x", "p2y"])
                && ["p1x", "p1y", "p2x", "p2y"].into_iter().zip(expected).all(|(k, expected)| v.get(k).and_then(float) == Some(expected))
        })
    {
        return Err("binding(s) omitted: easing does not match the observed curve for this property");
    }
    Ok(())
}

fn linear_key(node: &Kv<'_>, base: Vec2) -> Result<Keyframe, Reason> {
    let offset = key_vector(node)?;
    exact_curve(node, "LINEAR", [0.0, 0.0, 1.0, 1.0])?;
    if !node.get("motionPathBezierHandles").is_some_and(|v| {
        object_has_only(v, &["p1x", "p1y", "p2x", "p2y"])
            && ["p1x", "p1y", "p2x", "p2y"].into_iter().all(|name| v.get(name).and_then(float) == Some(0.0))
    }) {
        return Err("binding(s) omitted: nonzero or missing spatial motion-path handles");
    }
    let position = base + offset;
    if !position.x.is_finite() || !position.y.is_finite() || position.x.abs() > MAX_COORD || position.y.abs() > MAX_COORD {
        return Err("binding(s) omitted: nonfinite or out-of-range resulting position");
    }
    Ok(Keyframe { time_ms: key_time(node)?, value: AnimValue::Vec2(position), easing: Ease::Linear })
}

fn scale_extent(base: Vec2, size: Vec2, factor: f64) -> bool {
    // Charge neither clipping nor a later model clamp as preservation of out-of-range geometry.
    // The one admitted cubic is monotone: bounded positive key extents bound each whole segment.
    [
        base.x,
        base.y,
        base.x + size.x,
        base.y + size.y,
        base.x + size.x * 0.5 * (1.0 - factor),
        base.x + size.x * 0.5 * (1.0 + factor),
        base.y + size.y * 0.5 * (1.0 - factor),
        base.y + size.y * 0.5 * (1.0 + factor),
    ]
    .into_iter()
    .all(|coordinate| coordinate.is_finite() && coordinate.abs() <= MAX_COORD)
}

fn uniform_scale_key(node: &Kv<'_>, base: Vec2, size: Vec2) -> Result<Keyframe, Reason> {
    let ratio = key_vector(node)?;
    if ratio.x <= 0.0 || ratio.x > MAX_SCALE || ratio.x != ratio.y {
        return Err("binding(s) omitted: scale must be positive uniform ratios at most 100");
    }
    if node.get("motionPathBezierHandles").is_some() {
        return Err("binding(s) omitted: explicit scale spatial handles unsupported");
    }
    exact_curve(node, "CUSTOM_CUBIC", [0.5, 0.0, 0.5, 1.0])?;
    if !scale_extent(base, size, ratio.x) {
        return Err("binding(s) omitted: scaled or static rectangle extent outside supported coordinate range");
    }
    Ok(Keyframe { time_ms: key_time(node)?, value: AnimValue::Vec2(ratio), easing: Ease::CustomBezier([0.5, 0.0, 0.5, 1.0]) })
}

fn centered_scale_target(target: &Kv<'_>, frame: &Kv<'_>) -> Result<(), Reason> {
    for node in [target, frame] {
        if node.get("transformOrigin").is_some() {
            return Err("binding(s) omitted: explicit scale transform origin unsupported");
        }
        if ["motionTransform", "transformModifiers", "linkMotionScaleAspectRatio"].into_iter().any(|name| node.get(name).is_some()) {
            return Err("binding(s) omitted: explicit scale motion transform or modifiers unsupported");
        }
        if node.get("locked").is_some_and(|value| value.bool() != Some(false)) {
            return Err("binding(s) omitted: locked scale target or frame unsupported");
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Binding {
    target: Guid,
    frame: Guid,
    page: Guid,
    track: Guid,
    base: Vec2,
    size: Vec2,
    property: MotionProperty,
}

fn scale_map_wrappers(binding: &Binding, nodes: &Nodes<'_>) -> Result<(), Reason> {
    for id in [binding.target, binding.frame, binding.page] {
        let node = nodes.get(&id).copied().ok_or("binding(s) omitted: missing native Scale context")?;
        if let Some(map) = node.get("parameterConsumptionMap")
            && (!object_has_only(map, &["entries"]) || !matches!(map.get("entries"), Some(Kv::Array(_))))
        {
            return Err("binding(s) omitted: unknown Scale map or ancestor metadata unsupported");
        }
    }
    Ok(())
}

#[derive(Default)]
struct Prepared<'a> {
    by_track: BTreeMap<Guid, Vec<&'a Kv<'a>>>,
    timelines: BTreeMap<Guid, Result<u32, Reason>>,
    owners: BTreeMap<Guid, BTreeSet<Guid>>,
    bindings: Vec<Binding>,
    target_counts: BTreeMap<Guid, usize>,
    target_markers: BTreeMap<Guid, usize>,
    total_bindings: usize,
    keys_over_budget: bool,
    scan: Scan,
}

fn discover_within(changes: &[Kv<'_>], layer_cap: usize, notes: &mut Notes) -> Option<Scan> {
    let mut scan = Scan::default();
    let mut hinted = changes.iter().any(motion_hint);
    if !hinted {
        for node in changes {
            hinted |= discover(node, &mut scan);
            if hinted || scan.limited {
                break;
            }
        }
    }
    if scan.limited {
        notes.add("native graph(s) omitted: animation expression inspection budget exceeded", 1);
        return None;
    }
    if !hinted {
        return None;
    }
    if changes.len() > layer_cap.min(MAX_GRAPH) {
        notes.add("native graph(s) omitted: Motion node graph budget exceeded", 1);
        return None;
    }
    Some(scan)
}

fn latest_nodes<'a>(changes: &'a [Kv<'a>], scan: &mut Scan, notes: &mut Notes) -> Nodes<'a> {
    let mut nodes = Nodes::new();
    for node in changes {
        let Some(id) = node.get("guid").and_then(Guid::read) else {
            if motion_hint(node) || native_fields(node).any(|(_, value)| scan.expressions(value) > 0) {
                notes.add("native node(s) omitted: malformed GUID or ownership reference", 1);
            }
            continue;
        };
        if node.get("phase").and_then(Kv::str) == Some("REMOVED") {
            nodes.remove(&id);
        } else {
            nodes.insert(id, node);
        }
    }
    nodes
}

impl<'a> Prepared<'a> {
    fn read(nodes: &Nodes<'a>, notes: &mut Notes) -> Self {
        let mut prepared = Self::default();
        for (&id, &node) in nodes {
            if prepared.scan.limited {
                break;
            }
            prepared.resources(node, notes);
            prepared.timeline(id, node, nodes, notes);
            prepared.node_bindings(id, node, nodes, notes);
        }
        prepared.inspect_scale_maps(nodes);
        prepared
    }

    fn resources(&mut self, node: &'a Kv<'a>, notes: &mut Notes) {
        if kind(node) == Some("KEYFRAME") {
            if let Some(track) = parent(node) {
                let children = self.by_track.entry(track).or_default();
                if children.len() >= MAX_KEYS {
                    self.keys_over_budget = true;
                } else {
                    children.push(node);
                }
            } else {
                notes.add("keyframe node(s) omitted: missing parent track reference", 1);
            }
        }
        if kind(node) == Some("ANIMATION_PRESET_INSTANCE") {
            notes.add("preset node(s) omitted: animation presets unsupported", 1);
        }
    }

    fn timeline(&mut self, id: Guid, node: &Kv<'_>, nodes: &Nodes<'a>, notes: &mut Notes) {
        if !has_timeline(node) {
            return;
        }
        let root_page = parent(node)
            .filter(|page| nodes.get(page).is_some_and(|n| kind(n) == Some("CANVAS") && n.get("internalOnly").and_then(Kv::bool) != Some(true)));
        if kind(node) != Some("FRAME") {
            notes.add("timeline(s) omitted: owner is not a top-level frame", 1);
            return;
        }
        self.timelines.insert(id, if root_page.is_some() { duration(node) } else { Err("timeline(s) omitted: nested or unclear frame ownership") });
        if let Some(page) = root_page {
            self.owners.entry(page).or_default().insert(id);
        }
    }

    fn inspect(&mut self, value: &Kv<'_>) -> (usize, usize) {
        self.inspect_at(value, 0)
    }

    fn inspect_at(&mut self, value: &Kv<'_>, depth: usize) -> (usize, usize) {
        let before = self.scan.reference_markers;
        let count = self.scan.expressions_at(value, depth);
        let markers = count.max(self.scan.reference_markers.saturating_sub(before));
        self.total_bindings = self.total_bindings.saturating_add(markers);
        if self.total_bindings > MAX_BINDINGS {
            self.scan.limited = true;
        }
        (count, markers)
    }

    fn scale_contexts(&self) -> Option<BTreeSet<Guid>> {
        let mut contexts = BTreeSet::new();
        for binding in self.bindings.iter().filter(|binding| matches!(binding.property, MotionProperty::Scale)) {
            for id in [binding.target, binding.frame, binding.page] {
                if !contexts.contains(&id) && contexts.len() >= MAX_SCALE_CONTEXTS {
                    return None;
                }
                contexts.insert(id);
            }
        }
        Some(contexts)
    }

    fn inspect_scale_maps(&mut self, nodes: &Nodes<'a>) {
        if self.scan.limited {
            return;
        }
        // At most three direct contexts per already-budgeted binding; deduplicate before scanning
        // so a shared frame/page is not recharged for every emitted Scale target.
        let Some(contexts) = self.scale_contexts() else {
            self.scan.limited = true;
            return;
        };
        for id in contexts {
            if self.scan.limited {
                break;
            }
            if let Some(map) = nodes.get(&id).and_then(|node| node.get("parameterConsumptionMap")) {
                self.inspect_scale_siblings(id, map);
            }
        }
    }

    fn inspect_scale_siblings(&mut self, id: Guid, map: &Kv<'_>) {
        let Kv::Object(fields) = map else { return };
        if !matches!(map.get("entries"), Some(Kv::Array(_))) {
            return; // Malformed maps were already inspected as a whole in node_bindings.
        }
        if self.scan.visits > MAX_VISITS.saturating_sub(2) {
            self.scan.limited = true;
            return;
        }
        self.scan.visits += 2; // The map object and its primary entries array.
        let mut primary_entries = false;
        for (name, value) in fields {
            if *name == "entries" && !primary_entries {
                primary_entries = true;
                continue; // Every primary entry was already inspected once in node_bindings.
            }
            // Includes duplicate entries and unknown siblings. Their values share the same global
            // visits, reference, depth and binding budget as all ordinary animation inspection.
            let (_, markers) = self.inspect_at(value, 1);
            if markers > 0 {
                let target = self.target_markers.entry(id).or_default();
                *target = target.saturating_add(markers);
            }
            if self.scan.limited {
                break;
            }
        }
    }

    fn other_field(&mut self, id: Guid, field: &str, value: &Kv<'_>, notes: &mut Notes) {
        let (count, markers) = self.inspect(value);
        if markers > 0 {
            let target = self.target_markers.entry(id).or_default();
            *target = target.saturating_add(markers);
        }
        let reason = match field {
            "fillPaints" | "strokePaints" => "paint binding expression(s) omitted: paint animation unsupported",
            "effects" => "effect binding expression(s) omitted: effect animation unsupported",
            _ => "binding expression(s) omitted: nested/instance/non-node animation unsupported",
        };
        notes.add(reason, count);
        if count == 0 {
            notes.add("animation reference marker(s) omitted: no supported KEYFRAME expression", markers);
        }
    }

    fn node_bindings(&mut self, id: Guid, node: &'a Kv<'a>, nodes: &Nodes<'a>, notes: &mut Notes) {
        for (field, value) in native_fields(node) {
            if field != "parameterConsumptionMap" {
                self.other_field(id, field, value, notes);
                continue;
            }
            if !matches!(value.get("entries"), Some(Kv::Array(_))) {
                let (count, markers) = self.inspect(value);
                if markers > 0 {
                    let target = self.target_markers.entry(id).or_default();
                    *target = target.saturating_add(markers);
                }
                notes.add("binding expression(s) omitted: malformed parameter map", count.max(markers));
                continue;
            }
            for entry in value.get("entries").map(Kv::list).unwrap_or_default() {
                let (expressions, markers) = self.inspect(entry);
                if markers == 0 {
                    continue;
                }
                if self.scan.limited {
                    break;
                }
                let count = self.target_markers.entry(id).or_default();
                *count = count.saturating_add(markers);
                self.parameter_entry(id, node, entry, expressions, nodes, notes);
            }
        }
    }

    fn parameter_entry(&mut self, id: Guid, node: &'a Kv<'a>, entry: &Kv<'_>, expressions: usize, nodes: &Nodes<'a>, notes: &mut Notes) {
        let property = match entry.get("variableField").and_then(Kv::str) {
            Some("MOTION_TRANSLATION_XY") => MotionProperty::Position,
            Some("MOTION_SCALE_XY") => MotionProperty::Scale,
            _ => {
                notes.add("binding(s) omitted: unsupported native property (only scoped XY Position and uniform Scale are retained)", 1);
                return;
            }
        };
        *self.target_counts.entry(id).or_default() += 1;
        if expressions == 0 {
            notes.add("binding(s) omitted: track reference without a KEYFRAME expression", 1);
            return;
        }
        if expressions != 1 {
            notes.add("binding(s) omitted: nested KEYFRAME expression unsupported", 1);
            return;
        }
        let parsed = if !object_has_only(entry, &["variableField", "variableData"]) {
            Err("binding(s) omitted: ambiguous or unsupported property field metadata")
        } else {
            entry.get("variableData").ok_or("binding(s) omitted: missing variable data").and_then(|data| manual_reference(data, property))
        }
        .and_then(|track| {
            let (frame, page, base, size) = owner(node, nodes)?;
            if matches!(property, MotionProperty::Scale) {
                centered_scale_target(node, nodes.get(&frame).copied().ok_or("binding(s) omitted: missing scale frame")?)?;
            }
            Ok(Binding { target: id, frame, page, track, base, size, property })
        });
        match parsed {
            Ok(binding) => {
                self.owners.entry(binding.page).or_default().insert(binding.frame);
                self.bindings.push(binding);
            }
            Err(reason) => notes.add(reason, 1),
        }
    }

    /// Charge source and fan-out counts before constructing any editable keyframe vectors.
    fn within_budgets(&self, nodes: &Nodes<'a>, notes: &mut Notes) -> bool {
        if self.scan.limited {
            notes.add("native graph(s) omitted: binding/reference/expression inspection budget exceeded", 1);
            return false;
        }
        if self.keys_over_budget {
            notes.add("native graph(s) omitted: per-track keyframe budget exceeded before building editable tracks", 1);
            return false;
        }
        for (track, keys) in &self.by_track {
            if nodes.get(track).is_none_or(|node| kind(node) != Some("KEYFRAME_TRACK")) {
                notes.add("keyframe node(s) omitted: dangling parent track reference", keys.len());
            }
        }
        let mut expanded = 0usize;
        for binding in &self.bindings {
            let count = self.by_track.get(&binding.track).map_or(0, Vec::len);
            if count > MAX_KEYS || expanded.checked_add(count).is_none_or(|sum| sum > MAX_TOTAL_KEYS) {
                notes.add("native graph(s) omitted: keyframe expansion budget exceeded before building editable tracks", 1);
                return false;
            }
            expanded += count;
        }
        true
    }

    fn track_keys(&self, binding: &Binding, duration_ms: u32, nodes: &Nodes<'a>) -> Result<Vec<Keyframe>, Reason> {
        let track = nodes
            .get(&binding.track)
            .copied()
            .filter(|n| kind(n) == Some("KEYFRAME_TRACK"))
            .ok_or("binding(s) omitted: missing or removed track reference")?;
        if track.get("visible").and_then(Kv::bool) != Some(true) {
            return Err("binding(s) omitted: hidden track");
        }
        editable_resource(track)?;
        if !object_has_only(track, &["guid", "phase", "parentIndex", "type", "name", "visible", "opacity", "editInfo", "locked"]) {
            return Err("binding(s) omitted: unsupported native track metadata");
        }
        let resource =
            parent(track).and_then(|id| nodes.get(&id).copied()).ok_or("binding(s) omitted: missing track resource ownership reference")?;
        if kind(resource) != Some("CANVAS") || resource.get("internalOnly").and_then(Kv::bool) != Some(true) {
            return Err("binding(s) omitted: track resource is not an internal canvas");
        }
        if parent(resource) != nodes.get(&binding.page).and_then(|node| parent(node)) {
            return Err("binding(s) omitted: cyclic or cross-document resource ownership");
        }
        let source_keys =
            self.by_track.get(&binding.track).filter(|keys| !keys.is_empty()).ok_or("binding(s) omitted: track has no keyframe children")?;
        let mut keys: Vec<Keyframe> = source_keys
            .iter()
            .map(|node| match binding.property {
                MotionProperty::Position => linear_key(node, binding.base),
                MotionProperty::Scale => uniform_scale_key(node, binding.base, binding.size),
            })
            .collect::<Result<_, _>>()?;
        keys.sort_by_key(|key| key.time_ms);
        if keys.windows(2).any(|pair| pair[0].time_ms == pair[1].time_ms) {
            return Err("binding(s) omitted: duplicate keyframe times");
        }
        if keys.first().map(|key| key.time_ms) != Some(0) || keys.last().map(|key| key.time_ms) != Some(duration_ms) {
            return Err("binding(s) omitted: keys do not span the exact native frame duration");
        }
        Ok(keys)
    }

    fn pending(&self, binding: &Binding, nodes: &Nodes<'a>) -> Result<Option<Pending>, Reason> {
        if matches!(binding.property, MotionProperty::Scale) {
            scale_map_wrappers(binding, nodes)?;
        }
        if self.target_counts.get(&binding.target).copied().unwrap_or(0) != 1 {
            return Err("binding(s) omitted: duplicate or multiple property binding");
        }
        if matches!(binding.property, MotionProperty::Scale)
            && (self.target_markers.get(&binding.target).copied().unwrap_or(0) != 1
                || self.target_markers.get(&binding.frame).copied().unwrap_or(0) != 0
                || self.target_markers.get(&binding.page).copied().unwrap_or(0) != 0)
        {
            return Err("binding(s) omitted: multiple property bindings or animated scale ancestor unsupported");
        }
        if self.owners.get(&binding.page).map_or(0, BTreeSet::len) != 1 {
            return Err("binding(s) omitted: ambiguous multiple frame timeline owners");
        }
        let duration_ms = match self.timelines.get(&binding.frame) {
            Some(Ok(duration_ms)) => *duration_ms,
            Some(Err(_)) => return Ok(None), // The timeline's precise reason is reported once below.
            None => return Err("binding(s) omitted: missing native frame timeline duration"),
        };
        let keys = self.track_keys(binding, duration_ms, nodes)?;
        Ok(Some(Pending {
            target: binding.target.rest(),
            owner: binding.frame.rest(),
            base: binding.base,
            size: binding.size,
            duration_ms,
            property: binding.property,
            keys,
        }))
    }

    fn finish(&self, nodes: &Nodes<'a>, retained: &BTreeSet<Guid>, notes: &mut Notes) {
        for (frame, timeline) in &self.timelines {
            match timeline {
                Err(reason) => notes.add(reason, 1),
                Ok(_) if !retained.contains(frame) => notes.add("timeline(s) omitted: no supported unambiguous direct Motion track", 1),
                Ok(_) => {}
            }
        }
        for (id, node) in nodes {
            if kind(node) == Some("KEYFRAME_TRACK") && !self.scan.references.contains(id) {
                notes.add("track resource(s) omitted: no supported explicit binding reference", 1);
            }
        }
    }
}

impl NativeMotion {
    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.notes.0.is_empty()
    }

    pub(crate) fn read<'a>(message: &'a Kv<'a>, layer_cap: usize) -> Self {
        let changes = message.get("nodeChanges").map(Kv::list).unwrap_or_default();
        let mut result = Self::default();
        let Some(mut scan) = discover_within(changes, layer_cap, &mut result.notes) else { return result };
        let nodes = latest_nodes(changes, &mut scan, &mut result.notes);
        let prepared = Prepared::read(&nodes, &mut result.notes);
        if !prepared.within_budgets(&nodes, &mut result.notes) {
            return result;
        }
        let mut retained = BTreeSet::new();
        for binding in &prepared.bindings {
            match prepared.pending(binding, &nodes) {
                Ok(Some(pending)) => {
                    retained.insert(binding.frame);
                    result.pending.push(pending);
                }
                Ok(None) => {}
                Err(reason) => result.notes.add(reason, 1),
            }
        }
        prepared.finish(&nodes, &retained, &mut result.notes);
        result
    }

    pub(crate) fn apply(mut self, doc: &mut Document, ids: &BTreeMap<String, NodeId>) -> Vec<String> {
        let mut pages = BTreeSet::new();
        for pending in self.pending {
            let pair = ids.get(&pending.target).copied().zip(ids.get(&pending.owner).copied());
            let verified = pair.and_then(|(target, owner)| {
                let node = doc.node(target)?;
                let frame = doc.node(owner)?;
                let page = frame.parent?;
                let linear = |node: &figcraft_doc::Node| {
                    let c = node.transform.as_coeffs();
                    c[0] == 1.0 && c[1] == 0.0 && c[2] == 0.0 && c[3] == 1.0
                };
                (matches!(node.kind, NodeKind::Rectangle)
                    && matches!(frame.kind, NodeKind::Frame)
                    && doc.node(page).is_some_and(|node| node.is_page())
                    && node.parent == Some(owner)
                    && node.visible
                    && frame.visible
                    && node.layout.is_none()
                    && frame.layout.is_none()
                    && linear(node)
                    && linear(frame)
                    && base_position(node) == pending.base
                    && node.width == pending.size.x
                    && node.height == pending.size.y
                    && node.motion.is_empty()
                    && !doc.timelines.contains_key(&page))
                .then_some((target, page))
            });
            let Some((target, page)) = verified else {
                self.notes.add("binding(s) omitted: imported GUID ownership/geometry could not be verified", 1);
                continue;
            };
            if let Some(node) = doc.node_mut(target) {
                node.motion.tracks.push(Track { property: pending.property.model(), keys: pending.keys, visible: true });
                pages.insert((page, pending.duration_ms));
            }
        }
        for (page, duration_ms) in pages {
            doc.timelines.insert(page, TimelineSettings { duration_ms, fps: 60, looping: false });
            self.notes.add("timeline(s) use 60 fps and once playback: absent native fps/playback defaults are unverified", 1);
        }
        self.notes.finish()
    }
}
