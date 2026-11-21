//! Shared pre-allocation accounting. Source census never builds a JSON/Kv copy.

use std::collections::BTreeSet;
use std::fmt;

use figcraft_doc::{Document, Node, NodeKind, TextData};
use serde::Serialize;
use serde::ser::{self, SerializeMap, SerializeSeq, SerializeStruct};

use super::{FormatError, MAX_DEPTH, MAX_NETWORK, MAX_NODES};

pub(super) const MAX_BYTES: usize = 256 * 1024 * 1024;
pub(super) const MAX_WORK: usize = 4_000_000;
const MAX_DOCUMENT_WORK: usize = 128_000_000;
// Serde traverses extra containers for enum payloads, blocks and argument arrays.
// The Kiwi encoder independently limits the resulting native message to its wire depth.
const MAX_SOURCE_DEPTH: usize = 128;
pub(super) const MAX_AUX: usize = 200_000;

/// Every compression/archive write checks its final position before growing the buffer.
/// Seeking never grows it, including ZIP's header rewrites.
pub(super) struct Buffer {
    data: Vec<u8>,
    position: usize,
    limit: usize,
}

impl Buffer {
    pub fn new(limit: usize) -> Self {
        Self { data: Vec::new(), position: 0, limit }
    }
    pub fn size(&self) -> usize {
        self.data.len()
    }
    pub fn into_inner(self) -> Vec<u8> {
        self.data
    }
}

impl std::io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let end = self
            .position
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| std::io::Error::other(".fig output byte budget exceeded before buffer growth"))?;
        if end > self.data.len() {
            self.data.try_reserve_exact(end - self.data.len()).map_err(|_| std::io::Error::other(".fig output allocation refused"))?;
            self.data.resize(end, 0);
        }
        self.data[self.position..end].copy_from_slice(bytes);
        self.position = end;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl std::io::Seek for Buffer {
    fn seek(&mut self, from: std::io::SeekFrom) -> std::io::Result<u64> {
        use std::io::SeekFrom;
        let next = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(n) => self.data.len() as i128 + i128::from(n),
            SeekFrom::Current(n) => self.position as i128 + i128::from(n),
        };
        let next =
            usize::try_from(next).ok().filter(|n| *n <= self.limit).ok_or_else(|| std::io::Error::other(".fig output seek exceeds byte budget"))?;
        self.position = next;
        Ok(next as u64)
    }
}

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub nodes: usize,
    pub bytes: usize,
    pub work: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { nodes: MAX_NODES, bytes: MAX_BYTES, work: MAX_WORK }
    }
}

pub(super) struct Budget {
    pub limits: Limits,
    bytes: usize,
    work: usize,
    retained: usize,
    layer_work_end: Option<usize>,
    rows: usize,
    failure: Option<&'static str>,
}

impl Default for Budget {
    fn default() -> Self {
        Self::new(Limits::default())
    }
}

impl Budget {
    pub fn new(limits: Limits) -> Self {
        Self { limits, bytes: 0, work: 0, retained: 0, layer_work_end: None, rows: 0, failure: None }
    }
    pub fn failed(&self) -> bool {
        self.failure.is_some()
    }
    pub fn remaining_work(&self) -> usize {
        self.layer_work_end.unwrap_or(self.limits.work).saturating_sub(self.work)
    }
    pub fn layer_start(&mut self) -> (usize, usize) {
        self.layer_work_end = Some(self.limits.work.min(self.work.saturating_add(MAX_WORK)));
        (self.bytes, self.retained)
    }
    /// Blobs and image lookup entries remain live after the current row is complete.
    pub fn keep(&mut self, bytes: usize) -> bool {
        if !self.reserve(bytes, 0) {
            return false;
        }
        self.retained += bytes;
        true
    }
    pub fn finish_layer(&mut self, (start, retained): (usize, usize), row: &super::Kv<'_>) -> bool {
        // Paths, comparison rows and source estimates are temporary. The owned Kiwi row
        // and any new shared cache entries survive until the final message is encoded.
        self.bytes = start + (self.retained - retained);
        self.layer_work_end = None;
        self.reserve(row_heap_bytes(row).saturating_add(2 * size_of::<super::Kv>()), 0)
    }
    pub fn fail(&mut self, reason: &'static str) {
        if self.failure.is_none() {
            self.failure = Some(reason);
        }
    }
    pub fn check(&self) -> Result<(), FormatError> {
        match self.failure {
            Some(reason) => Err(FormatError::TooLarge(format!(".fig export: {reason}"))),
            None => Ok(()),
        }
    }
    pub fn reserve(&mut self, bytes: usize, work: usize) -> bool {
        if self.failed() {
            return false;
        }
        let Some(bytes) = self.bytes.checked_add(bytes).filter(|n| *n <= self.limits.bytes) else {
            self.fail("generation byte budget exceeded before allocation");
            return false;
        };
        let Some(work) = self.work.checked_add(work).filter(|n| *n <= self.layer_work_end.unwrap_or(self.limits.work)) else {
            self.fail("generation work budget exceeded before allocation");
            return false;
        };
        self.bytes = bytes;
        self.work = work;
        true
    }
    pub fn items(&mut self, count: usize, bytes_each: usize, work_each: usize) -> bool {
        let costs = count.checked_mul(bytes_each).zip(count.checked_mul(work_each));
        match costs {
            Some((bytes, work)) => self.reserve(bytes, work),
            None => {
                self.fail("generation count overflow");
                false
            }
        }
    }
    pub fn source<T: Serialize + ?Sized>(&mut self, value: &T) -> bool {
        self.source_copies(value, 8, false)
    }
    pub fn text_source(&mut self, text: &TextData) -> bool {
        self.cached_text_source(text, text, 8)
    }
    fn cached_text_source<T: Serialize + ?Sized>(&mut self, value: &T, text: &TextData, copies: usize) -> bool {
        let cache = text.native_cache.as_deref();
        if cache.is_some_and(|c| !self.items(c.placement_count(), 0, 1)) {
            return false;
        }
        self.source_copies(value, copies, cache.is_some())
    }
    fn source_copies<T: Serialize + ?Sized>(&mut self, value: &T, copies: usize, native_layout_counted: bool) -> bool {
        if self.failed() {
            return false;
        }
        let mut census = Census {
            bytes: 0,
            work: 0,
            byte_limit: self.limits.bytes.saturating_sub(self.bytes) / copies,
            work_limit: self.remaining_work(),
            native_layout_counted,
        };
        if value.serialize(Counter { census: &mut census, depth: 0 }).is_err() {
            self.fail("source value/byte/depth budget exceeded before generation");
            return false;
        }
        self.items(census.bytes, copies, 0) && self.reserve(0, census.work)
    }
    /// Equality/search work can revisit an entire source value for each candidate. Measure
    /// the same source without copying it, and pay that repeated work before the search.
    pub fn comparisons<T: Serialize + ?Sized>(&mut self, value: &T, times: usize) -> bool {
        if self.failed() || times == 0 {
            return !self.failed();
        }
        let mut census =
            Census { bytes: 0, work: 0, byte_limit: self.limits.bytes, work_limit: self.remaining_work() / times, native_layout_counted: false };
        if value.serialize(Counter { census: &mut census, depth: 0 }).is_err() {
            self.fail("source comparison work budget exceeded before generation");
            return false;
        }
        self.items(census.work, 0, times)
    }
    pub fn row(&mut self) -> bool {
        self.rows(1)
    }
    pub fn rows(&mut self, count: usize) -> bool {
        self.rows = self.rows.saturating_add(count);
        if self.rows > MAX_NODES {
            self.fail("global inline-derived row budget exceeded");
            return false;
        }
        self.items(count, 1024, 64)
    }
    pub fn nodes(&mut self, count: usize) -> bool {
        if count > self.limits.nodes {
            self.fail("total node-change budget exceeded before allocation");
            return false;
        }
        true
    }
    pub fn tree(&mut self, doc: &Document, library: &super::library::Library) -> Result<(), FormatError> {
        // This pass inspects the borrowed source without creating copies. Charge that
        // traversal once, independently from the later allocations it guards.
        let mut source = Budget::new(self.limits);
        let scale_work = self.limits.work == MAX_WORK;
        let mut visited = BTreeSet::new();
        let mut work = Vec::new();
        if !self.items(doc.pages.len(), 24, 1) {
            return self.check();
        }
        work.extend(doc.pages.iter().rev().map(|id| (*id, None, 0usize)));
        work.extend(library.roots.iter().map(|id| (*id, doc.library_store(), 1)));
        let mut aux = doc.variables.collections.iter().map(|c| c.modes.len()).try_fold(0usize, |sum, n| sum.checked_add(n));
        if aux.is_none_or(|n| n > MAX_AUX) {
            self.fail("auxiliary identifier budget exceeded before source assignment");
            return self.check();
        }
        while let Some((id, expected_parent, depth)) = work.pop() {
            if depth > MAX_DEPTH {
                self.fail("source tree depth budget exceeded");
                break;
            }
            if !visited.insert(id) {
                self.fail("cyclic or duplicate source child reference");
                break;
            }
            if visited.len() > MAX_NODES {
                self.fail("source graph node budget exceeded");
                break;
            }
            if scale_work {
                // Large canvases repeat bounded work per layer. Only validated tree entries
                // contribute to the document allowance; one generated row keeps its own cap.
                source.limits.work = visited.len().saturating_mul(8192).clamp(MAX_WORK, MAX_DOCUMENT_WORK);
            }
            let Some(node) = doc.node(id) else {
                self.fail("dangling source child reference");
                break;
            };
            if expected_parent.is_some() && node.parent != expected_parent {
                self.fail("inconsistent source parent reference");
                break;
            }
            if !source.node_source_copies(node, 1) {
                return source.check();
            }
            let defs = match &node.kind {
                NodeKind::Component { props, .. } => props.len().checked_mul(2).and_then(|n| n.checked_add(node.name.split(',').count())),
                _ => Some(0),
            };
            let grid = node.layout.as_ref().map_or(Some(0), |l| (l.grid_columns.min(1000) as usize).checked_add(l.grid_rows.min(1000) as usize));
            aux = aux.zip(defs).zip(grid).and_then(|((a, b), c)| a.checked_add(b)?.checked_add(c));
            if aux.is_none_or(|n| n > MAX_AUX) {
                self.fail("auxiliary identifier/work budget exceeded before assignment");
                break;
            }
            if !self.items(node.children.len(), 24, 1) {
                break;
            }
            let hidden = library.nodes.contains(&id);
            work.extend(node.children.iter().rev().filter(|c| !hidden || library.nodes.contains(c)).map(|child| (*child, Some(id), depth + 1)));
        }
        self.limits.work = source.limits.work;
        if !self.source(&doc.styles) || !self.source(&doc.variables) || !self.assets_unique(doc) {
            return self.check();
        }
        if !self.assets_work(doc) {
            return self.check();
        }
        let set_ids: BTreeSet<_> = doc.variables.collections.iter().map(|c| c.id.as_str()).collect();
        let variables = doc.variables.variables.iter().filter(|v| set_ids.contains(v.collection.as_str())).count();
        let assets = doc.styles.len().saturating_add(doc.variables.collections.len()).saturating_add(variables);
        self.nodes(1usize.saturating_add(visited.len()).saturating_add(assets).saturating_add(usize::from(assets > 0 || !library.roots.is_empty())));
        self.check()
    }

    fn assets_work(&mut self, doc: &Document) -> bool {
        let lookups = doc.variables.variables.len().checked_mul(doc.variables.collections.len());
        if lookups.is_none_or(|n| !self.reserve(0, n)) {
            return false;
        }
        let rows = doc.variables.variables.iter().try_fold(0usize, |n, v| {
            let modes = doc.variables.collections.iter().find(|c| c.id == v.collection).map_or(0, |c| c.modes.len());
            n.checked_add(modes)
        });
        if rows.is_none_or(|n| !self.items(n, 4096, 64)) {
            return false;
        }
        // The existing resolver consults at most32 variables per colour calculation.
        let calculations = doc
            .variables
            .variables
            .iter()
            .flat_map(|v| v.values.values())
            .filter(|v| matches!(v, figcraft_doc::VariableValue::ColorAlpha { .. } | figcraft_doc::VariableValue::ColorMix { .. }))
            .count();
        self.comparisons(&doc.variables, calculations.saturating_mul(32))
    }

    fn assets_unique(&mut self, doc: &Document) -> bool {
        let count = doc.styles.len().saturating_add(doc.variables.collections.len()).saturating_add(doc.variables.variables.len());
        if !self.items(count, 64, 1) {
            return false;
        }
        let mut styles = BTreeSet::new();
        let mut sets = BTreeSet::new();
        let mut vars = BTreeSet::new();
        if doc.styles.iter().any(|style| !styles.insert(style.id.as_str()))
            || doc.variables.collections.iter().any(|set| !sets.insert(set.id.as_str()))
            || doc.variables.variables.iter().any(|var| !vars.insert(var.id.as_str()))
        {
            self.fail("duplicate source asset identifier before assignment");
            return false;
        }
        for set in &doc.variables.collections {
            if !self.items(set.modes.len(), 64, 1) {
                return false;
            }
            let mut modes = BTreeSet::new();
            if set.modes.iter().any(|mode| !modes.insert(mode.id.as_str())) {
                self.fail("duplicate source variable mode identifier before assignment");
                return false;
            }
        }
        true
    }

    /// Bound variable-width outline expansion before entering the pathops generator. Its
    /// sampler caps at MAX_SAMPLES and each round join/cap adds at most64 points. Refuse
    /// sampling overflow instead of reaching its unbounded uniform-stroke fallback.
    pub fn outline_path(&mut self, node: &Node) -> Option<figcraft_doc::geom::BezPath> {
        use figcraft_doc::geom::measure::{MAX_SAMPLES, polylines};
        use figcraft_doc::{StrokeAlign, StrokeCap, StrokeJoin};
        if !node.stroke.dashes.is_empty() {
            self.fail("dashed variable-width outline generation is outside the bounded subset");
            return None;
        }
        if matches!(&node.kind, NodeKind::Vector { network } if network.vertices.iter().any(|v| v.radius > 0.0)) {
            self.fail("rounded-vector variable-width outline expansion is outside the bounded subset");
            return None;
        }
        let input = match &node.kind {
            NodeKind::Polygon { count } => *count as usize,
            NodeKind::Star { count, .. } => (*count as usize).saturating_mul(2),
            NodeKind::Vector { network } => network.vertices.len().saturating_add(network.segments.len()),
            _ => 16,
        };
        if !self.items(input.saturating_mul(16).saturating_add(128), 128, 8) || !self.items(MAX_SAMPLES, 64, 2) {
            return None;
        }
        let path = figcraft_doc::geometry::stroke_geometry(node)?;
        let Some(lines) = polylines(&path) else {
            self.fail("outline flattening budget exceeded before expansion");
            return None;
        };
        // sum ceil(segment_length/(line_length/128)) <=128 + segment count;
        // extra2 covers floating-point rounding and each subpath's initial point.
        let samples = lines.iter().fold(0usize, |sum, line| sum.saturating_add(line.points.len().saturating_add(130)));
        if samples > MAX_SAMPLES {
            self.fail("outline resampling budget exceeded before expansion");
            return None;
        }
        let profile_points = node.stroke.profile().map_or(0, |p| p.points.len().min(figcraft_doc::WidthProfile::MAX_POINTS));
        if !self.items(samples, 0, profile_points.saturating_mul(4).saturating_add(8)) {
            return None;
        }
        let joins = if node.stroke.join == StrokeJoin::Round { 132 } else { 8 };
        let caps = if [node.stroke.cap_start, node.stroke.cap_end].into_iter().any(|cap| cap.shape() == StrokeCap::Round) { 132 } else { 32 };
        let elements = samples.saturating_mul(joins).saturating_add(lines.len().saturating_mul(caps)).saturating_add(128);
        if !self.items(elements, 128, 4) {
            return None;
        }
        if node.stroke.align != StrokeAlign::Center && figcraft_doc::geometry::is_closed(node) {
            let segments = elements.saturating_add(path.segments().count());
            let crossings = segments.saturating_mul(segments.saturating_sub(1)) / 2;
            // The sweep refuses beyond this cap; reserve its maximum possible split work.
            if !self.items(crossings.min(figcraft_pathops::MAX_CROSSINGS as usize), 64, 1) {
                return None;
            }
        }
        Some(path)
    }
    pub fn node_source(&mut self, node: &Node) -> bool {
        self.node_source_copies(node, 8)
    }
    fn node_source_copies(&mut self, node: &Node, copies: usize) -> bool {
        let geometry = match &node.kind {
            NodeKind::Polygon { count } => *count as usize,
            NodeKind::Star { count, .. } => (*count as usize).saturating_mul(2),
            NodeKind::Vector { network } => network
                .vertices
                .len()
                .saturating_add(network.segments.len())
                .saturating_add(network.regions.iter().flat_map(|r| &r.loops).fold(0usize, |n, v| n.saturating_add(v.len()))),
            _ => 0,
        };
        if geometry > MAX_NETWORK {
            self.fail("source geometry complexity budget exceeded before construction");
            return false;
        }
        match node.kind.text() {
            Some(text) => self.cached_text_source(node, text, copies),
            None => self.source_copies(node, copies, false),
        }
    }
    pub fn path_source(&mut self, node: &Node) -> bool {
        // The unsmoothed, square-corner box has exactly five path elements at any size.
        if (node.kind.is_frame_like() || matches!(node.kind, NodeKind::Rectangle | NodeKind::Section { .. } | NodeKind::Text(_)))
            && node.corners.radii.iter().all(|r| *r <= 0.0)
        {
            return self.items(5, 128, 8);
        }
        if matches!(node.kind, NodeKind::Line) {
            return self.items(2, 128, 8);
        }
        let input = match &node.kind {
            NodeKind::Polygon { count } => *count as usize,
            NodeKind::Star { count, .. } => (*count as usize).saturating_mul(2),
            NodeKind::Vector { network } => network
                .vertices
                .len()
                .saturating_add(network.segments.len())
                .saturating_add(network.regions.iter().flat_map(|r| &r.loops).fold(0usize, |n, l| n.saturating_add(l.len()))),
            _ => 16,
        };
        self.items(input.saturating_mul(16).saturating_add(128), 128, 8)
    }
}

fn row_heap_bytes(row: &super::Kv<'_>) -> usize {
    use super::Kv;
    match row {
        Kv::Str(s) => s.capacity(),
        Kv::Bytes(b) => b.capacity(),
        Kv::Array(a) => a.iter().fold(a.capacity().saturating_mul(size_of::<Kv>()), |n, x| n.saturating_add(row_heap_bytes(x))),
        Kv::Object(o) => o.iter().fold(o.capacity().saturating_mul(size_of::<(&str, Kv)>()), |n, (_, x)| n.saturating_add(row_heap_bytes(x))),
        _ => 0,
    }
}

#[derive(Debug)]
struct CountError;
impl fmt::Display for CountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("source census budget exceeded")
    }
}
impl std::error::Error for CountError {}
impl ser::Error for CountError {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Self
    }
}

struct Census {
    native_layout_counted: bool,
    bytes: usize,
    work: usize,
    byte_limit: usize,
    work_limit: usize,
}
impl Census {
    fn add(&mut self, bytes: usize, work: usize) -> Result<(), CountError> {
        self.bytes = self.bytes.checked_add(bytes).filter(|n| *n <= self.byte_limit).ok_or(CountError)?;
        self.work = self.work.checked_add(work).filter(|n| *n <= self.work_limit).ok_or(CountError)?;
        Ok(())
    }
}
struct Counter<'a> {
    census: &'a mut Census,
    depth: usize,
}
struct Compound<'a> {
    native_cache: bool,
    census: &'a mut Census,
    depth: usize,
}
impl<'a> Counter<'a> {
    fn atom(self, bytes: usize) -> Result<(), CountError> {
        if self.depth > MAX_SOURCE_DEPTH {
            return Err(CountError);
        }
        self.census.add(bytes, 1)
    }
    fn compound(self, len: Option<usize>) -> Result<Compound<'a>, CountError> {
        if self.depth > MAX_SOURCE_DEPTH || len.is_some_and(|n| n > self.census.work_limit.saturating_sub(self.census.work)) {
            return Err(CountError);
        }
        self.census.add(8, 1)?;
        Ok(Compound { census: self.census, depth: self.depth + 1, native_cache: false })
    }
}
impl Compound<'_> {
    fn value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CountError> {
        value.serialize(Counter { census: self.census, depth: self.depth })
    }
}
macro_rules! atoms {
    ($($name:ident($ty:ty)),* $(,)?) => { $(fn $name(self, _: $ty) -> Result<(), CountError> { self.atom(16) })* };
}
impl<'a> ser::Serializer for Counter<'a> {
    type Ok = ();
    type Error = CountError;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;
    atoms!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char)
    );
    fn serialize_str(self, value: &str) -> Result<(), CountError> {
        if self.depth > MAX_SOURCE_DEPTH {
            return Err(CountError);
        }
        self.census.add(value.len().checked_mul(6).and_then(|n| n.checked_add(8)).ok_or(CountError)?, value.len().div_ceil(64).saturating_add(1))
    }
    fn serialize_bytes(self, value: &[u8]) -> Result<(), CountError> {
        self.census.add(value.len(), 1)
    }
    fn collect_str<T: fmt::Display + ?Sized>(self, value: &T) -> Result<(), CountError> {
        struct Sink<'a>(&'a mut Census);
        impl fmt::Write for Sink<'_> {
            fn write_str(&mut self, value: &str) -> fmt::Result {
                self.0.add(value.len().saturating_mul(6), value.len().div_ceil(64).saturating_add(1)).map_err(|_| fmt::Error)
            }
        }
        if self.depth > MAX_SOURCE_DEPTH {
            return Err(CountError);
        }
        fmt::write(&mut Sink(self.census), format_args!("{value}")).map_err(|_| CountError)
    }
    fn serialize_none(self) -> Result<(), CountError> {
        self.atom(1)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), CountError> {
        value.serialize(Counter { census: self.census, depth: self.depth + 1 })
    }
    fn serialize_unit(self) -> Result<(), CountError> {
        self.atom(1)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), CountError> {
        self.atom(1)
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, name: &'static str) -> Result<(), CountError> {
        self.serialize_str(name)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _: &'static str, value: &T) -> Result<(), CountError> {
        self.serialize_some(value)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(self, _: &'static str, _: u32, name: &'static str, value: &T) -> Result<(), CountError> {
        self.census.add(name.len(), 1)?;
        self.serialize_some(value)
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, CountError> {
        self.compound(len)
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, CountError> {
        self.compound(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<Self::SerializeTupleStruct, CountError> {
        self.compound(Some(len))
    }
    fn serialize_tuple_variant(self, _: &'static str, _: u32, _: &'static str, len: usize) -> Result<Self::SerializeTupleVariant, CountError> {
        self.compound(Some(len))
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, CountError> {
        self.compound(len)
    }
    fn serialize_struct(self, name: &'static str, len: usize) -> Result<Self::SerializeStruct, CountError> {
        let mut compound = self.compound(Some(len))?;
        compound.native_cache = compound.census.native_layout_counted && name == "NativeTextCache";
        Ok(compound)
    }
    fn serialize_struct_variant(self, _: &'static str, _: u32, _: &'static str, len: usize) -> Result<Self::SerializeStructVariant, CountError> {
        self.compound(Some(len))
    }
}
impl SerializeSeq for Compound<'_> {
    type Ok = ();
    type Error = CountError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CountError> {
        self.value(value)
    }
    fn end(self) -> Result<(), CountError> {
        Ok(())
    }
}
impl ser::SerializeTuple for Compound<'_> {
    type Ok = ();
    type Error = CountError;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CountError> {
        self.value(value)
    }
    fn end(self) -> Result<(), CountError> {
        Ok(())
    }
}
impl ser::SerializeTupleStruct for Compound<'_> {
    type Ok = ();
    type Error = CountError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CountError> {
        self.value(value)
    }
    fn end(self) -> Result<(), CountError> {
        Ok(())
    }
}
impl ser::SerializeTupleVariant for Compound<'_> {
    type Ok = ();
    type Error = CountError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CountError> {
        self.value(value)
    }
    fn end(self) -> Result<(), CountError> {
        Ok(())
    }
}
impl SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = CountError;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), CountError> {
        self.value(key)
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), CountError> {
        self.value(value)
    }
    fn end(self) -> Result<(), CountError> {
        Ok(())
    }
}
impl SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = CountError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), CountError> {
        self.census.add(key.len(), 1)?;
        // Node copies share this immutable Arc. Native text emission accounts separately for
        // each placement, its wire fields and the actual globally deduplicated outline bytes.
        if self.native_cache && key == "layout" {
            return Ok(());
        }
        self.value(value)
    }
    fn end(self) -> Result<(), CountError> {
        Ok(())
    }
}
impl ser::SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = CountError;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), CountError> {
        self.census.add(key.len(), 1)?;
        self.value(value)
    }
    fn end(self) -> Result<(), CountError> {
        Ok(())
    }
}
