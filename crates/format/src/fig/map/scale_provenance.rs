//! Capture historical supplier observations while private .fig views still exist.
//! No observation here installs or resolves native scaling context.

use super::component::{Entry, Inst, OverrideRow};
use super::{Kv, MAX_SCALE_FACTOR, MIN_SCALE_FACTOR, Mapper, Obj, Value, View, guid, scaled};
use crate::fig_scale_provenance::{Authority, Field, ReadScope, Record, RelativeFactor, Resolution, RowKind, Supplier, Units};

pub(super) struct Capture<'c, 'a> {
    pub id: &'c str,
    pub rest_id: &'c str,
    pub view: &'c View<'a>,
    pub entries: &'c [OverrideRow<'a>],
    pub instance: Option<&'c Inst<'a>>,
    pub wire_instance: bool,
}

/// Unit-only expansion is ordinary component/override mapping, not scale history. In
/// particular our exporter writes explicit one even when no Scale operation occurred.
/// Once admitted, keep all descriptors, including unit suppliers, unknown units, styles
/// and computed geometry. The private ancestry bit preserves cancelled nested factors.
pub(super) fn has_scale_history(view: &View<'_>, instance: Option<&Inst<'_>>, wire_instance: bool) -> bool {
    if instance.is_none() && !wire_instance {
        return false;
    }
    instance.is_some_and(|instance| instance.has_scale_history)
        || view.scales.iter().any(|scale| *scale != 1.0)
        || (wire_instance && view.get("symbolData").and_then(|data| data.get("uniformScaleFactor")).is_some_and(|factor| factor.f64() != Some(1.0)))
}

fn index(view: &View<'_>, key: &str) -> Option<usize> {
    view.layers.iter().rposition(|row| row.get(key).is_some())
}

fn supplier(c: &Capture<'_, '_>, view: &View<'_>, at: usize, style: Option<&Kv<'_>>) -> Option<Supplier> {
    let row = *view.layers.get(at)?;
    let scale = *view.scales.get(at)?;
    let entry = c.entries.iter().find(|entry| std::ptr::eq(entry.row, row));
    let kind = match entry.map(|entry| entry.entry) {
        Some(Entry::Override) => RowKind::Override,
        Some(Entry::Derived) => RowKind::Derived,
        Some(Entry::Stale) => RowKind::Stale,
        None if style.is_some_and(|style| std::ptr::eq(style, row)) => RowKind::SharedStyle,
        None if row.get("guid").and_then(guid).is_some() => RowKind::Wire,
        _ => RowKind::Auxiliary,
    };
    Some(Supplier {
        kind,
        view_index: u32::try_from(at).ok()?,
        scale,
        row_guid: row.get("guid").and_then(guid),
        owner_guid: entry.and_then(|entry| entry.owner).and_then(|owner| owner.get("guid")).and_then(guid),
        ordinal: entry.and_then(|entry| u32::try_from(entry.ordinal).ok()),
        source_depth: entry.and_then(|entry| u16::try_from(entry.source_depth).ok()),
        override_level: entry.and_then(|entry| u16::try_from(entry.level).ok()),
    })
}

fn raw_field(c: &Capture<'_, '_>, view: &View<'_>, key: &str, name: &str, units: Units, scope: ReadScope, style: Option<&Kv<'_>>) -> Option<Field> {
    let at = index(view, key)?;
    Some(Field {
        name: name.into(),
        scope,
        units,
        resolution: if view.fitted.is_some() && matches!(key, "size" | "transform") { Resolution::ComputedFit } else { Resolution::RawWinner },
        supplier: supplier(c, view, at, style)?,
        style_guid: style.and_then(|style| style.get("guid")).and_then(guid),
        linked_by: None,
    })
}

fn numeric_index(view: &View<'_>, key: &str) -> Option<usize> {
    view.layers.iter().zip(&view.scales).rposition(|(row, scale)| {
        row.get(key).filter(|value| !matches!(value, Kv::Bool(_))).and_then(Kv::f64).and_then(|value| scaled(value, *scale)).is_some()
    })
}

fn unit_of(view: &View<'_>, key: &str) -> Units {
    match view.get(key).and_then(|value| value.get("units")).and_then(Kv::str) {
        Some("PIXELS") => Units::Pixels,
        Some("PERCENT") => Units::Percent,
        Some("RAW") => Units::NotDimensional,
        _ => Units::Unknown,
    }
}

const DIRECT: &[(&str, &str, Units)] = &[
    ("size", "size", Units::Length),
    ("transform", "transform.translation", Units::Length),
    ("cornerRadius", "cornerRadius", Units::Length),
    ("rectangleTopLeftCornerRadius", "rectangleTopLeftCornerRadius", Units::Length),
    ("rectangleTopRightCornerRadius", "rectangleTopRightCornerRadius", Units::Length),
    ("rectangleBottomLeftCornerRadius", "rectangleBottomLeftCornerRadius", Units::Length),
    ("rectangleBottomRightCornerRadius", "rectangleBottomRightCornerRadius", Units::Length),
    ("strokeWeight", "strokeWeight", Units::Length),
    ("borderTopWeight", "borderTopWeight", Units::Length),
    ("borderRightWeight", "borderRightWeight", Units::Length),
    ("borderBottomWeight", "borderBottomWeight", Units::Length),
    ("borderLeftWeight", "borderLeftWeight", Units::Length),
    ("dashPattern", "dashPattern", Units::Length),
    ("stackSpacing", "stackSpacing", Units::Length),
    ("stackCounterSpacing", "stackCounterSpacing", Units::Length),
    ("stackHorizontalPadding", "stackHorizontalPadding", Units::Length),
    ("stackVerticalPadding", "stackVerticalPadding", Units::Length),
    ("stackPaddingRight", "stackPaddingRight", Units::Length),
    ("stackPaddingBottom", "stackPaddingBottom", Units::Length),
    ("gridColumnGap", "gridColumnGap", Units::Length),
    ("gridRowGap", "gridRowGap", Units::Length),
    ("gridColumns", "gridColumns", Units::Mixed),
    ("gridRows", "gridRows", Units::Mixed),
    ("minSize", "minSize", Units::Length),
    ("maxSize", "maxSize", Units::Length),
    ("effects", "effects", Units::Mixed),
    ("layoutGrids", "layoutGrids", Units::Mixed),
    ("paragraphSpacing", "paragraphSpacing", Units::Length),
    ("paragraphIndent", "paragraphIndent", Units::Length),
    ("textData", "textData.characterStyles", Units::Mixed),
    ("vectorData", "vectorData.pointStyles", Units::Mixed),
    ("parameterConsumptionMap", "parameterConsumptionMap", Units::NotDimensional),
];

impl<'a> Mapper<'a> {
    pub(super) fn retain_expansion_provenance(&mut self, out: &mut Obj, view: &View<'a>, id: &str, rest_id: &str, instance: Option<&Inst<'a>>) {
        let wire_instance =
            out.get(crate::rest::FIG_SOURCE_FIELD).and_then(|source| source.get("wireInstance")).and_then(Value::as_bool) == Some(true);
        if !has_scale_history(view, instance, wire_instance) {
            return;
        }
        if !crate::fig_scale_provenance::can_capture(self.scale_provenance_bytes) {
            self.warn("optional .fig scale provenance could not reserve bounded storage and was omitted");
            return;
        }
        let path = instance.map(|instance| [instance.path.as_slice(), &[id.to_string()]].concat());
        let entries = instance.zip(path.as_ref()).and_then(|(instance, path)| instance.overrides.get(path)).map(Vec::as_slice).unwrap_or_default();
        self.retain_scale_provenance(out, Capture { id, rest_id, view, entries, instance, wire_instance });
    }

    pub(super) fn retain_scale_provenance(&mut self, out: &mut Obj, c: Capture<'_, 'a>) {
        if !has_scale_history(c.view, c.instance, c.wire_instance) {
            return;
        }
        if !crate::fig_scale_provenance::can_capture(self.scale_provenance_bytes) {
            self.warn("optional .fig scale provenance could not reserve bounded storage and was omitted");
            return;
        }
        #[cfg(test)]
        crate::fig_scale_provenance::test_note_capture();
        let record = self.scale_observation(out, &c);
        match crate::fig_scale_provenance::encode(&record, &mut self.scale_provenance_bytes) {
            Ok(payload) => {
                if let Some(source) = out.get_mut(crate::rest::FIG_SOURCE_FIELD).and_then(Value::as_object_mut) {
                    source.insert(crate::fig_scale_provenance::PRIVATE_FIELD.into(), Value::String(payload));
                }
            }
            Err(()) => self.warn("optional .fig scale provenance exceeded its schema or storage limits and was omitted"),
        }
    }

    fn scale_observation(&self, out: &Obj, c: &Capture<'_, 'a>) -> Record {
        let mut fields = std::collections::BTreeMap::new();
        for &(key, name, units) in DIRECT {
            if let Some(field) = raw_field(c, c.view, key, name, units, ReadScope::Layer, None) {
                fields.insert(name.to_string(), field);
            }
        }
        // listSpacing alone uses the last valid non-boolean numeric row, as text_num does.
        if let Some(at) = numeric_index(c.view, "listSpacing")
            && let Some(source) = supplier(c, c.view, at, None)
        {
            fields.insert(
                "listSpacing".into(),
                Field {
                    name: "listSpacing".into(),
                    scope: ReadScope::Layer,
                    units: Units::Length,
                    resolution: Resolution::RawWinner,
                    supplier: source,
                    style_guid: None,
                    linked_by: None,
                },
            );
        }
        self.linked_scale_fields(out, c, &mut fields);
        let enclosing = c.instance.map_or(1.0, |instance| instance.scale);
        let local = c.wire_instance.then(|| {
            let wire = c.view.get("symbolData").and_then(|data| data.get("uniformScaleFactor")).and_then(Kv::f64);
            let effective = wire.filter(|value| (MIN_SCALE_FACTOR..=MAX_SCALE_FACTOR).contains(value)).unwrap_or(1.0);
            RelativeFactor { wire, effective, supplier: index(c.view, "symbolData").and_then(|at| supplier(c, c.view, at, None)) }
        });
        let product = local.as_ref().map(|local| enclosing * local.effective);
        let children = product.map(|product| product.clamp(MIN_SCALE_FACTOR, MAX_SCALE_FACTOR));
        Record {
            version: 1,
            authority: Authority::DescriptiveOnly,
            origin_guid: c.id.into(),
            rest_id: c.rest_id.into(),
            enclosing_cumulative: enclosing,
            local_relative: local,
            children_cumulative: children,
            child_clamp_applied: product.zip(children).is_some_and(|(product, children)| product != children),
            fitted_geometry: c.view.fitted.is_some(),
            fields: fields.into_values().collect(),
        }
    }
    fn linked_scale_fields(&self, out: &Obj, c: &Capture<'_, 'a>, fields: &mut std::collections::BTreeMap<String, Field>) {
        let styles = out.get("styles");
        for (slot, key, legacy, field_name, scope) in [
            ("effect", "styleIdForEffect", "inheritEffectStyleID", "effects", ReadScope::EffectStyle),
            ("grid", "styleIdForGrid", "inheritGridStyleID", "layoutGrids", ReadScope::GridStyle),
        ] {
            let style = styles.and_then(|styles| styles.get(slot)).and_then(Value::as_str).and_then(|id| self.nodes.get(id)).copied();
            if let Some(style) = style {
                let view = View::of_scaled(style, c.view.link_scale(key, legacy));
                if let Some(mut field) = raw_field(c, &view, field_name, field_name, Units::Mixed, scope, Some(style)) {
                    field.linked_by = index(c.view, key).or_else(|| index(c.view, legacy)).and_then(|at| supplier(c, c.view, at, None));
                    fields.insert(field_name.into(), field);
                }
            }
        }
        let text_style = styles
            .and_then(|styles| styles.get("text"))
            .and_then(Value::as_str)
            .and_then(|id| self.nodes.get(id))
            .copied()
            .filter(|style| style.get("styleType").and_then(Kv::str) == Some("TEXT"));
        let text_view = c.view.styled(text_style, "styleIdForText", "inheritTextStyleID");
        for key in ["fontSize", "letterSpacing", "lineHeight"] {
            let units = if key == "fontSize" { Units::Length } else { unit_of(&text_view, key) };
            if let Some(mut field) = raw_field(c, &text_view, key, key, units, ReadScope::StyledText, text_style) {
                if text_style.is_some() {
                    field.linked_by =
                        index(c.view, "styleIdForText").or_else(|| index(c.view, "inheritTextStyleID")).and_then(|at| supplier(c, c.view, at, None));
                }
                fields.insert(key.into(), field);
            }
        }
        // Effective paragraph settings in the layer beat the shared style; without one the
        // already-built styled view remains the donor. Preserve that exact fallback distinction.
        for key in ["paragraphSpacing", "paragraphIndent"] {
            if c.view.len(key).is_none()
                && let Some(mut field) = raw_field(c, &text_view, key, key, Units::Length, ReadScope::StyledText, text_style)
            {
                field.linked_by =
                    index(c.view, "styleIdForText").or_else(|| index(c.view, "inheritTextStyleID")).and_then(|at| supplier(c, c.view, at, None));
                fields.insert(key.into(), field);
            }
        }
        if numeric_index(c.view, "listSpacing").is_none()
            && let Some(at) = numeric_index(&text_view, "listSpacing")
            && let Some(source) = supplier(c, &text_view, at, text_style)
        {
            fields.insert(
                "listSpacing".into(),
                Field {
                    name: "listSpacing".into(),
                    scope: ReadScope::StyledText,
                    units: Units::Length,
                    resolution: Resolution::RawWinner,
                    supplier: source,
                    style_guid: text_style.and_then(|style| style.get("guid")).and_then(guid),
                    linked_by: index(c.view, "styleIdForText")
                        .or_else(|| index(c.view, "inheritTextStyleID"))
                        .and_then(|at| supplier(c, c.view, at, None)),
                },
            );
        }
    }
}

#[cfg(test)]
mod budget_admission_tests {
    use super::*;

    #[test]
    fn provenance_budget_mapper_skips_capture_and_serialization_before_admission() {
        use crate::fig_scale_provenance::{MAX_RECORD_BYTES, MAX_TOTAL_BYTES, test_reset_work, test_work};
        let meaningful = Kv::Object(vec![("symbolData", Kv::Object(vec![("uniformScaleFactor", Kv::Float(0.5))]))]);
        let view = View::of(&meaningful);
        for initial in [MAX_TOTAL_BYTES, usize::MAX, MAX_TOTAL_BYTES - MAX_RECORD_BYTES + 1] {
            let mut mapper = Mapper::new(&meaningful, 1);
            mapper.scale_provenance_bytes = initial;
            let mut out = Obj::new();
            out.insert(crate::rest::FIG_SOURCE_FIELD.into(), serde_json::json!({"origin": "1:9", "wireInstance": true}));
            test_reset_work();
            mapper.retain_scale_provenance(
                &mut out,
                Capture { id: "1:9", rest_id: "1:9", view: &view, entries: &[], instance: None, wire_instance: true },
            );
            assert_eq!(test_work(), [0; 4], "the real mapper must not build a record after reservation fails");
            assert_eq!(mapper.scale_provenance_bytes, initial);
            assert!(out[crate::rest::FIG_SOURCE_FIELD].get(crate::fig_scale_provenance::PRIVATE_FIELD).is_none());
        }
        let mut mapper = Mapper::new(&meaningful, 1);
        let mut out = Obj::new();
        out.insert(crate::rest::FIG_SOURCE_FIELD.into(), serde_json::json!({"origin": "1:9", "wireInstance": true}));
        test_reset_work();
        mapper
            .retain_scale_provenance(&mut out, Capture { id: "1:9", rest_id: "1:9", view: &view, entries: &[], instance: None, wire_instance: true });
        assert_eq!(test_work()[0], 1, "the same meaningful context captures when storage is available");
        assert_eq!(test_work()[1], 1, "positive capture reaches the actual bounded encoder");
        assert!(mapper.scale_provenance_bytes > 0);
        let payload = out[crate::rest::FIG_SOURCE_FIELD][crate::fig_scale_provenance::PRIVATE_FIELD].as_str().unwrap();
        let record: Record = serde_json::from_str(payload).unwrap();
        assert_eq!(record.local_relative.unwrap().wire, Some(0.5));
    }
}
