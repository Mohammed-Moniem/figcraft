//! K-scale presentation of dimensional variable bindings. Contexts are relative to their parent.

use figcraft_doc::{Document, Node, NodeId};
use std::collections::BTreeSet;

/// Malformed saved contexts cannot introduce a negative, zero or non-finite scale.
pub(crate) fn context(value: Option<f64>) -> f64 {
    value.filter(|x| x.is_finite() && *x > 0.0).unwrap_or(1.0)
}

/// The scale on this layer, including each ancestor once. Refuse nonrepresentable products.
pub(crate) fn factor(doc: &Document, id: NodeId) -> Option<f64> {
    let mut factor = 1.0;
    let mut seen = BTreeSet::new();
    let mut current = Some(id);
    while let Some(id) = current {
        // Malformed imports must not silently truncate ancestry or apply a cycle repeatedly.
        if seen.len() >= 10_000 || !seen.insert(id) {
            return None;
        }
        let node = doc.node(id)?;
        factor *= context(node.variable_scale);
        if !factor.is_finite() || factor <= 0.0 {
            return None;
        }
        current = node.parent;
    }
    Some(factor)
}

pub(crate) fn dimensional_group(group: &str) -> bool {
    matches!(group, "padding" | "gap" | "corners" | "fills" | "strokes" | "effects" | "grids" | "textStyle")
}

/// Grid bindings have separate component matching rules, but their values still belong to the
/// grids group when carrying overrides or retiring K-derived ownership.
pub(crate) fn binding_group(key: &str) -> Option<&'static str> {
    use crate::cmd::lists::{BoundList, list_binding};
    match list_binding(key) {
        Some((BoundList::Grids, _, _)) => Some("grids"),
        _ => crate::components::binding_group(key),
    }
}

/// Explicit changes own the corresponding group even when a new binding resolves to the same
/// number. Call before derived binding resolution, so variable/mode changes do not become edits.
pub(crate) fn retire_changed(doc: &mut Document, before: &Document) {
    let changed = doc.changed_ids(before, usize::MAX).unwrap_or_default();
    for id in changed {
        let (Some(n), Some(b)) = (doc.node(id), before.node(id)) else { continue };
        if n.scale_derived_overrides.is_empty() {
            continue;
        }
        let mut groups: BTreeSet<&str> = crate::components::diff_groups(b, n).into_iter().collect();
        // Choosing or detaching a style owns its group even when cached values are equal.
        // Otherwise a later main binding can replace that explicit local choice.
        for (group, changed) in [
            ("fills", b.styles.fill != n.styles.fill),
            ("strokes", b.styles.stroke != n.styles.stroke),
            ("effects", b.styles.effect != n.styles.effect),
            ("grids", b.styles.grid != n.styles.grid),
            ("textStyle", b.styles.text != n.styles.text),
        ] {
            if changed {
                groups.insert(group);
            }
        }
        for key in b.bound.keys().chain(n.bound.keys()) {
            if b.bound.get(key) != n.bound.get(key) {
                groups.extend(binding_group(key));
            }
        }
        let retired: Vec<String> = n.scale_derived_overrides.iter().filter(|g| groups.contains(g.as_str())).cloned().collect();
        if !retired.is_empty()
            && let Some(n) = doc.node_mut(id)
        {
            for group in retired {
                n.scale_derived_overrides.remove(&group);
            }
        }
    }
}

pub(crate) fn retire_field(n: &mut Node, field: &str) {
    if let Some(group) = binding_group(field) {
        n.scale_derived_overrides.remove(group);
    }
}

pub(crate) fn length(field: &str) -> bool {
    matches!(
        field,
        "width"
            | "w"
            | "height"
            | "h"
            | "minWidth"
            | "maxWidth"
            | "minHeight"
            | "maxHeight"
            | "gap"
            | "columnGap"
            | "crossGap"
            | "rowGap"
            | "padding"
            | "paddingTop"
            | "paddingRight"
            | "paddingBottom"
            | "paddingLeft"
            | "paddingHorizontal"
            | "paddingVertical"
            | "cornerRadius"
            | "topLeftRadius"
            | "topRightRadius"
            | "bottomRightRadius"
            | "bottomLeftRadius"
            | "strokeWeight"
            | "strokeTopWeight"
            | "strokeRightWeight"
            | "strokeBottomWeight"
            | "strokeLeftWeight"
            | "fontSize"
            | "lineHeight"
            | "letterSpacing"
            | "paragraphSpacing"
            | "paragraphIndent"
    )
}

pub(crate) fn effect_length(field: &str) -> bool {
    matches!(field, "offsetX" | "offsetY" | "radius" | "startRadius" | "spread" | "depth")
}

pub(crate) fn grid_length(field: &str) -> bool {
    matches!(field, "size" | "gutter" | "offset")
}
