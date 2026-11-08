//! Variables bound inside shared styles. A style's `bound` map names fields of its own value in
//! the keys a layer's bindings use (`fills/<i>/color`, `fills/<i>/stops/<j>/color`,
//! `effects/<i>/<field>`, `layoutGrids/<i>/<field>`, a text style's `fontSize`, `lineHeight`,
//! `letterSpacing`, `fontFamily`, `fontStyle`). Each layer using the style sees those fields
//! resolved in its own modes; the style itself stores them resolved in the default modes.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use figcraft_color::PaintKind;
use figcraft_doc::{
    CollectionId, Document, EffectKind, LetterSpacing, LineHeight, ModeId, Node, NodeId, NodeKind, Style, StyleId, StyleKind, TextStyle, VariableId,
    VariableType, VariableValue, Variables,
};

use crate::cmd::lists::{BoundList, list_binding, stop_binding};

mod scale;
pub(crate) use scale::{effects as scale_effects, grids as scale_grids, paints as scale_paints, text as scale_text};

type Modes = BTreeMap<CollectionId, ModeId>;

/// The type of variable a style of `kind` takes on `key`; None when it has no such field.
pub fn field_type(kind: &StyleKind, key: &str) -> Option<VariableType> {
    use VariableType::{Color, Number, String};
    match (kind, list_binding(key)) {
        (StyleKind::Paint { paints }, Some((BoundList::Fills, i, field))) => {
            let p = paints.get(i)?;
            let ok = match stop_binding(field) {
                Some(j) => matches!(&p.kind, PaintKind::Gradient(g) if j < g.stops.len()),
                None => field == "color" && matches!(p.kind, PaintKind::Solid { .. }),
            };
            ok.then_some(Color)
        }
        (StyleKind::Effect { effects }, Some((BoundList::Effects, i, field))) => match (&effects.get(i)?.kind, field) {
            (EffectKind::DropShadow(_) | EffectKind::InnerShadow(_), "color") => Some(Color),
            (EffectKind::DropShadow(_) | EffectKind::InnerShadow(_), "radius" | "spread" | "offsetX" | "offsetY") => Some(Number),
            (EffectKind::LayerBlur(_) | EffectKind::BackgroundBlur(_), "radius" | "startRadius") => Some(Number),
            (EffectKind::Glass(_), f) if figcraft_doc::Glass::BOUND_FIELDS.contains(&f) => Some(Number),
            _ => None,
        },
        (StyleKind::Grid { grids }, Some((BoundList::Grids, i, "size" | "count" | "gutter" | "offset"))) => grids.get(i).map(|_| Number),
        (StyleKind::Text { .. }, None) => match key {
            "fontSize" | "lineHeight" | "letterSpacing" => Some(Number),
            "fontFamily" | "fontStyle" => Some(String),
            _ => None,
        },
        _ => None,
    }
}

/// The variable scopes a picker offers for `key` of a style of `kind`: a variable scoped to any one
/// of them fits. A paint style's colors take fill and stroke variables, an effect style's fields
/// take effect variables, and a text style's fields take the variables of their font property.
/// None when every variable of the field's type fits (a grid's fields, as on layers) or the style
/// has no such field.
pub fn field_scopes(kind: &StyleKind, key: &str) -> Option<&'static [&'static str]> {
    field_type(kind, key)?;
    Some(match (kind, list_binding(key)) {
        (StyleKind::Paint { .. }, _) => &["FRAME_FILL", "SHAPE_FILL", "TEXT_FILL", "STROKE_COLOR"],
        (StyleKind::Effect { .. }, Some((_, _, "color"))) => &["EFFECT_COLOR"],
        (StyleKind::Effect { .. }, _) => &["EFFECT_FLOAT"],
        (StyleKind::Text { .. }, _) => match key {
            "fontSize" => &["FONT_SIZE"],
            "lineHeight" => &["LINE_HEIGHT"],
            "letterSpacing" => &["LETTER_SPACING"],
            "fontFamily" => &["FONT_FAMILY"],
            "fontStyle" => &["FONT_STYLE"],
            _ => return None,
        },
        _ => return None,
    })
}

/// The key a style stores a binding of `key` under: the first paint's colour has one spelling.
pub fn canonical(key: &str) -> String {
    match list_binding(key) {
        Some((list, i, field)) => list.key(i, field),
        None => key.to_string(),
    }
}

/// Bindings that no longer name a field of the style's value (an item removed, a paint turned
/// into a gradient) go.
pub fn prune(st: &mut Style) {
    let kind = &st.kind;
    st.bound.retain(|k, _| field_type(kind, k).is_some());
}

/// The bindings a new style of `kind` takes over from the layer it was made from (`field`: the
/// link the layer gets to it).
pub fn from_layer(n: &Node, field: &str, kind: &StyleKind) -> BTreeMap<String, VariableId> {
    let mut out = BTreeMap::new();
    for (k, v) in &n.bound {
        let key = match (field, list_binding(k)) {
            ("fillStyle", Some((BoundList::Fills, i, f))) | ("strokeStyle", Some((BoundList::Strokes, i, f))) => BoundList::Fills.key(i, f),
            ("effectStyle", Some((BoundList::Effects, ..))) | ("gridStyle", Some((BoundList::Grids, ..))) | ("textStyle", None) => k.clone(),
            _ => continue,
        };
        if field_type(kind, &key).is_some() {
            out.insert(key, v.clone());
        }
    }
    out
}

/// `st`'s value with its bound fields resolved in `modes`.
fn resolved(vars: &Variables, st: &Style, modes: &Modes, factor: f64) -> StyleKind {
    let mut kind = scale::kind(&st.kind, factor).unwrap_or_else(|| st.kind.clone());
    for (key, var) in &st.bound {
        let Some(mut val) = vars.resolve(var, modes) else { continue };
        if let VariableValue::Number(value) = &mut val
            && dimensional(key)
            && factor != 1.0
        {
            *value *= factor;
            if !value.is_finite() {
                continue;
            }
        }
        match (&mut kind, list_binding(key), &val) {
            (StyleKind::Paint { paints }, Some((BoundList::Fills, i, field)), VariableValue::Color(c)) => {
                crate::cmd::styles::set_paint_color(paints, i, stop_binding(field), *c);
            }
            (StyleKind::Effect { effects }, Some((BoundList::Effects, i, field)), _) => crate::cmd::styles::set_effect_field(effects, i, field, &val),
            (StyleKind::Grid { grids }, Some((BoundList::Grids, i, field)), VariableValue::Number(x)) => {
                crate::cmd::lists::set_grid_field(grids, i, field, *x);
            }
            (StyleKind::Text { style }, None, _) => set_text_field(style, key, &val),
            _ => {}
        }
    }
    kind
}

fn dimensional(key: &str) -> bool {
    use crate::variable_scale;
    match list_binding(key) {
        Some((BoundList::Effects, _, field)) => variable_scale::effect_length(field),
        Some((BoundList::Grids, _, field)) => variable_scale::grid_length(field),
        None => variable_scale::length(key),
        _ => false,
    }
}

fn set_text_field(style: &mut TextStyle, key: &str, val: &VariableValue) {
    match (key, val) {
        ("fontSize", VariableValue::Number(x)) if x.is_finite() => style.font_size = x.clamp(1.0, 5000.0),
        ("lineHeight", VariableValue::Number(x)) if x.is_finite() => style.line_height = LineHeight::Pixels(x.clamp(0.0, 100_000.0)),
        ("letterSpacing", VariableValue::Number(x)) if x.is_finite() => style.letter_spacing = LetterSpacing::Pixels(x.clamp(-10_000.0, 10_000.0)),
        ("fontFamily", VariableValue::String(f)) if !f.trim().is_empty() => style.font_family = f.clone(),
        ("fontStyle", VariableValue::String(f)) if !f.trim().is_empty() => style.font_style = f.clone(),
        _ => {}
    }
}

/// `st`'s value as layer `id` shows it: bound fields resolved in the layer's modes.
pub fn kind_for<'a>(doc: &Document, st: &'a Style, id: NodeId) -> Cow<'a, StyleKind> {
    let factor = crate::variable_scale::factor(doc, id).unwrap_or(1.0);
    kind_for_factor(doc, st, id, factor)
}

fn kind_for_factor<'a>(doc: &Document, st: &'a Style, id: NodeId, factor: f64) -> Cow<'a, StyleKind> {
    if st.bound.is_empty() && factor == 1.0 { Cow::Borrowed(&st.kind) } else { Cow::Owned(resolved(&doc.variables, st, &doc.modes_for(id), factor)) }
}

/// Capture physical values as a shared definition independent of the source layer's K context.
pub(crate) fn captured_kind(doc: &Document, id: NodeId, kind: &StyleKind) -> Option<StyleKind> {
    let factor = crate::variable_scale::factor(doc, id)?;
    let captured = scale::kind(kind, 1.0 / factor)?;
    let presented = scale::kind(&captured, factor)?;
    scale::equivalent(kind, &presented).then_some(captured)
}

/// An unchanged contextual presentation keeps its existing global definition, including
/// saturated values whose inverse is ambiguous. Changed sources use inverse capture.
pub(crate) fn redefined_kind(doc: &Document, id: NodeId, current: &Style, kind: &StyleKind) -> Option<StyleKind> {
    let factor = crate::variable_scale::factor(doc, id)?;
    let presented = kind_for_factor(doc, current, id, factor);
    if scale::equivalent(kind, presented.as_ref()) { Some(current.kind.clone()) } else { captured_kind(doc, id, kind) }
}

fn linked(n: &Node) -> BTreeSet<&str> {
    let refs = &n.styles;
    let mut ids: BTreeSet<_> = [&refs.fill, &refs.stroke, &refs.effect, &refs.grid, &refs.text].into_iter().flatten().map(String::as_str).collect();
    if let NodeKind::Text(text) = &n.kind {
        ids.extend(text.style.style_id.as_deref());
        ids.extend(text.runs.iter().filter_map(|run| run.style.style_id.as_deref()));
    }
    ids
}

/// A K operation cannot make a linked style's dimensional presentation nonrepresentable.
pub(crate) fn can_scale(doc: &Document, ids: &[NodeId], factor: f64) -> bool {
    ids.iter().all(|id| {
        let Some(n) = doc.node(*id) else { return false };
        let links = linked(n);
        if links.is_empty() {
            return true;
        }
        let Some(factor) = crate::variable_scale::factor(doc, *id).map(|scale| scale * factor) else { return false };
        let modes = doc.modes_for(*id);
        links.into_iter().filter_map(|id| doc.style(id)).all(|style| {
            scale::kind(&style.kind, factor).is_some()
                && style.bound.iter().all(|(key, var)| {
                    !dimensional(key)
                        || match doc.variables.resolve(var, &modes) {
                            Some(VariableValue::Number(value)) => (value * factor).is_finite(),
                            _ => true,
                        }
                })
        })
    })
}

/// Each style with bindings stores its value resolved in the default modes (what pickers, exports
/// and other software show for it).
pub fn settle(doc: &mut Document) {
    let none = Modes::new();
    for i in 0..doc.styles.len() {
        let Some(st) = doc.styles.get(i).filter(|s| !s.bound.is_empty()) else { continue };
        let mut next = st.clone();
        prune(&mut next);
        next.kind = resolved(&doc.variables, &next, &none, 1.0);
        if next != *st
            && let Some(slot) = doc.styles.get_mut(i)
        {
            *slot = next;
        }
    }
}

/// A document just read from a file: styles with bindings hold their default-mode values and the
/// layers using them show their own modes' (files give a styled layer the style's stored value).
pub fn after_load(doc: &mut Document) {
    settle(doc);
    refresh(doc, None, None);
}

/// The layers `only` (every layer when `None`) that use styles with bindings show them resolved in
/// their own modes (only what changed, so unchanged layers stay shared). What an edit since
/// `before` changed on a layer is left as edited: that unlinks it from the style instead.
pub fn refresh(doc: &mut Document, before: Option<&Document>, only: Option<&BTreeSet<NodeId>>) {
    if doc.styles.is_empty() {
        return;
    }
    let bound: BTreeSet<StyleId> = doc.styles.iter().filter(|style| !style.bound.is_empty()).map(|style| style.id.clone()).collect();
    // Foreign imports and ordinary old documents have no native K context. Keep their legacy
    // bound-only refresh, without walking every linked layer's ancestors or reapplying literals.
    let has_context = |doc: &Document| doc.nodes().any(|n| crate::variable_scale::context(n.variable_scale) != 1.0);
    let may_scale = only.is_some() || has_context(doc) || before.is_some_and(has_context);
    if bound.is_empty() && !may_scale {
        return;
    }
    let uses = |n: &Node| {
        let links = linked(n);
        !links.is_empty() && (may_scale || links.iter().any(|id| bound.contains(*id)))
    };
    let ids: Vec<NodeId> = match only {
        Some(ids) => ids.iter().copied().filter(|i| doc.node(*i).is_some_and(uses)).collect(),
        None => doc.nodes().filter(|n| uses(n)).map(|n| n.id).collect(),
    };
    for id in ids {
        let Some(n) = doc.node(id) else { continue };
        let was = before.and_then(|b| b.node(id));
        let factor = if may_scale { crate::variable_scale::factor(doc, id).unwrap_or(1.0) } else { 1.0 };
        let scaled = was.is_some() && before.is_some_and(|before| crate::variable_scale::factor(before, id) != Some(factor));
        let contextual = factor != 1.0 || scaled;
        let links = linked(n);
        if !contextual && !links.iter().any(|id| bound.contains(*id)) {
            continue;
        }
        let mut next = n.clone();
        let of = |r: &Option<StyleId>| {
            r.as_deref()
                .and_then(|s| doc.style(s))
                .filter(|style| !style.bound.is_empty() || contextual)
                .map(|style| kind_for_factor(doc, style, id, factor))
        };
        if let Some(StyleKind::Paint { paints }) = of(&n.styles.fill).as_deref()
            && was.is_none_or(|w| w.fills == n.fills || (scaled && w.styles.fill == n.styles.fill))
        {
            next.fills.clone_from(paints);
        }
        if let Some(StyleKind::Paint { paints }) = of(&n.styles.stroke).as_deref()
            && was.is_none_or(|w| w.strokes == n.strokes || (scaled && w.styles.stroke == n.styles.stroke))
        {
            next.strokes.clone_from(paints);
        }
        if let Some(StyleKind::Effect { effects }) = of(&n.styles.effect).as_deref()
            && was.is_none_or(|w| w.effects == n.effects || (scaled && w.styles.effect == n.styles.effect))
        {
            next.effects.clone_from(effects);
        }
        if let Some(StyleKind::Grid { grids }) = of(&n.styles.grid).as_deref()
            && was.is_none_or(|w| w.grids == n.grids || (scaled && w.styles.grid == n.styles.grid))
        {
            next.grids.clone_from(grids);
        }
        if let NodeKind::Text(t) = &mut next.kind
            && was.is_none_or(|w| w.kind == n.kind || scaled)
        {
            // Resolve only the styles this text actually uses, including mixed linked runs.
            for st in links.into_iter().filter_map(|id| doc.style(id)).filter(|style| !style.bound.is_empty() || contextual) {
                if let StyleKind::Text { style } = kind_for_factor(doc, st, id, factor).as_ref() {
                    t.update_text_style(&st.id, style);
                }
            }
        }
        if next != *n
            && let Ok(slot) = doc.get_mut(id)
        {
            *slot = next;
        }
    }
}
