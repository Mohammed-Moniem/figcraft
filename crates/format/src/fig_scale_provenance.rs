//! Historical .fig length-supplier observations. These bytes are descriptive plugin data,
//! never authority for variable resolution, instance rebasing, or native scale installation.

use figcraft_doc::Node;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub(crate) const KEY: &str = "figcraft.import.fig.scale-provenance.v1";
pub(crate) const PRIVATE_FIELD: &str = "scaleProvenance";
pub(crate) const MAX_RECORD_BYTES: usize = 32 * 1024;
pub(crate) const MAX_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const MAX_FIELDS: usize = 64;
const MAX_REST_ID_BYTES: usize = 44 * (256 + 2);

pub(crate) const FIELDS: &[&str] = &[
    "size",
    "transform.translation",
    "cornerRadius",
    "rectangleTopLeftCornerRadius",
    "rectangleTopRightCornerRadius",
    "rectangleBottomLeftCornerRadius",
    "rectangleBottomRightCornerRadius",
    "strokeWeight",
    "borderTopWeight",
    "borderRightWeight",
    "borderBottomWeight",
    "borderLeftWeight",
    "dashPattern",
    "stackSpacing",
    "stackCounterSpacing",
    "stackHorizontalPadding",
    "stackVerticalPadding",
    "stackPaddingRight",
    "stackPaddingBottom",
    "gridColumnGap",
    "gridRowGap",
    "gridColumns",
    "gridRows",
    "minSize",
    "maxSize",
    "effects",
    "layoutGrids",
    "fontSize",
    "paragraphSpacing",
    "paragraphIndent",
    "listSpacing",
    "letterSpacing",
    "lineHeight",
    "textData.characterStyles",
    "vectorData.pointStyles",
    "parameterConsumptionMap",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Authority {
    DescriptiveOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RowKind {
    Wire,
    Override,
    Derived,
    Stale,
    SharedStyle,
    Auxiliary,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReadScope {
    Layer,
    StyledText,
    EffectStyle,
    GridStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Units {
    Length,
    Pixels,
    Percent,
    Mixed,
    NotDimensional,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Resolution {
    RawWinner,
    ComputedFit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Supplier {
    pub kind: RowKind,
    pub view_index: u32,
    pub scale: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordinal: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_depth: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_level: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Field {
    pub name: String,
    pub scope: ReadScope,
    pub units: Units,
    pub resolution: Resolution,
    pub supplier: Supplier,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_by: Option<Supplier>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RelativeFactor {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire: Option<f64>,
    pub effective: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplier: Option<Supplier>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Record {
    pub version: u8,
    pub authority: Authority,
    pub origin_guid: String,
    pub rest_id: String,
    pub enclosing_cumulative: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_relative: Option<RelativeFactor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children_cumulative: Option<f64>,
    pub child_clamp_applied: bool,
    pub fitted_geometry: bool,
    pub fields: Vec<Field>,
}

fn guid(value: &str) -> bool {
    let Some((a, b)) = value.split_once(':') else { return false };
    value.len() <= 41 && [a, b].into_iter().all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()) && part.parse::<u64>().is_ok())
}

fn factor(value: f64) -> bool {
    value.is_finite() && (1e-3..=1e3).contains(&value)
}

impl Supplier {
    fn valid(&self) -> bool {
        factor(self.scale)
            && self.row_guid.as_deref().is_none_or(guid)
            && self.owner_guid.as_deref().is_none_or(guid)
            && self.source_depth.is_none_or(|depth| depth <= 256)
            && self.override_level.is_none_or(|level| level <= 256)
    }
}

impl Record {
    pub(crate) fn valid(&self) -> bool {
        let rest = self.rest_id.strip_prefix('I').unwrap_or(&self.rest_id);
        let last = rest.rsplit(';').next();
        let mut names = BTreeSet::new();
        self.version == 1
            && guid(&self.origin_guid)
            && self.rest_id.len() <= MAX_REST_ID_BYTES
            && rest.split(';').all(guid)
            && last == Some(self.origin_guid.as_str())
            && factor(self.enclosing_cumulative)
            && self.children_cumulative.is_none_or(factor)
            && self.local_relative.as_ref().is_none_or(|local| {
                factor(local.effective) && local.wire.is_none_or(f64::is_finite) && local.supplier.as_ref().is_none_or(Supplier::valid)
            })
            && (self.local_relative.is_some() == self.children_cumulative.is_some())
            && self.fields.len() <= MAX_FIELDS
            && self.fields.iter().all(|field| {
                FIELDS.contains(&field.name.as_str())
                    && names.insert(field.name.as_str())
                    && field.supplier.valid()
                    && field.style_guid.as_deref().is_none_or(guid)
                    && field.linked_by.as_ref().is_none_or(Supplier::valid)
            })
    }
}

// Test-only stage observations: exercise the real optional work boundaries without an
// unsafe/global allocator or replacing the importer. Each test thread owns its counters.
#[cfg(test)]
std::thread_local! {
    static TEST_WORK: std::cell::Cell<[usize; 4]> = const { std::cell::Cell::new([0; 4]) };
}

#[cfg(test)]
pub(crate) fn test_reset_work() {
    TEST_WORK.with(|work| work.set([0; 4]));
}

#[cfg(test)]
pub(crate) fn test_work() -> [usize; 4] {
    TEST_WORK.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(crate) fn test_note_capture() {
    TEST_WORK.with(|work| {
        let mut counts = work.get();
        counts[0] += 1;
        work.set(counts);
    });
}

#[cfg(test)]
fn test_note_serialize() {
    TEST_WORK.with(|work| {
        let mut counts = work.get();
        counts[1] += 1;
        work.set(counts);
    });
}

#[cfg(test)]
fn test_note_parse() {
    TEST_WORK.with(|work| {
        let mut counts = work.get();
        counts[2] += 1;
        work.set(counts);
    });
}

#[cfg(test)]
fn test_note_encoded_bytes(bytes: usize) {
    TEST_WORK.with(|work| {
        let mut counts = work.get();
        counts[3] = counts[3].max(bytes);
        work.set(counts);
    });
}

/// Reserve the existing maximum record allowance before constructing optional metadata.
/// At most one record allowance is left unused at the end of an import; this prevents
/// repeated capture/serialization work once no complete bounded record can be admitted.
pub(crate) fn can_capture(used: usize) -> bool {
    used.checked_add(MAX_RECORD_BYTES).is_some_and(|total| total <= MAX_TOTAL_BYTES)
}

fn can_store(used: usize, bytes: usize) -> bool {
    used.checked_add(bytes).is_some_and(|total| total <= MAX_TOTAL_BYTES)
}

/// Serialization writes directly into an existing bounded buffer, rather than allocating a
/// complete oversized String and checking its size afterward.
struct RecordWriter {
    bytes: Vec<u8>,
}

impl std::io::Write for RecordWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().checked_add(bytes.len()).is_none_or(|total| total > MAX_RECORD_BYTES) {
            return Err(std::io::Error::other("optional .fig provenance record exceeds its byte limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn bounded_json(record: &Record) -> Result<String, ()> {
    let mut writer = RecordWriter { bytes: Vec::with_capacity(MAX_RECORD_BYTES) };
    let result = serde_json::to_writer(&mut writer, record);
    #[cfg(test)]
    test_note_encoded_bytes(writer.bytes.len());
    result.map_err(|_| ())?;
    // Retained payloads must not each keep the complete maximum-record reservation alive.
    String::from_utf8(writer.bytes.into_boxed_slice().into_vec()).map_err(|_| ())
}

fn reserve(used: &mut usize, bytes: usize) -> Result<(), ()> {
    let next = used.checked_add(bytes).filter(|total| *total <= MAX_TOTAL_BYTES).ok_or(())?;
    *used = next;
    Ok(())
}

pub(crate) fn encode(record: &Record, used: &mut usize) -> Result<String, ()> {
    if !can_capture(*used) || !record.valid() {
        return Err(());
    }
    #[cfg(test)]
    test_note_serialize();
    let payload = bounded_json(record)?;
    reserve(used, payload.len())?;
    Ok(payload)
}

/// Called only after the private .fig path and primary REST identity have been established.
/// Keep the original serialized observation byte-for-byte; never replace existing plugin data.
pub(crate) fn preserve(node: &mut Node, payload: &str, origin: &str, rest_id: &str, used: &mut usize) -> Result<(), ()> {
    if payload.len() > MAX_RECORD_BYTES || node.plugin_data.contains_key(KEY) || !can_store(*used, payload.len()) {
        return Err(());
    }
    #[cfg(test)]
    test_note_parse();
    let record: Record = serde_json::from_str(payload).map_err(|_| ())?;
    if !record.valid() || record.origin_guid != origin || record.rest_id != rest_id {
        return Err(());
    }
    reserve(used, payload.len())?;
    node.plugin_data.insert(KEY.into(), payload.into());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use figcraft_doc::{NodeId, NodeKind};

    fn record() -> Record {
        Record {
            version: 1,
            authority: Authority::DescriptiveOnly,
            origin_guid: "1:9".into(),
            rest_id: "1:9".into(),
            enclosing_cumulative: 0.5,
            local_relative: None,
            children_cumulative: None,
            child_clamp_applied: false,
            fitted_geometry: false,
            fields: Vec::new(),
        }
    }

    fn node() -> Node {
        Node::new(NodeId(9), NodeKind::Frame, "Observation only", 10.0, 20.0)
    }

    #[test]
    fn valid_payload_is_kept_byte_for_byte_and_preserves_unrelated_plugin_data() {
        let mut encoded = 0;
        let payload = encode(&record(), &mut encoded).unwrap();
        let mut node = node();
        node.plugin_data.insert("existing".into(), "unchanged".into());
        let mut stored = 0;
        preserve(&mut node, &payload, "1:9", "1:9", &mut stored).unwrap();
        assert_eq!(node.plugin_data[KEY], payload);
        assert_eq!(node.plugin_data["existing"], "unchanged");
        assert_eq!((node.width, node.height), (10.0, 20.0));
        assert_eq!((encoded, stored), (payload.len(), payload.len()));
    }

    #[test]
    fn unknown_schema_authority_invalid_identity_and_oversized_payload_fail_without_refund_or_writes() {
        let mut used = 0;
        let payload = encode(&record(), &mut used).unwrap();
        let mut wrong_schema: serde_json::Value = serde_json::from_str(&payload).unwrap();
        wrong_schema["installNativeContext"] = serde_json::json!(true);
        let cases = [
            payload.replace("descriptive-only", "native-authority"),
            serde_json::to_string(&wrong_schema).unwrap(),
            payload.replace("1:9", "../not-an-id"),
            format!("{}{}", " ".repeat(MAX_RECORD_BYTES), payload),
        ];
        for payload in cases {
            let mut node = node();
            let before = node.clone();
            let mut used = 0;
            assert!(preserve(&mut node, &payload, "1:9", "1:9", &mut used).is_err());
            assert_eq!(node, before);
            assert_eq!(used, 0);
        }
    }

    #[test]
    fn full_budget_overflow_and_existing_key_refuse_before_changing_node_or_budget() {
        let payload = encode(&record(), &mut 0).unwrap();
        for initial in [MAX_TOTAL_BYTES, usize::MAX] {
            let mut used = initial;
            let mut node = node();
            assert!(preserve(&mut node, &payload, "1:9", "1:9", &mut used).is_err());
            assert_eq!(used, initial);
            assert!(node.plugin_data.is_empty());
        }
        let mut node = node();
        node.plugin_data.insert(KEY.into(), "existing-user-data".into());
        assert!(preserve(&mut node, &payload, "1:9", "1:9", &mut 0).is_err());
        assert_eq!(node.plugin_data[KEY], "existing-user-data");
    }

    #[test]
    fn duplicate_fields_and_invalid_scales_are_rejected() {
        let mut value = record();
        let supplier = Supplier {
            kind: RowKind::Wire,
            view_index: 0,
            scale: 0.5,
            row_guid: Some("1:6".into()),
            owner_guid: None,
            ordinal: None,
            source_depth: None,
            override_level: None,
        };
        let field = Field {
            name: "strokeWeight".into(),
            scope: ReadScope::Layer,
            units: Units::Length,
            resolution: Resolution::RawWinner,
            supplier,
            style_guid: None,
            linked_by: None,
        };
        value.fields = vec![field.clone(), field];
        assert!(encode(&value, &mut 0).is_err());
        value.fields.pop();
        value.fields[0].supplier.scale = 0.0;
        assert!(encode(&value, &mut 0).is_err());
        value.fields[0].supplier.scale = f64::INFINITY;
        assert!(encode(&value, &mut 0).is_err());
    }

    #[test]
    fn provenance_budget_encode_declines_before_serialization_when_a_record_cannot_be_reserved() {
        for initial in [MAX_TOTAL_BYTES, usize::MAX, MAX_TOTAL_BYTES - MAX_RECORD_BYTES + 1] {
            let mut used = initial;
            test_reset_work();
            assert!(encode(&record(), &mut used).is_err());
            assert_eq!(used, initial);
            assert_eq!(test_work(), [0; 4], "no optional serialization after record reservation fails");
        }
        let mut used = MAX_TOTAL_BYTES - MAX_RECORD_BYTES;
        test_reset_work();
        let payload = encode(&record(), &mut used).unwrap();
        assert_eq!(test_work()[1], 1, "an exactly available maximum-record reservation still permits a small record");
        assert_eq!(used, MAX_TOTAL_BYTES - MAX_RECORD_BYTES + payload.len());
    }

    #[test]
    fn provenance_budget_preserve_declines_before_json_parse_when_known_payload_bytes_do_not_fit() {
        let payload = encode(&record(), &mut 0).unwrap();
        for initial in [MAX_TOTAL_BYTES, usize::MAX, MAX_TOTAL_BYTES - payload.len() + 1] {
            let mut used = initial;
            let mut node = node();
            let before = node.clone();
            test_reset_work();
            assert!(preserve(&mut node, &payload, "1:9", "1:9", &mut used).is_err());
            assert_eq!(node, before);
            assert_eq!(used, initial);
            assert_eq!(test_work(), [0; 4], "the known-size preflight runs before deserialization");
        }
        let mut used = MAX_TOTAL_BYTES - payload.len();
        let mut node = node();
        test_reset_work();
        preserve(&mut node, &payload, "1:9", "1:9", &mut used).unwrap();
        assert_eq!(used, MAX_TOTAL_BYTES);
        assert_eq!(test_work()[2], 1, "a fitting exact-sized payload is still parsed and retained");
    }

    #[test]
    fn provenance_record_serialization_stops_writing_at_the_existing_record_limit() {
        let id = "18446744073709551615:18446744073709551615";
        let supplier = Supplier {
            kind: RowKind::Override,
            view_index: u32::MAX,
            scale: 999.9999999999999,
            row_guid: Some(id.into()),
            owner_guid: Some(id.into()),
            ordinal: Some(u32::MAX),
            source_depth: Some(256),
            override_level: Some(256),
        };
        let mut value = record();
        value.origin_guid = id.into();
        value.rest_id = format!("I{}", vec![id; 258].join(";"));
        value.fields = FIELDS
            .iter()
            .map(|name| Field {
                name: (*name).into(),
                scope: ReadScope::StyledText,
                units: Units::Mixed,
                resolution: Resolution::ComputedFit,
                supplier: supplier.clone(),
                style_guid: Some(id.into()),
                linked_by: Some(supplier.clone()),
            })
            .collect();
        assert!(value.valid(), "the large record uses only existing valid schema values");
        assert!(serde_json::to_string(&value).unwrap().len() > MAX_RECORD_BYTES, "the fixture actually crosses the encoded byte limit");
        let mut used = 0;
        test_reset_work();
        assert!(encode(&value, &mut used).is_err());
        assert_eq!(used, 0, "failed encoding consumes no accepted-byte quota");
        assert!(test_work()[3] <= MAX_RECORD_BYTES, "the real encoder never allocates a complete over-limit JSON payload");
    }
}
