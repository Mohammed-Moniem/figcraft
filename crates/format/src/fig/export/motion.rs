//! Minimal observed native Position and uniform Scale graphs.
//! Inventory precedes copied keys and native values.

use std::collections::{BTreeMap, BTreeSet};

use figcraft_doc::anim::motion::{AnimProperty, AnimValue, Ease, Track, base_position};
use figcraft_doc::color::PaintKind;
use figcraft_doc::geom::Vec2;

use super::budget::Budget;
use super::{Document, Fields, Kv, LAYERS, MAX_DEPTH, MAX_NODES, Node, NodeId, NodeKind, arr, b, e, fl, guid, o, position, s, xy};

const SESSION: u32 = 3;
const MAX_TRACKS: usize = 10_000;
const MAX_KEYS: usize = 100_000;
const MAX_TRACK_KEYS: usize = 10_000;
const MAX_MOTION_BYTES: usize = 64 * 1024 * 1024;
const MAX_COORD: f64 = 1e9;
const ERROR_PX: f64 = 1.0 / 1024.0;

#[derive(Clone, Copy)]
enum Property {
    Position,
    Scale,
}

impl Property {
    fn field(self) -> &'static str {
        match self {
            Self::Position => "MOTION_TRANSLATION_XY",
            Self::Scale => "MOTION_SCALE_XY",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Position => "Position",
            Self::Scale => "Scale",
        }
    }

    fn fallback(self) -> f64 {
        match self {
            Self::Position => 0.0,
            Self::Scale => 1.0,
        }
    }

    fn key_label(self) -> &'static str {
        match self {
            Self::Position => "Position key",
            Self::Scale => "Scale key",
        }
    }

    fn curve(self) -> Kv<'static> {
        let (kind, curve) = match self {
            Self::Position => ("LINEAR", handles(0.0, 0.0, 1.0, 1.0)),
            Self::Scale => ("CUSTOM_CUBIC", handles(0.5, 0.0, 0.5, 1.0)),
        };
        o(vec![("easingType", e(kind)), ("easingValue", o(vec![("bezierEasing", curve)]))])
    }
}

struct Key {
    id: u32,
    micros: i64,
    x: f32,
    y: f32,
}

struct Planned {
    id: u32,
    keys: Vec<Key>,
    property: Property,
}

#[derive(Default)]
pub(super) struct Plan {
    tracks: Vec<Planned>,
    bindings: BTreeMap<NodeId, (u32, Property)>,
    durations: BTreeMap<NodeId, u64>,
    pub warnings: Vec<String>,
}

struct Candidate<'a> {
    target: &'a Node,
    track: &'a Track,
    owner: NodeId,
    page: NodeId,
    duration: u32,
    property: Property,
}

#[derive(Default)]
struct Inventory<'a> {
    candidates: Vec<Candidate<'a>>,
    owners: BTreeMap<NodeId, BTreeSet<NodeId>>,
    reasons: BTreeMap<&'static str, usize>,
    tracks: usize,
    keys: usize,
    clips: usize,
}

impl<'a> Inventory<'a> {
    fn read(doc: &'a Document, ids: &BTreeMap<NodeId, u32>, budget: &mut Budget) -> Self {
        let mut out = Self::default();
        for (index, node) in doc.nodes().enumerate() {
            if index >= MAX_NODES || !budget.reserve(0, 1) {
                budget.fail("complete source node inventory exceeds the bounded export graph");
                break;
            }
            if node.motion.is_empty() {
                continue;
            }
            if !ids.contains_key(&node.id) && !budget.node_source(node) {
                break;
            }
            out.clips = out.clips.saturating_add(node.motion.clips.len());
            if let Some((owner, page)) = owner_of(doc, node) {
                out.owners.entry(page).or_default().insert(owner);
            }
            let positions = node.motion.tracks.iter().filter(|t| t.property == AnimProperty::Position).count();
            for track in &node.motion.tracks {
                if budget.failed() {
                    break;
                }
                out.tracks = out.tracks.saturating_add(1);
                out.keys = out.keys.saturating_add(track.keys.len());
                match candidate(doc, ids, node, track, positions, budget) {
                    Ok(value) if budget.items(1, 128, 16) => out.candidates.push(value),
                    Ok(_) => break,
                    Err(reason) => *out.reasons.entry(reason).or_default() += 1,
                }
            }
        }
        out
    }

    fn unambiguous(&mut self) {
        let owners = &self.owners;
        let reasons = &mut self.reasons;
        self.candidates.retain(|c| {
            let keep = owners.get(&c.page).is_some_and(|set| set.len() == 1 && set.contains(&c.owner));
            if !keep {
                *reasons.entry("multiple animated frame owners on one page").or_default() += 1;
            }
            keep
        });
    }
}

impl Plan {
    pub fn prepare(doc: &Document, ids: &BTreeMap<NodeId, u32>, assets: usize, budget: &mut Budget) -> Self {
        if !budget.source(&doc.timelines) {
            return Self::default();
        }
        let mut inventory = Inventory::read(doc, ids, budget);
        inventory.unambiguous();
        inventory.candidates.sort_by_key(|c| ids.get(&c.target.id).copied());
        let count = inventory.candidates.len();
        let keys = inventory.candidates.iter().fold(0usize, |n, c| n.saturating_add(c.track.keys.len()));
        let bytes = count.checked_mul(2048).and_then(|n| keys.checked_mul(1024).and_then(|k| n.checked_add(k)));
        if count > MAX_TRACKS || keys > MAX_KEYS || bytes.is_none_or(|n| n > MAX_MOTION_BYTES) {
            budget.fail("native Motion track/key/byte preflight exceeds the export limit");
            return Self::default();
        }
        let total = 1usize
            .saturating_add(ids.len())
            .saturating_add(assets)
            .saturating_add(count)
            .saturating_add(keys)
            .saturating_add(usize::from(assets > 0 || count > 0));
        if !budget.nodes(total) || !budget.rows(count.saturating_add(keys)) || !budget.items(keys, 2048, 96) || !budget.items(count, 4096, 256) {
            return Self::default();
        }
        let mut out = Self::default();
        let mut next = 0u32;
        for c in &inventory.candidates {
            let Some(id) = next.checked_add(1) else {
                budget.fail("native Motion GUID range overflow");
                return Self::default();
            };
            next = id;
            let base = base_position(c.target);
            let mut values = Vec::with_capacity(c.track.keys.len());
            for key in &c.track.keys {
                let Some(local) = next.checked_add(1) else {
                    budget.fail("native Motion key GUID range overflow");
                    return Self::default();
                };
                next = local;
                let AnimValue::Vec2(value) = key.value else { continue };
                let vector = match c.property {
                    Property::Position => value - base,
                    Property::Scale => value,
                };
                values.push(Key { id: local, micros: i64::from(key.time_ms) * 1000, x: vector.x as f32, y: vector.y as f32 });
            }
            out.durations.insert(c.owner, u64::from(c.duration) * 1000);
            out.bindings.insert(c.target.id, (id, c.property));
            out.tracks.push(Planned { id, keys: values, property: c.property });
        }
        out.describe(doc, inventory, keys);
        out
    }

    fn describe(&mut self, doc: &Document, inventory: Inventory<'_>, keys: usize) {
        let retained = self.tracks.len();
        let positions = self.tracks.iter().filter(|track| matches!(track.property, Property::Position)).count();
        let scales = retained.saturating_sub(positions);
        let omitted = inventory.tracks.saturating_sub(retained);
        let omitted_keys = inventory.keys.saturating_sub(keys);
        let timelines = self.durations.len();
        let omitted_timelines = doc.timelines.len().saturating_sub(timelines);
        if omitted > 0 || inventory.clips > 0 || omitted_timelines > 0 {
            self.warnings.push(format!("Motion export retained {positions} Position tracks and {scales} Scale tracks ({keys} keyframes) and {timelines} frame durations; omitted {omitted} tracks ({omitted_keys} keyframes), {} clips and {omitted_timelines} timeline settings; save a .figcraft copy to preserve editable animation", inventory.clips));
        }
        for (reason, count) in inventory.reasons {
            self.warnings.push(format!("Motion omitted {count} tracks: {reason}"));
        }
        if inventory.clips > 0 {
            self.warnings.push(format!(
                "Motion omitted {} clips: native presets and clip composition, including hidden clips, are unsupported",
                inventory.clips
            ));
        }
        if omitted_timelines > 0 {
            self.warnings.push(format!("Motion omitted {omitted_timelines} timeline settings without one retained frame owner"));
        }
        if timelines > 0 {
            self.warnings.push(format!(
                "Motion retained {timelines} frame durations, {positions} Position tracks and {scales} Scale tracks; native fps and playback defaults are unverified, so 60fps/once settings are not encoded"
            ));
        }
    }

    pub fn resource_count(&self) -> usize {
        self.tracks.iter().fold(0usize, |n, track| n.saturating_add(1).saturating_add(track.keys.len()))
    }

    pub fn duration(&self, owner: NodeId) -> Option<u64> {
        self.durations.get(&owner).copied()
    }

    pub fn binding(&self, target: NodeId) -> Option<Kv<'static>> {
        let (track, property) = *self.bindings.get(&target)?;
        let constant = data("vectorValue", xy(property.fallback(), property.fallback()), "VECTOR", "VECTOR");
        let manual = o(vec![("keyframeTrackId", o(vec![("guid", guid(SESSION, track))]))]);
        let parameter = o(vec![("value", o(vec![("manual", manual)])), ("type", e("MANUAL"))]);
        let params = data(
            "keyframeTrackParameterValue",
            o(vec![("parameters", arr(vec![parameter]))]),
            "KEYFRAME_TRACK_PARAMETER_DATA",
            "KEYFRAME_TRACK_PARAMETER_DATA",
        );
        let expression = o(vec![("expressionFunction", e("KEYFRAME")), ("expressionArguments", arr(vec![constant, params]))]);
        Some(o(vec![("variableField", e(property.field())), ("variableData", data("expressionValue", expression, "EXPRESSION", "VECTOR"))]))
    }

    pub fn emit(&self, canvas: u32, index: usize, nodes: &mut Vec<Kv<'static>>) {
        for (order, track) in self.tracks.iter().enumerate() {
            nodes.push(o(head(track.id, LAYERS, canvas, index + order, "KEYFRAME_TRACK", track.property.label())));
            for (order, key) in track.keys.iter().enumerate() {
                let mut fields = head(key.id, SESSION, track.id, order, "KEYFRAME", track.property.key_label());
                fields.extend([
                    ("timelinePosition", Kv::Int(key.micros)),
                    (
                        "keyframeValue",
                        o(vec![("value", o(vec![("vectorValue", xy(f64::from(key.x), f64::from(key.y)))])), ("valueType", e("VECTOR"))]),
                    ),
                    ("easingData", track.property.curve()),
                ]);
                if matches!(track.property, Property::Position) {
                    fields.push(("motionPathBezierHandles", handles(0.0, 0.0, 0.0, 0.0)));
                }
                nodes.push(o(fields));
            }
        }
    }
}

fn data(field: &'static str, value: Kv<'static>, ty: &'static str, resolved: &'static str) -> Kv<'static> {
    o(vec![("value", o(vec![(field, value)])), ("dataType", e(ty)), ("resolvedDataType", e(resolved))])
}

fn head(id: u32, parent_session: u32, parent: u32, index: usize, ty: &'static str, name: &str) -> Fields {
    vec![
        ("guid", guid(SESSION, id)),
        ("phase", e("CREATED")),
        ("parentIndex", o(vec![("guid", guid(parent_session, parent)), ("position", s(&position(index)))])),
        ("type", e(ty)),
        ("name", s(name)),
        ("visible", b(true)),
    ]
}

fn handles(a: f64, b: f64, c: f64, d: f64) -> Kv<'static> {
    o(vec![("p1x", fl(a)), ("p1y", fl(b)), ("p2x", fl(c)), ("p2y", fl(d))])
}

fn owner_of(doc: &Document, node: &Node) -> Option<(NodeId, NodeId)> {
    let mut current = node;
    for _ in 0..MAX_DEPTH {
        let parent = doc.node(current.parent?)?;
        if matches!(parent.kind, NodeKind::Page { .. }) {
            return matches!(current.kind, NodeKind::Frame).then_some((current.id, parent.id));
        }
        current = parent;
    }
    None
}

fn identity(node: &Node) -> bool {
    let c = node.transform.as_coeffs();
    c[..4] == [1.0, 0.0, 0.0, 1.0] && c[4..].iter().all(|v| coordinate(*v)) && dimensions(node)
}

fn dimensions(node: &Node) -> bool {
    // The native float wire format flushes subnormal values to zero. Retained animation
    // must keep positive dimensions after encoding, not only after the Float32 cast.
    [node.width, node.height].into_iter().all(|value| value > 0.0 && (value as f32).is_normal() && coordinate(value))
}

fn coordinate(value: f64) -> bool {
    value.is_finite() && value.abs() <= MAX_COORD && (f64::from(value as f32) - value).abs() <= ERROR_PX
}

fn candidate<'a>(
    doc: &'a Document,
    ids: &BTreeMap<NodeId, u32>,
    node: &'a Node,
    track: &'a Track,
    positions: usize,
    budget: &mut Budget,
) -> Result<Candidate<'a>, &'static str> {
    let property = match track.property {
        AnimProperty::Position => Property::Position,
        AnimProperty::Scale => Property::Scale,
        _ => return Err("unsupported property; only scoped XY Position and uniform Scale are emitted"),
    };
    if !track.visible {
        return Err("hidden track");
    }
    if !ids.contains_key(&node.id) {
        return Err("target is outside the emitted layer graph");
    }
    if !matches!(node.kind, NodeKind::Rectangle) || !node.children.is_empty() || node.source.is_some() {
        return Err("target is not a direct owned rectangle");
    }
    if matches!(property, Property::Position) && positions != 1 {
        return Err("duplicate Position tracks");
    }
    let frame = node.parent.and_then(|id| doc.node(id)).ok_or("missing direct frame owner")?;
    let page = frame.parent.and_then(|id| doc.node(id)).ok_or("missing owning page")?;
    if !matches!(frame.kind, NodeKind::Frame)
        || !matches!(page.kind, NodeKind::Page { .. })
        || !ids.contains_key(&frame.id)
        || !ids.contains_key(&page.id)
    {
        return Err("owner must be one direct top-level ordinary frame");
    }
    if ![node, frame, page].iter().all(|n| n.visible && !n.locked && n.opacity == 1.0) {
        return Err("hidden, locked or translucent target/owner");
    }
    if !dimensions(node) || !dimensions(frame) {
        return Err("nonpositive, out-of-range or Float32 dimension precision");
    }
    if !identity(node) || !identity(frame) || node.layout.is_some() || frame.layout.is_some() || page.layout.is_some() {
        return Err("nonidentity transform, precision or auto layout");
    }
    if !frame.motion.is_empty() || !page.motion.is_empty() || !node.motion.clips.is_empty() {
        return Err("animated ancestor or target clips require unsupported composition");
    }
    if node.stroke.profile().is_some_and(super::common::one_sided) {
        return Err("target is converted to an outlined stroke");
    }
    let timeline = doc.timelines.get(&page.id).ok_or("missing declared page timeline")?;
    if timeline.duration_ms == 0 || timeline.duration_ms > 600_000 {
        return Err("timeline clock exceeds whole-ms native subset");
    }
    if timeline.fps != 60 || timeline.looping {
        return Err("unsupported fps/playback settings");
    }
    match property {
        Property::Position => validate_keys(track, base_position(node), timeline.duration_ms)?,
        Property::Scale => {
            scale_target(node, frame, page)?;
            validate_scale_keys(track, node, frame, timeline.duration_ms, budget)?;
        }
    }
    Ok(Candidate { target: node, track, owner: frame.id, page: page.id, duration: timeline.duration_ms, property })
}

fn scale_target(node: &Node, frame: &Node, page: &Node) -> Result<(), &'static str> {
    if node.motion.tracks.len() != 1 {
        return Err("scale requires one track without mixed property composition");
    }
    if page.transform.as_coeffs() != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] {
        return Err("scale requires an identity page transform");
    }
    if [node, frame].into_iter().any(|n| {
        !n.bound.is_empty() || !n.prop_refs.is_empty() || n.variable_scale.is_some() || !n.scale_derived_overrides.is_empty() || n.source.is_some()
    }) {
        return Err("scale alias, property reference, K or source-owned composition unsupported");
    }
    let solid = matches!(node.fills.as_slice(), [paint] if paint.visible && matches!(paint.kind, PaintKind::Solid { .. }));
    if !solid
        || !node.strokes.is_empty()
        || !node.effects.is_empty()
        || !node.corners.radii.into_iter().all(|radius| radius == 0.0)
        || node.corners.smoothing != 0.0
        || node.is_mask
    {
        return Err("unsupported scale decoration; requires one plain solid nonrounded rectangle without stroke or effects");
    }
    Ok(())
}

fn native_float(value: f64) -> f64 {
    let value = value as f32;
    if value.is_subnormal() { 0.0 } else { f64::from(value) }
}

fn world_corner(source: f64, decoded: f64) -> Result<(), &'static str> {
    if !source.is_finite() || !decoded.is_finite() || source.abs() > MAX_COORD || decoded.abs() > MAX_COORD {
        return Err("scale world corner outside finite coordinate range");
    }
    if (source - decoded).abs() > ERROR_PX {
        return Err("scale Float32 world-corner reconstruction precision");
    }
    Ok(())
}

fn frame_clip_edges(frame: &Node) -> Result<(), &'static str> {
    let transform = frame.transform.as_coeffs();
    for (base, size) in [(transform[4], frame.width), (transform[5], frame.height)] {
        world_corner(base, native_float(base))?;
        world_corner(base + size, native_float(base) + native_float(size))?;
    }
    Ok(())
}

fn scale_world_corners(node: &Node, frame: &Node, ratio: Vec2) -> Result<(), &'static str> {
    let parent = frame.transform.as_coeffs();
    let local = node.transform.as_coeffs();
    for (base, translation, size, ratio) in [(parent[4], local[4], node.width, ratio.x), (parent[5], local[5], node.height, ratio.y)] {
        let center = base + translation + size * 0.5;
        let decoded_center = native_float(base) + native_float(translation) + native_float(size) * 0.5;
        for direction in [-1.0, 1.0] {
            world_corner(center + direction * size * ratio * 0.5, decoded_center + direction * native_float(size) * native_float(ratio) * 0.5)?;
        }
    }
    Ok(())
}

fn validate_scale_keys(track: &Track, node: &Node, frame: &Node, duration: u32, budget: &mut Budget) -> Result<(), &'static str> {
    if track.keys.len() < 2 || track.keys.len() > MAX_TRACK_KEYS {
        return Err("key count outside 2..10000 subset");
    }
    if track.keys.first().is_none_or(|key| key.time_ms != 0) || track.keys.last().is_none_or(|key| key.time_ms != duration) {
        return Err("key span must exactly match the declared timeline duration");
    }
    // Per-key corner arithmetic is precharged to the existing shared work cap. No separate
    // allowance is reset per Scale track, and the subsequent native-value precharges stay intact.
    if !budget.items(track.keys.len(), 0, 128) {
        return Err("scale world-corner inspection work budget exceeded");
    }
    frame_clip_edges(frame)?;
    scale_world_corners(node, frame, Vec2::new(1.0, 1.0))?;
    let mut previous = None;
    for key in &track.keys {
        if previous.is_some_and(|time| key.time_ms <= time) {
            return Err("unsorted or duplicate key times");
        }
        previous = Some(key.time_ms);
        if key.easing != Ease::CustomBezier([0.5, 0.0, 0.5, 1.0]) {
            return Err("unsupported scale easing; requires the observed uniform cubic");
        }
        let AnimValue::Vec2(ratio) = key.value else {
            return Err("scale key is not a Vec2");
        };
        if !ratio.x.is_finite() || !ratio.y.is_finite() || ratio.x <= 0.0 || ratio.x > 100.0 || ratio.y <= 0.0 || ratio.y > 100.0 {
            return Err("nonfinite or out-of-range scale ratio; requires 0 < scale <= 100");
        }
        if ratio.x != ratio.y {
            return Err("scale ratio must be uniform");
        }
        if !(ratio.x as f32).is_normal() || !(ratio.y as f32).is_normal() {
            return Err("scale ratio is a Float32 subnormal or underflows the native wire");
        }
        // At the same curve progress, the exact admitted monotone cubic keeps ratios and their
        // serialization errors within endpoint bounds; no overshoot curve is substituted.
        scale_world_corners(node, frame, ratio)?;
    }
    Ok(())
}

fn validate_keys(track: &Track, base: Vec2, duration: u32) -> Result<(), &'static str> {
    if track.keys.len() < 2 || track.keys.len() > MAX_TRACK_KEYS {
        return Err("key count outside 2..10000 subset");
    }
    if track.keys.first().is_none_or(|k| k.time_ms != 0) || track.keys.last().is_none_or(|k| k.time_ms != duration) {
        return Err("key span must exactly match the declared timeline duration");
    }
    let mut previous = None;
    for key in &track.keys {
        if previous.is_some_and(|time| key.time_ms <= time) {
            return Err("unsorted or duplicate key times");
        }
        previous = Some(key.time_ms);
        if key.easing != Ease::Linear {
            return Err("unsupported nonlinear or Hold easing");
        }
        let AnimValue::Vec2(value) = key.value else {
            return Err("Position key is not a Vec2");
        };
        for (value, base) in [(value.x, base.x), (value.y, base.y)] {
            let relative = value - base;
            if !coordinate(value)
                || !coordinate(base)
                || !coordinate(relative)
                || (f64::from(base as f32) + f64::from(relative as f32) - value).abs() > ERROR_PX
            {
                return Err("nonfinite, out-of-range or Float32 reconstruction precision");
            }
        }
    }
    Ok(())
}
