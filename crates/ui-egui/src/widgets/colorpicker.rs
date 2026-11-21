//! The colour picker popover: paint type, saturation/brightness square, hue and alpha sliders,
//! hex and RGB fields, gradient stops, image and pattern fills and document colours.

use egui::{Align2, Color32, CornerRadius, Id, Mesh, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use figcraft_color::{
    BlendMode, Color, ColorStop, Gradient, GradientKind, Hsb, ImageFilters, ImagePaint, ImageScaleMode, Paint, PaintKind, PatternAlign, PatternPaint,
    PatternTile,
};
use figcraft_doc::{NodeId, NodeKind};
use serde_json::json;

use crate::FigApp;
use crate::a11y::Tip;
use crate::i18n::{tr, tr_noop, trf};
use crate::icons::{self, Icon};
use crate::popover::{Hold, Open, Popover};
use crate::theme::{Tokens, doc_c32, regular};
use crate::widgets;

#[derive(Clone, Debug, PartialEq)]
pub enum PickerTarget {
    PageBackground,
    SelectionColor {
        targets: Vec<figcraft_engine::selection_colors::PaintTarget>,
        selection: Vec<NodeId>,
        document: u64,
    },
    Paint {
        fills: bool,
        index: usize,
        ids: Vec<NodeId>,
    },
    Effect {
        ids: Vec<NodeId>,
        index: usize,
        /// A duo noise's second colour.
        secondary: bool,
    },
    /// The fill of a character range of a text layer being edited.
    TextRange {
        id: NodeId,
        start: usize,
        end: usize,
    },
    /// A colour variable's value in one mode.
    Variable {
        id: String,
        mode: String,
    },
    /// One paint of a colour style (edited from the style picker).
    Style {
        id: String,
        index: usize,
    },
    /// A colour field of a plugin's panel or parameters form. A drag only shows `pending`; the
    /// plugin hears of the colour when the drag ends.
    PluginField {
        plugin: String,
        control: String,
        pending: Option<String>,
    },
}

/// These targets identify data in one open document. Background and plugin controls retain
/// their existing explicit cross-document behavior; an identical variable/node UUID is not an owner.
fn document_bound(target: &PickerTarget) -> bool {
    !matches!(target, PickerTarget::PageBackground | PickerTarget::PluginField { .. })
}

fn owner_matches(app: &FigApp, target: &PickerTarget) -> bool {
    !document_bound(target) || app.ui.picker_context.as_ref().is_none_or(|(_, uid, _, _)| app.session.ds().is_ok_and(|ds| ds.uid == *uid))
}

/// Capture ownership when the popup opens, before its first draw can be delayed by a tab switch.
pub(crate) fn bind_owner(app: &mut FigApp) {
    let Some(target) = app.ui.picker.clone() else { return };
    let Ok(ds) = app.session.ds() else { return };
    let (uid, page) = (ds.uid, ds.page);
    let image =
        matches!(target, PickerTarget::Paint { .. }) && current_paint(app, &target).is_some_and(|paint| matches!(paint.kind, PaintKind::Image(_)));
    app.ui.picker_context = Some((target, uid, page, image));
}

/// The paints of colour style `id`.
fn style_paints(doc: &figcraft_doc::Document, id: &str) -> Option<Vec<Paint>> {
    match &doc.style(id)?.kind {
        figcraft_doc::StyleKind::Paint { paints } => Some(paints.clone()),
        _ => None,
    }
}

/// The fills a text range paints with (its own, else the layer's).
pub fn range_fills(doc: &figcraft_doc::Document, id: NodeId, start: usize) -> Option<Vec<Paint>> {
    let n = doc.node(id)?;
    let t = n.kind.text()?;
    Some(t.style_at(start).fills.clone().unwrap_or_else(|| n.fills.clone()))
}

fn current_paint(app: &FigApp, target: &PickerTarget) -> Option<Paint> {
    if !owner_matches(app, target) {
        return None;
    }
    let doc = app.session.doc().ok()?;
    match target {
        PickerTarget::SelectionColor { targets, selection, document } => {
            let ds = app.session.ds().ok()?;
            if ds.uid != *document || ds.selection != *selection {
                return None;
            }
            targets.first()?.paint(doc)
        }
        PickerTarget::PageBackground => match &doc.node(app.session.page().ok()?)?.kind {
            NodeKind::Page { background, .. } => Some(Paint::solid(*background)),
            _ => None,
        },
        PickerTarget::Paint { fills, index, ids } => {
            let n = doc.node(*ids.first()?)?;
            let list = if *fills { &n.fills } else { &n.strokes };
            list.get(*index).cloned()
        }
        PickerTarget::Effect { ids, index, secondary } => doc.node(*ids.first()?)?.effects.get(*index)?.color(*secondary).map(Paint::solid),
        PickerTarget::TextRange { id, start, .. } => range_fills(doc, *id, *start)?.into_iter().next(),
        PickerTarget::Variable { id, mode } => match doc.variables.get_by_id(id)?.values.get(mode)? {
            figcraft_doc::VariableValue::Color(c) => {
                let mut p = Paint::solid(c.with_alpha(1.0));
                p.opacity = c.a;
                Some(p)
            }
            _ => None,
        },
        PickerTarget::Style { id, index } => style_paints(doc, id)?.get(*index).cloned(),
        PickerTarget::PluginField { plugin, control, pending } => {
            let hex = match pending {
                Some(hex) => hex.clone(),
                None => crate::plugins_ui::field_value(app, plugin, control)?,
            };
            let c = Color::from_hex(&hex)?;
            let mut p = Paint::solid(c.with_alpha(1.0));
            p.opacity = c.a;
            Some(p)
        }
    }
}

/// The command and its params that write `paint` to `target`, when the target can take it.
fn paint_command(app: &FigApp, target: &PickerTarget, paint: &Paint) -> Option<(&'static str, serde_json::Value)> {
    if !owner_matches(app, target) {
        return None;
    }
    Some(match target {
        PickerTarget::PluginField { .. } => return None,
        PickerTarget::SelectionColor { targets, .. } => ("selection.setPaint", json!({"targets": targets, "paint": paint})),
        PickerTarget::PageBackground => {
            let c = paint.solid_color().unwrap_or(Color::DEFAULT_PAGE);
            ("page.setBackground", json!({"color": c.to_css_hex()}))
        }
        PickerTarget::Paint { fills, index, ids } => {
            (if *fills { "fill.set" } else { "stroke.set" }, json!({"ids": ids, "index": index, "paint": paint}))
        }
        PickerTarget::Effect { ids, index, secondary } => {
            let mut e = ids.first().and_then(|i| app.session.doc().ok()?.node(*i)?.effects.get(*index).cloned())?;
            if let Some(c) = e.color_mut(*secondary) {
                *c = paint.solid_color().unwrap_or(Color::BLACK);
            }
            ("effect.set", json!({"ids": ids, "index": index, "effect": e}))
        }
        PickerTarget::TextRange { id, start, end } => ("text.style", json!({"id": id, "start": start, "end": end, "textFills": [paint]})),
        PickerTarget::Variable { id, mode } => {
            let c = paint.solid_color().unwrap_or(Color::BLACK);
            ("variable.set", json!({"variable": id, "mode": mode, "value": c.to_css_hex()}))
        }
        PickerTarget::Style { id, index } => {
            let mut paints = app.session.doc().ok().and_then(|d| style_paints(d, id))?;
            if let Some(slot) = paints.get_mut(*index) {
                *slot = paint.clone();
            }
            ("style.update", json!({"style": id, "value": paints}))
        }
    })
}

/// Write `paint` to the target. `live` edits are part of an ongoing drag.
fn write(app: &mut FigApp, target: &PickerTarget, paint: &Paint, live: bool) {
    if !owner_matches(app, target) {
        return;
    }
    if let PickerTarget::PluginField { plugin, control, .. } = target {
        let hex = paint.solid_color().unwrap_or(Color::BLACK).to_css_hex();
        let (plugin, control) = (plugin.clone(), control.clone());
        if !live {
            crate::plugins_ui::set_field(app, &plugin, &control, &hex);
        }
        app.ui.picker = Some(PickerTarget::PluginField { plugin, control, pending: live.then_some(hex) });
        return;
    }
    crate::number_wheel::before_edit(app);
    if live {
        let _ = app.session.begin_interaction("Color");
    }
    let Some((cmd, params)) = paint_command(app, target, paint) else { return };
    if let Err(e) = app.session.execute(cmd, &params) {
        app.toast(e.to_string());
    }
    // Drag previews (`live`) are not choices: a drag records only the colour it ends on. Not "an
    // interaction is open": editing text holds one, and its colour clicks are choices.
    if !live && let Some(c) = paint.solid_color() {
        remember_recent(app, c);
    }
}

/// Show the target's paint with `blend` on the canvas while its blend mode row is hovered.
pub(crate) fn preview_blend(app: &mut FigApp, ctx: &egui::Context, target: &PickerTarget, paint: &Paint, blend: BlendMode) {
    let mut p = paint.clone();
    p.blend = blend;
    if let Some((cmd, params)) = paint_command(app, target, &p) {
        app.preview_text(ctx, cmd, params);
    }
}

fn remember_recent(app: &mut FigApp, c: Color) {
    // By 8-bit value: a document stores colours rounded to 8 bits, so the colour picked and the one read
    // back differ by a fraction and would show as two identical swatches.
    app.session.recent_colors.retain(|x| x.to_rgba8() != c.to_rgba8());
    app.session.recent_colors.insert(0, c);
    app.session.recent_colors.truncate(16);
}

fn finish(app: &mut FigApp) {
    finish_with(app, true);
}

/// Finish the picker's edit. `record`: the colour it ends on is a choice for the recent colours
/// (not when the picker was only closed because its layers went away or changed).
fn finish_with(app: &mut FigApp, record: bool) {
    if let Some((target, owner, _, _)) = app.ui.picker_context.as_ref()
        && document_bound(target)
        && app.session.ds().is_ok_and(|ds| ds.uid != *owner)
    {
        let owner = *owner;
        finish_document(app, owner);
        return;
    }
    if let Some(PickerTarget::PluginField { plugin, control, pending: Some(hex) }) = app.ui.picker.clone() {
        crate::plugins_ui::set_field(app, &plugin, &control, &hex);
        app.ui.picker = Some(PickerTarget::PluginField { plugin, control, pending: None });
    }
    // Any open interaction counts for the recent colours: editing text holds an "Edit text" one, and
    // the colour a drag ends on is still a choice. Only a colour interaction is committed here.
    let label = app.session.ds().ok().and_then(|d| d.interaction.as_ref().map(|interaction| interaction.label.clone()));
    if let Some(label) = label {
        if record
            && let Some(t) = app.ui.picker.clone()
            && !matches!(t, PickerTarget::PluginField { .. })
            && let Some(c) = current_paint(app, &t).and_then(|p| p.solid_color())
        {
            remember_recent(app, c);
        }
        let gradient = matches!(label.as_str(), "Edit gradient" | "Add gradient stop" | "Remove gradient stop")
            && app.ui.picker.as_ref().and_then(|target| current_paint(app, target)).is_some_and(|paint| matches!(paint.kind, PaintKind::Gradient(_)));
        if label == "Color" || gradient {
            let _ = app.session.commit_interaction();
        }
    }
}

/// Finish the same eligible picker gestures in their owning document after a tab switch.
fn finish_document(app: &mut FigApp, uid: u64) {
    let Some(index) = app.session.docs.iter().position(|doc| doc.uid == uid) else { return };
    let active = app.session.active;
    app.session.active = index;
    // Reuse the normal Color/gradient policy, including its target check, without recording a
    // recent colour. The temporary owner matches the captured context, so this cannot recurse.
    finish_with(app, false);
    app.session.active = active;
}

/// The same layers, in any order.
fn same_layers(a: &[NodeId], b: &[NodeId]) -> bool {
    a.len() == b.len() && a.iter().all(|id| b.contains(id))
}

/// Close the picker, committing a colour interaction it still has open.
fn close_picker(app: &mut FigApp) {
    finish_with(app, false);
    app.ui.picker = None;
    app.ui.picker_context = None;
    app.ui.picker_popover = None;
    app.ui.picker_button = None;
    app.ui.picker_button_id = None;
}

/// Keep an open image-adjustment picker attached to the selected image layer.
/// Other picker targets retain their existing selection ownership.
fn sync_picker_target(app: &mut FigApp) -> bool {
    let Some(target) = app.ui.picker.clone() else {
        app.ui.picker_context = None;
        return false;
    };
    let Ok(ds) = app.session.ds() else {
        close_picker(app);
        return false;
    };
    let (uid, page) = (ds.uid, ds.page);
    if let Some((owner_target, owner_uid, _, image_owner)) = app.ui.picker_context.clone()
        && owner_uid != uid
        && (image_owner || document_bound(&owner_target))
    {
        finish_document(app, owner_uid);
        app.ui.picker = None;
        app.ui.picker_context = None;
        app.ui.picker_popover = None;
        app.ui.picker_button = None;
        app.ui.picker_button_id = None;
        return false;
    }
    if let Some((owner_target, owner_uid, owner_page, image_owner)) = app.ui.picker_context.clone()
        && owner_target == target
        && owner_uid == uid
        && image_owner
        && owner_page != page
    {
        close_picker(app);
        return false;
    }
    let image_owner =
        matches!(target, PickerTarget::Paint { .. }) && current_paint(app, &target).is_some_and(|paint| matches!(paint.kind, PaintKind::Image(_)));
    app.ui.picker_context = Some((target.clone(), uid, page, image_owner));

    let (fills, index, ids) = match &target {
        PickerTarget::Paint { fills, index, ids } => (*fills, *index, ids.clone()),
        _ => return true,
    };
    let original_is_image = ids.first().and_then(|id| app.session.doc().ok()?.node(*id)).is_some_and(|node| {
        let paints = if fills { &node.fills } else { &node.strokes };
        matches!(paints.get(index).map(|paint| &paint.kind), Some(PaintKind::Image(_)))
    });
    if !original_is_image {
        return true;
    }

    let selected = app.session.selection();
    // The popup was opened for these layers (several can share one image paint): keep editing them all.
    if same_layers(&ids, &selected) {
        return true;
    }
    if selected.len() != 1 {
        close_picker(app);
        return false;
    }
    let selected_id = selected[0];
    let Some(node) = app.session.doc().ok().and_then(|doc| doc.node(selected_id)) else {
        close_picker(app);
        return false;
    };
    let target_image = |fills: bool, index: usize| {
        let paints = if fills { &node.fills } else { &node.strokes };
        matches!(paints.get(index).map(|paint| &paint.kind), Some(PaintKind::Image(_)))
    };
    let (fills, index) = if target_image(fills, index) {
        (fills, index)
    } else if let Some(index) = node.fills.iter().position(|paint| matches!(paint.kind, PaintKind::Image(_))) {
        (true, index)
    } else if let Some(index) = node.strokes.iter().position(|paint| matches!(paint.kind, PaintKind::Image(_))) {
        (false, index)
    } else {
        close_picker(app);
        return false;
    };
    let next = PickerTarget::Paint { fills, index, ids: selected };
    if next == target {
        return true;
    }
    finish_with(app, false);
    app.ui.picker_context = Some((next.clone(), uid, page, true));
    app.ui.picker = Some(next);
    true
}

/// The selected gradient stop in the picker (shared with the on-canvas stops).
pub fn stop_id() -> Id {
    Id::unique("cp_stop")
}

/// Apply a swatch colour: a solid paint's colour, or the selected gradient stop's.
fn set_color(app: &mut FigApp, target: &PickerTarget, paint: &mut Paint, c: Color, stop: Id, ui: &Ui) {
    match &mut paint.kind {
        PaintKind::Solid { color } => {
            *color = c.with_alpha(1.0);
            paint.opacity = c.a;
        }
        PaintKind::Gradient(g) => {
            let sel: usize = ui.data(|d| d.get_temp(stop)).unwrap_or(0);
            if let Some(s) = g.stops.get_mut(sel) {
                s.color = c;
            }
        }
        PaintKind::Image(_) | PaintKind::Pattern(_) => return,
    }
    write(app, target, paint, false);
}

/// The page's most used solid colours, recomputed only when the document snapshot changes.
fn page_colors(app: &FigApp, ui: &Ui) -> Vec<Color> {
    let Ok(ds) = app.session.ds() else { return vec![] };
    let key = (std::sync::Arc::as_ptr(&ds.doc) as usize, ds.page);
    let cache_id = Id::unique("cp_doc_colors");
    if let Some((k, colors)) = ui.data(|d| d.get_temp::<((usize, figcraft_doc::NodeId), Vec<Color>)>(cache_id))
        && k == key
    {
        return colors;
    }
    let colors: Vec<Color> = figcraft_engine::cmd::styles::document_colors(&ds.doc, ds.page).into_iter().take(21).map(|(c, _)| c).collect();
    ui.data_mut(|d| d.insert_temp(cache_id, (key, colors.clone())));
    colors
}

/// Colour models the value row can show.
const MODELS: [&str; 7] = ["Hex", "RGB", "HSL", "HSB", "LCH", "OKLCH", "CSS"];

#[derive(Clone)]
struct FieldOwner {
    document: u64,
    target: PickerTarget,
    epoch: u64,
}

fn field_owner_id(ctx: &egui::Context) -> Id {
    Id::unique(("color_picker_field_owner", ctx.viewport_id()))
}

fn same_field_target(a: &PickerTarget, b: &PickerTarget) -> bool {
    match (a, b) {
        (PickerTarget::PluginField { plugin: ap, control: ac, .. }, PickerTarget::PluginField { plugin: bp, control: bc, .. }) => {
            ap == bp && ac == bc
        }
        _ => a == b,
    }
}

/// Each opening gets fresh draft/drag IDs, including a reopening in the same document. The
/// document or target changing while the popup stays open also invalidates its previous drafts.
fn begin_fields(app: &FigApp, ctx: &egui::Context, target: &PickerTarget, opening: bool) -> Option<u64> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let document = app.session.ds().map_or(0, |ds| ds.uid);
    let key = field_owner_id(ctx);
    let previous = ctx.data(|d| d.get_temp::<FieldOwner>(key));
    if !opening && let Some(previous) = previous.filter(|p| p.document == document && same_field_target(&p.target, target)) {
        return Some(previous.epoch);
    }
    let epoch = NEXT.try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1)).ok()?;
    ctx.data_mut(|d| {
        d.insert_temp(key, FieldOwner { document, target: target.clone(), epoch });
        d.insert_temp(key.with("epoch"), epoch);
    });
    Some(epoch)
}

fn context_field_id(ctx: &egui::Context, key: impl std::hash::Hash + std::fmt::Debug) -> Id {
    let owner = field_owner_id(ctx).with("epoch");
    let epoch = ctx.data(|d| d.get_temp::<u64>(owner)).unwrap_or(0);
    Id::unique(("color_picker_field", ctx.viewport_id(), epoch, key))
}

fn field_id(ui: &Ui, key: impl std::hash::Hash + std::fmt::Debug) -> Id {
    context_field_id(ui.ctx(), key)
}

#[cfg(test)]
pub(crate) fn current_field_id(ctx: &egui::Context, key: impl std::hash::Hash + std::fmt::Debug) -> Id {
    context_field_id(ctx, key)
}

/// The model dropdown with its fields and the alpha field. Returns a new colour and whether the
/// edit is done (false while a field is being scrubbed).
fn model_fields(ui: &mut Ui, c: Color) -> Option<(Color, bool)> {
    let model_id = Id::unique("cp_model");
    let model: usize = ui.data(|d| d.get_temp(model_id)).unwrap_or(0).min(MODELS.len() - 1);
    let mut out: Option<Color> = None;
    let mut done = true;
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.spacing_mut().item_spacing.x = 4.0;
        // "LCH" and "OKLCH" need a wider dropdown; their fields and alpha give up the room.
        let wide = matches!(MODELS.get(model).copied(), Some("LCH" | "OKLCH"));
        let resp = widgets::dropdown(ui, MODELS.get(model).copied().unwrap_or("Hex"), if wide { 66.0 } else { 52.0 }, None);
        crate::right::menu_popup(&resp).show(|ui| {
            for (i, m) in MODELS.iter().enumerate() {
                if crate::right::menu_item(ui, m, "") {
                    ui.data_mut(|d| d.insert_temp(model_id, i));
                    ui.close();
                }
            }
        });
        // `decimals` places shown; `step` is what a scrub or arrow moves by.
        let num = |ui: &mut Ui, id: &str, v: f64, w: f32, decimals: i32, step: f64| {
            let p = 10f64.powi(decimals);
            match widgets::number_field(ui, field_id(ui, ("cp_f", id)), widgets::Lead::None, Some((v * p).round() / p), "", w, step) {
                Some(widgets::FieldEdit::Set(x)) => Some((x, true)),
                Some(widgets::FieldEdit::Scrub(x, end)) => Some((x, end)),
                None => None,
            }
        };
        let name = MODELS.get(model).copied().unwrap_or("Hex");
        match name {
            "RGB" => {
                let [r, g, b, _] = c.to_rgba8();
                let vals = [f64::from(r), f64::from(g), f64::from(b)];
                for (k, v) in vals.iter().enumerate() {
                    if let Some((x, end)) = num(ui, ["r", "g", "b"][k.min(2)], *v, 31.0, 0, 1.0) {
                        done = end;
                        let mut rgb = vals;
                        if let Some(slot) = rgb.get_mut(k) {
                            *slot = x.clamp(0.0, 255.0);
                        }
                        out = Some(Color::new(rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0, c.a));
                    }
                }
            }
            "HSL" | "HSB" | "LCH" | "OKLCH" => {
                let Some(spec) = Polar::of(name) else { return };
                // The values last shown or typed stay while the colour is theirs: a chroma lowered to
                // fit sRGB, or a hue on a grey, isn't read back from the colour on the next frame.
                let kept_id = field_id(ui, "cp_polar");
                let rgba = c.to_rgba8();
                let vals = ui
                    .data(|d| d.get_temp::<(&'static str, [f64; 3], [u8; 4])>(kept_id))
                    .filter(|(m, _, k)| *m == name && *k == rgba)
                    .map_or_else(|| spec.values(c), |(_, v, _)| v);
                let mut kept = vals;
                for (k, (v, f)) in vals.iter().zip(spec.fields()).enumerate() {
                    let w = if !wide {
                        31.0
                    } else if f.decimals > 0 {
                        30.0
                    } else {
                        27.0
                    };
                    if let Some((x, end)) = num(ui, f.id, *v, w, f.decimals, f.step) {
                        done = end;
                        let mut v3 = vals;
                        if let Some(slot) = v3.get_mut(k) {
                            *slot = if f.wraps { x.rem_euclid(360.0) } else { x.clamp(0.0, f.max) };
                        }
                        let col = spec.color(v3).with_alpha(c.a);
                        kept = v3;
                        out = Some(col);
                    }
                }
                let shown = out.unwrap_or(c).to_rgba8();
                ui.data_mut(|d| d.insert_temp(kept_id, (name, kept, shown)));
            }
            "CSS" => {
                if let Some(text) = widgets::text_field(ui, field_id(ui, "cp_css"), &c.to_css(), 152.0, "") {
                    out = Color::parse_css(&text);
                }
            }
            _ => {
                if let Some(h) = widgets::text_field(ui, field_id(ui, "cp_hex"), &c.to_hex(), 101.0, "") {
                    out = Color::from_hex_or_name(&h).map(|h| h.with_alpha(if h.a < 1.0 { h.a } else { c.a }));
                }
            }
        }
        if MODELS.get(model) != Some(&"CSS") {
            let alpha = Some((f64::from(out.unwrap_or(c).a) * 100.0).round());
            let edit = match widgets::number_field(ui, field_id(ui, "cp_a"), widgets::Lead::None, alpha, "%", if wide { 40.0 } else { 44.0 }, 1.0) {
                Some(widgets::FieldEdit::Set(v)) => Some((v, true)),
                Some(widgets::FieldEdit::Scrub(v, end)) => Some((v, end)),
                None => None,
            };
            if let Some((v, end)) = edit {
                done = end;
                out = Some(out.unwrap_or(c).with_alpha((v / 100.0).clamp(0.0, 1.0) as f32));
            }
        }
    });
    out.map(|c| (c, done))
}

/// One field of a [`Polar`] model: its id, decimals shown, scrub/arrow step and range (0..=max,
/// or a hue that wraps at 360).
struct PolarField {
    id: &'static str,
    decimals: i32,
    step: f64,
    max: f64,
    wraps: bool,
}

const fn field(id: &'static str, decimals: i32, step: f64, max: f64) -> PolarField {
    PolarField { id, decimals, step, max, wraps: false }
}

const fn hue(id: &'static str) -> PolarField {
    PolarField { id, decimals: 0, step: 1.0, max: 360.0, wraps: true }
}

/// The three-field models: HSL, HSB, LCH and OKLCH, shown in their CSS units (percentages for
/// saturation and lightness, OKLCH lightness in percent).
#[derive(Clone, Copy)]
enum Polar {
    Hsl,
    Hsb,
    Lch,
    Oklch,
}

impl Polar {
    fn of(name: &str) -> Option<Self> {
        Some(match name {
            "HSL" => Self::Hsl,
            "HSB" => Self::Hsb,
            "LCH" => Self::Lch,
            "OKLCH" => Self::Oklch,
            _ => return None,
        })
    }

    const HSL_FIELDS: [PolarField; 3] = [hue("hsl_h"), field("hsl_s", 0, 1.0, 100.0), field("hsl_l", 0, 1.0, 100.0)];
    const HSB_FIELDS: [PolarField; 3] = [hue("hsb_h"), field("hsb_s", 0, 1.0, 100.0), field("hsb_b", 0, 1.0, 100.0)];
    const LCH_FIELDS: [PolarField; 3] = [field("lch_l", 0, 1.0, 100.0), field("lch_c", 0, 1.0, 230.0), hue("lch_h")];
    const OKLCH_FIELDS: [PolarField; 3] = [field("oklch_l", 0, 1.0, 100.0), field("oklch_c", 2, 0.01, 0.5), hue("oklch_h")];

    const fn fields(self) -> &'static [PolarField; 3] {
        match self {
            Self::Hsl => &Self::HSL_FIELDS,
            Self::Hsb => &Self::HSB_FIELDS,
            Self::Lch => &Self::LCH_FIELDS,
            Self::Oklch => &Self::OKLCH_FIELDS,
        }
    }

    fn values(self, c: Color) -> [f64; 3] {
        let v = match self {
            Self::Hsl => {
                let x = figcraft_color::Hsl::from_color(c);
                [x.h, x.s * 100.0, x.l * 100.0]
            }
            Self::Hsb => {
                let x = Hsb::from_color(c);
                [x.h, x.s * 100.0, x.b * 100.0]
            }
            Self::Lch => {
                let x = figcraft_color::Lch::from_color(c);
                [x.l, x.c, x.h]
            }
            Self::Oklch => {
                let x = figcraft_color::Oklch::from_color(c);
                [x.l * 100.0, x.c, x.h]
            }
        };
        v.map(f64::from)
    }

    /// Opaque.
    fn color(self, v: [f64; 3]) -> Color {
        let [a, b, d] = v.map(|v| v as f32);
        match self {
            Self::Hsl => figcraft_color::Hsl { h: a, s: b / 100.0, l: d / 100.0 }.to_color(),
            Self::Hsb => Hsb { h: a, s: b / 100.0, b: d / 100.0 }.to_color(),
            Self::Lch => figcraft_color::Lch { l: a, c: b, h: d }.to_color(),
            Self::Oklch => figcraft_color::Oklch { l: a / 100.0, c: b, h: d }.to_color(),
        }
    }
}

fn hue_color(ctx: &egui::Context, h: f32) -> Color32 {
    doc_c32(ctx, Hsb { h, s: 1.0, b: 1.0 }.to_color())
}

/// Draw the saturation/value square and its knob, updating `hsb.s` and `hsb.b` on click or
/// drag; returns `(changed, done)`.
fn sv_square(ui: &mut Ui, rect: Rect, hsb: &mut Hsb) -> (bool, bool) {
    // White → hue horizontally, then transparent → black vertically.
    let mut m = Mesh::default();
    let hc = hue_color(ui.ctx(), hsb.h);
    let corners = [(rect.left_top(), Color32::WHITE), (rect.right_top(), hc), (rect.right_bottom(), hc), (rect.left_bottom(), Color32::WHITE)];
    for (p, c) in corners {
        m.colored_vertex(p, c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(m);
    let mut m2 = Mesh::default();
    for (p, c) in [
        (rect.left_top(), Color32::TRANSPARENT),
        (rect.right_top(), Color32::TRANSPARENT),
        (rect.right_bottom(), Color32::BLACK),
        (rect.left_bottom(), Color32::BLACK),
    ] {
        m2.colored_vertex(p, c);
    }
    m2.add_triangle(0, 1, 2);
    m2.add_triangle(0, 2, 3);
    ui.painter().add(m2);
    let resp = ui.interact(rect, field_id(ui, "cp_sv"), Sense::click_and_drag());
    let mut changed = false;
    if (resp.dragged() || resp.clicked() || resp.drag_started())
        && let Some(p) = resp.interact_pointer_pos()
    {
        hsb.s = ((p.x - rect.min.x) / rect.width()).clamp(0.0, 1.0);
        hsb.b = 1.0 - ((p.y - rect.min.y) / rect.height()).clamp(0.0, 1.0);
        changed = true;
    }
    let knob = pos2(rect.min.x + hsb.s * rect.width(), rect.min.y + (1.0 - hsb.b) * rect.height());
    ui.painter().circle_stroke(knob, 6.0, Stroke::new(2.0, Color32::WHITE));
    ui.painter().circle_stroke(knob, 7.0, Stroke::new(1.0, Color32::from_black_alpha(80)));
    (changed, resp.drag_stopped() || resp.clicked())
}

/// Draw the hue gradient and knob, updating `hsb.h` clamped below 360; returns
/// `(changed, done)`.
fn hue_slider(ui: &mut Ui, rect: Rect, hsb: &mut Hsb) -> (bool, bool) {
    let n = 12;
    let mut m = Mesh::default();
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let x = rect.min.x + t * rect.width();
        let c = hue_color(ui.ctx(), t * 360.0);
        m.colored_vertex(pos2(x, rect.min.y), c);
        m.colored_vertex(pos2(x, rect.max.y), c);
        if i > 0 {
            let k = (i * 2) as u32;
            m.add_triangle(k - 2, k - 1, k);
            m.add_triangle(k - 1, k, k + 1);
        }
    }
    ui.painter().add(m);
    slider_knob(ui, rect, hsb.h / 360.0, field_id(ui, "cp_hue"))
        .map(|(t, done)| {
            hsb.h = (t * 360.0).clamp(0.0, 359.9);
            (true, done)
        })
        .unwrap_or((false, false))
}

fn slider_knob(ui: &mut Ui, rect: Rect, t: f32, id: Id) -> Option<(f32, bool)> {
    let resp = ui.interact(rect.expand(4.0), id, Sense::click_and_drag());
    let x = rect.min.x + t.clamp(0.0, 1.0) * rect.width();
    ui.painter().circle_stroke(pos2(x, rect.center().y), rect.height() / 2.0 + 1.0, Stroke::new(2.0, Color32::WHITE));
    ui.painter().circle_stroke(pos2(x, rect.center().y), rect.height() / 2.0 + 2.0, Stroke::new(1.0, Color32::from_black_alpha(80)));
    if (resp.dragged() || resp.clicked() || resp.drag_started())
        && let Some(p) = resp.interact_pointer_pos()
    {
        return Some((((p.x - rect.min.x) / rect.width()).clamp(0.0, 1.0), resp.drag_stopped() || resp.clicked()));
    }
    if resp.drag_stopped() {
        return Some((t, true));
    }
    None
}

/// Draw the checkerboard and transparent-to-opaque alpha gradient with a knob; returns the
/// new alpha and done state, or `None` when the slider is not interacting.
fn alpha_slider(ui: &mut Ui, rect: Rect, color: Color) -> Option<(f32, bool)> {
    widgets::checker(&ui.painter().with_clip_rect(rect), rect, rect.height() / 2.0);
    let mut m = Mesh::default();
    let opaque = doc_c32(ui.ctx(), color.with_alpha(1.0));
    for (p, c) in [
        (rect.left_top(), Color32::TRANSPARENT),
        (rect.right_top(), opaque),
        (rect.right_bottom(), opaque),
        (rect.left_bottom(), Color32::TRANSPARENT),
    ] {
        m.colored_vertex(p, c);
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    ui.painter().add(m);
    slider_knob(ui, rect, color.a, field_id(ui, "cp_alpha"))
}

/// The image files the picker offers.
const IMAGE_FILES: [&str; 7] = ["png", "jpg", "jpeg", "webp", "gif", "tif", "tiff"];

fn image_pending_id() -> Id {
    Id::unique("cp_image_pending")
}

/// Whether the Image tab of `target` shows its placeholder: chosen, with no file yet.
fn image_pending(ctx: &egui::Context, target: &PickerTarget) -> bool {
    ctx.data(|d| d.get_temp::<String>(image_pending_id())).is_some_and(|k| k == format!("{target:?}"))
}

/// Show the Image tab's placeholder for `target`, or (`None`) for nothing.
fn set_image_pending(ctx: &egui::Context, target: Option<&PickerTarget>) {
    ctx.data_mut(|d| {
        d.remove::<String>(image_pending_id());
        if let Some(t) = target {
            d.insert_temp(image_pending_id(), format!("{t:?}"));
        }
    });
}

/// `image.replace`'s parameters for layers' paint `index`.
fn image_params(fills: bool, index: usize, ids: &[NodeId]) -> serde_json::Value {
    json!({"ids": ids, "target": if fills { "fill" } else { "stroke" }, "index": index})
}

/// Where a file dragged over the window would become the open picker's image, and the parameters
/// that does it: the picker, while it shows layers' image paint or the Image tab's placeholder. A
/// platform doesn't say where a dragged file is until it can: an unknown position is the picker's.
pub(crate) fn image_drop_target(app: &FigApp, ctx: &egui::Context) -> Option<(Rect, serde_json::Value)> {
    let target = app.ui.picker.as_ref()?;
    let PickerTarget::Paint { fills, index, ids } = target else { return None };
    if !image_pending(ctx, target) && !matches!(current_paint(app, target)?.kind, PaintKind::Image(_)) {
        return None;
    }
    let rect = ctx.memory(|m| m.area_rect(window_id()))?;
    ctx.input(|i| i.pointer.hover_pos()).is_none_or(|p| rect.contains(p)).then(|| (rect, image_params(*fills, *index, ids)))
}

/// The bytes of a picked `file`: its `path`, or its base64 `data` where there are no paths.
fn picked_bytes(file: &serde_json::Value) -> Option<Vec<u8>> {
    use base64::Engine as _;
    match file["path"].as_str() {
        Some(path) => figcraft_engine::file_access::read_capped(path, figcraft_doc::ImageData::MAX_BYTES).ok(),
        None => base64::engine::general_purpose::STANDARD.decode(file["data"].as_str()?).ok(),
    }
}

/// Ask for an image file for `target`: layers' paints keep their fill mode, crop and position.
fn upload_image(app: &mut FigApp, target: &PickerTarget) {
    let target = target.clone();
    app.pick_file(tr("Choose image"), &IMAGE_FILES, true, move |app, file| {
        if let PickerTarget::Paint { fills, index, ids } = &target {
            let mut params = image_params(*fills, *index, ids);
            for key in ["path", "data"] {
                if let Some(v) = file.get(key) {
                    params[key] = v.clone();
                }
            }
            app.run("image.replace", params);
            return;
        }
        let Some(bytes) = picked_bytes(&file) else { return };
        let Some(hash) = app.session.doc_mut().ok().and_then(|d| figcraft_format_add(d, bytes)) else { return };
        if let Some(mut paint) = current_paint(app, &target) {
            paint.kind = Paint::image(hash, ImageScaleMode::Fill).kind;
            write(app, &target, &paint, false);
        }
    });
}

/// The image `hash` as a texture at most `side` pixels across, kept while it stays the one shown.
fn image_texture(app: &FigApp, ctx: &egui::Context, hash: &str, side: f64) -> Option<egui::TextureHandle> {
    let id = Id::unique("cp_image_texture");
    if let Some((_, tex)) = ctx.data(|d| d.get_temp::<(String, egui::TextureHandle)>(id)).filter(|(h, _)| h == hash) {
        return Some(tex);
    }
    let doc = app.session.doc().ok()?;
    let pix = figcraft_render::images::shown(hash, doc.images.get(hash), &ImageFilters::default(), doc.color_profile, side)?;
    let tex = ctx.load_texture("cp-image", crate::render_worker::screen_image(&pix, doc.color_profile), egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, (hash.to_string(), tex.clone())));
    Some(tex)
}

/// The asset preview: the paint's image, or checkers until there is one, and the upload button. A
/// file dropped on it takes the image's place (`image_drop_target`).
fn image_well(app: &mut FigApp, ui: &mut Ui, target: &PickerTarget, paint: &Paint, t: &Tokens) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 120.0), Sense::hover());
    let frame = r.shrink2(vec2(16.0, 0.0));
    widgets::checker(&ui.painter().with_clip_rect(frame), frame, 8.0);
    let ppp = f64::from(ui.ctx().pixels_per_point());
    if let PaintKind::Image(img) = &paint.kind
        && let Some(tex) = image_texture(app, ui.ctx(), &img.image, f64::from(frame.width()) * ppp)
    {
        let [tw, th] = tex.size().map(|v| v.max(1) as f32);
        let k = (frame.width() / tw).min(frame.height() / th);
        let shown = Rect::from_center_size(frame.center(), vec2(tw * k, th * k));
        ui.painter().image(tex.id(), shown, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    ui.painter().rect_stroke(frame, CornerRadius::same(4), Stroke::new(1.0, t.border), StrokeKind::Inside);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        if widgets::button(ui, tr("Upload from computer"), false, true).clicked() {
            upload_image(app, target);
        }
    });
}

/// The row of paint kinds (solid, gradients, image, pattern) with `paint`'s kind selected.
fn paint_type_row(ui: &mut Ui, app: &mut FigApp, target: &PickerTarget, paint: &mut Paint) {
    let kinds = [
        (Icon::Fill, tr("Solid")),
        (Icon::GradientLinear, tr("Linear")),
        (Icon::GradientRadial, tr("Radial")),
        (Icon::GradientAngular, tr("Angular")),
        (Icon::GradientDiamond, tr("Diamond")),
        (Icon::Image, tr("Image")),
        (Icon::ViewGrid, tr("Pattern")),
    ];
    let active = match &paint.kind {
        _ if image_pending(ui.ctx(), target) => 5,
        PaintKind::Solid { .. } => 0,
        PaintKind::Gradient(g) => match g.kind {
            GradientKind::Linear => 1,
            GradientKind::Radial => 2,
            GradientKind::Angular => 3,
            GradientKind::Diamond => 4,
        },
        PaintKind::Image(_) => 5,
        PaintKind::Pattern(_) => 6,
    };
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        if let Some(i) = widgets::segmented(ui, &kinds, Some(active), 30.0) {
            let base = paint.preview_color();
            let choosing = i == 5;
            set_image_pending(ui.ctx(), choosing.then_some(target).filter(|_| !matches!(paint.kind, PaintKind::Image(_))));
            paint.kind = match i {
                0 => PaintKind::Solid { color: base.with_alpha(1.0) },
                // Nothing changes until there is a file: the tab shows the placeholder.
                5 => paint.kind.clone(),
                6 => match pattern_sources(app, target).first() {
                    Some((id, _)) => Paint::pattern(id.0).kind,
                    None => {
                        app.toast(tr("Add a layer or component to tile first").to_string());
                        paint.kind.clone()
                    }
                },
                k => {
                    let kind = [GradientKind::Linear, GradientKind::Radial, GradientKind::Angular, GradientKind::Diamond][k - 1];
                    match &paint.kind {
                        PaintKind::Gradient(g) => {
                            PaintKind::Gradient(Gradient { kind, ..Gradient::default_for(kind, base).with_stops(g.stops.clone()) })
                        }
                        _ => PaintKind::Gradient(Gradient::default_for(kind, base)),
                    }
                }
            };
            if !choosing {
                write(app, target, paint, false);
            }
        }
    });
}

/// The paint's own blend mode dropdown.
fn blend_row(ui: &mut Ui, app: &mut FigApp, target: &PickerTarget, paint: &mut Paint) {
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let resp = widgets::dropdown(ui, tr(paint.blend.label()), 208.0, Some(Icon::Blend)).on_hover_text(tr("Blend mode"));
        crate::right::menu_popup(&resp).width(200.0).show(|ui| {
            let ctx = ui.ctx().clone();
            if let Some(b) = crate::right::blend_menu(ui, false, |b| preview_blend(app, &ctx, target, paint, b)) {
                paint.blend = b;
                write(app, target, paint, false);
            }
        });
    });
}

/// The image paint's scale mode, rotation, colour profile, presets and filter sliders.
fn image_controls(app: &mut FigApp, ui: &mut Ui, target: &PickerTarget, paint: &mut Paint, mut img: ImagePaint, t: &Tokens) {
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let resp = widgets::dropdown(ui, tr(&format!("{:?}", img.scale_mode)), 100.0, None);
        crate::right::menu_popup(&resp).show(|ui| {
            for (l, m) in [
                (tr("Fill"), ImageScaleMode::Fill),
                (tr("Fit"), ImageScaleMode::Fit),
                (tr("Crop"), ImageScaleMode::Crop),
                (tr("Tile"), ImageScaleMode::Tile),
            ] {
                if crate::right::menu_item(ui, l, "") {
                    img.scale_mode = m;
                    paint.kind = PaintKind::Image(img.clone());
                    write(app, target, paint, false);
                    ui.close();
                }
            }
        });
        if icons::button(ui, Icon::Rotate, 24.0, false, tr("Rotate 90°")).clicked() {
            img.rotation = (img.rotation + 1) % 4;
            paint.kind = PaintKind::Image(img.clone());
            write(app, target, paint, false);
        }
    });
    // The image's own colour profile (decoding converts it into the file's).
    let source = app.session.doc().ok().and_then(|d| figcraft_render::images::profile_name(&img.image, d.images.get(&img.image)));
    let source = match source.as_deref() {
        None => tr("Unknown — treated as sRGB"),
        Some("") => tr("Embedded profile"),
        Some(name) => name,
    };
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let text = trf("Color profile: {name}", &[("name", &source)]);
        ui.add(egui::Label::new(egui::RichText::new(text).font(regular(11.0)).color(t.text_secondary)).truncate());
    });
    image_presets(app, ui, target, paint, img.filters);
    for (label, val) in [
        (tr_noop("Exposure"), img.filters.exposure),
        (tr_noop("Contrast"), img.filters.contrast),
        (tr_noop("Saturation"), img.filters.saturation),
        (tr_noop("Temperature"), img.filters.temperature),
        (tr_noop("Tint"), img.filters.tint),
        (tr_noop("Highlights"), img.filters.highlights),
        (tr_noop("Shadows"), img.filters.shadows),
    ] {
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            // Labels share a column so the sliders line up.
            let (lr, _) = ui.allocate_exact_size(vec2(72.0, 18.0), Sense::hover());
            ui.painter().text(lr.left_center(), Align2::LEFT_CENTER, tr(label), regular(11.0), t.text);
            let mut v = val;
            let r = ui.add(egui::Slider::new(&mut v, -1.0..=1.0).show_value(false));
            if r.changed() {
                let f = &mut img.filters;
                match label {
                    "Exposure" => f.exposure = v,
                    "Contrast" => f.contrast = v,
                    "Saturation" => f.saturation = v,
                    "Temperature" => f.temperature = v,
                    "Tint" => f.tint = v,
                    "Highlights" => f.highlights = v,
                    _ => f.shadows = v,
                }
                paint.kind = PaintKind::Image(img.clone());
                write(app, target, paint, !r.drag_stopped());
            }
            if r.drag_stopped() {
                finish(app);
            }
        });
    }
}

/// The gradient bar with its stops, the stop actions and the selected stop's colour variable.
/// The selected stop's colour (what the colour fields then edit).
fn gradient_controls(
    app: &mut FigApp,
    ui: &mut Ui,
    target: &PickerTarget,
    paint: &mut Paint,
    mut g: Gradient,
    t: &Tokens,
    sel_stop_id: Id,
) -> Option<Color> {
    let mut sel: usize = ui.data(|d| d.get_temp(sel_stop_id)).unwrap_or(0).min(g.stops.len().saturating_sub(1));
    let (bar, _) = ui.allocate_exact_size(vec2(240.0, 28.0), Sense::hover());
    let bar = bar.shrink2(vec2(16.0, 6.0));
    widgets::checker(&ui.painter().with_clip_rect(bar), bar, 4.0);
    let n = 24;
    for i in 0..n {
        let a = i as f32 / n as f32;
        let b = (i + 1) as f32 / n as f32;
        let r = Rect::from_min_max(pos2(bar.min.x + a * bar.width(), bar.min.y), pos2(bar.min.x + b * bar.width(), bar.max.y));
        ui.painter().rect_filled(r, CornerRadius::ZERO, doc_c32(ui.ctx(), g.sample((a + b) / 2.0)));
    }
    let mut changed = false;
    let mut done = false;
    // The bar first, so the stops on it take their own clicks.
    let bresp = ui.interact(bar, field_id(ui, "cp_bar"), Sense::click());
    for (i, s) in g.stops.clone().iter().enumerate() {
        let x = bar.min.x + s.position * bar.width();
        let r = Rect::from_center_size(pos2(x, bar.center().y), vec2(12.0, 18.0));
        let resp = ui.interact(r, field_id(ui, ("cp_stopk", i)), Sense::click_and_drag());
        ui.painter().rect_filled(r, CornerRadius::same(3), doc_c32(ui.ctx(), s.color.with_alpha(1.0)));
        ui.painter().rect_stroke(
            r,
            CornerRadius::same(3),
            Stroke::new(if i == sel { 2.0 } else { 1.0 }, if i == sel { crate::theme::accent(ui.ctx()) } else { Color32::WHITE }),
            StrokeKind::Outside,
        );
        if resp.clicked() || resp.drag_started() {
            sel = i;
        }
        if resp.dragged()
            && let Some(p) = resp.interact_pointer_pos()
            && let Some(st) = g.stops.get_mut(i)
        {
            st.position = ((p.x - bar.min.x) / bar.width()).clamp(0.0, 1.0);
            changed = true;
        }
        if resp.drag_stopped() {
            done = true;
        }
    }
    // Click on the bar adds a stop.
    if bresp.clicked()
        && let Some(p) = bresp.interact_pointer_pos()
    {
        let pos = ((p.x - bar.min.x) / bar.width()).clamp(0.0, 1.0);
        g.stops.push(ColorStop { position: pos, color: g.sample(pos) });
        sel = g.stops.len() - 1;
        changed = true;
        done = true;
    }
    ui.data_mut(|d| d.insert_temp(sel_stop_id, sel));
    if changed {
        paint.kind = PaintKind::Gradient(g.clone());
        write(app, target, paint, !done);
    }
    if done {
        finish(app);
    }
    let edit_color = g.stops.get(sel).map(|s| s.color);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        if ui.small_button(tr("Remove stop")).clicked() && g.stops.len() > 2 {
            g.stops.remove(sel);
            ui.data_mut(|d| d.insert_temp(sel_stop_id, 0usize));
            paint.kind = PaintKind::Gradient(g.clone());
            write(app, target, paint, false);
        }
        if ui.small_button(tr("Flip")).clicked() {
            for s in &mut g.stops {
                s.position = 1.0 - s.position;
            }
            paint.kind = PaintKind::Gradient(g.clone());
            write(app, target, paint, false);
        }
        if ui.small_button(tr("Rotate 90°")).clicked() {
            let (fx, fy) = g.from;
            let rot = |(x, y): (f64, f64)| (fx - (y - fy), fy + (x - fx));
            g.to = rot(g.to);
            g.width = rot(g.width);
            paint.kind = PaintKind::Gradient(g.clone());
            write(app, target, paint, false);
        }
    });
    // The selected stop's colour can come from a colour variable.
    if let PickerTarget::Paint { fills, index, ids } = target {
        let field = format!("{}/{index}/stops/{sel}/color", if *fills { "fills" } else { "strokes" });
        let stop = g.stops.get(sel).map(|s| s.color);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            crate::variable_ui::bindable(app, ui, ids, &field, figcraft_doc::VariableType::Color, 208.0, |_, ui| {
                let (r, _) = ui.allocate_exact_size(vec2(208.0, widgets::FIELD_H), Sense::hover());
                ui.painter().rect_filled(r, CornerRadius::same(5), t.field);
                let sw = Rect::from_center_size(pos2(r.min.x + 13.0, r.center().y), vec2(14.0, 14.0));
                if let Some(c) = stop {
                    widgets::paint_swatch(ui.painter(), sw, &Paint::solid(c));
                    let text = trf("Stop {n}  {hex}", &[("n", &(sel + 1)), ("hex", &c.to_hex())]);
                    ui.painter().text(pos2(sw.max.x + 8.0, r.center().y), Align2::LEFT_CENTER, text, regular(11.0), t.text);
                }
            });
        });
    }
    edit_color
}

/// The colour picker window's area id. A fixed id (not `Window::new`'s, which egui keys by
/// `Id::unique(Some(title))`) so the style picker can tell a click inside the picker from one outside.
pub fn window_id() -> Id {
    Id::unique("color_picker")
}

/// Draw the colour picker for the current target: close it when there is no target or the
/// target's selection no longer matches, otherwise apply eyedropper, paint-kind, colour and
/// gradient-stop edits live, positioned beside a plugin field when applicable.
pub fn show(app: &mut FigApp, ctx: &egui::Context) {
    let previous_target = app.ui.picker.clone();
    if !sync_picker_target(app) {
        // Synchronization can close the target before the ordinary no-target branch below.
        // An eyedropper started by this picker belongs to it and must end on that path too.
        app.session.tool.picked = None;
        if app.session.tool.pick_only {
            app.run("tool.move", json!({}));
        }
        return;
    }
    let Some(target) = app.ui.picker.clone() else {
        // A colour sampled for a picker that has closed has no paint to go to.
        app.session.tool.picked = None;
        if app.session.tool.pick_only {
            app.run("tool.move", json!({}));
        }
        return;
    };
    // A layer's picker belongs to the selection it was opened for: once that changes, close it
    // rather than keep editing layers that are no longer selected.
    let stale = match &target {
        PickerTarget::Paint { ids, .. } | PickerTarget::Effect { ids, .. } => !same_layers(ids, &app.session.selection()),
        PickerTarget::TextRange { id, .. } => !app.session.selection().contains(id),
        _ => false,
    };
    let current = if stale { None } else { current_paint(app, &target) };
    let Some(mut paint) = current else {
        finish_with(app, false);
        app.ui.picker = None;
        app.ui.picker_popover = None;
        return;
    };
    let Some(field_epoch) = begin_fields(app, ctx, &target, app.ui.picker_popover.is_none()) else {
        close_picker(app);
        return;
    };
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    // A plugin's colour field: beside its window, which sits where the picker otherwise would.
    let beside = match &target {
        PickerTarget::PluginField { plugin, .. } => crate::plugins_ui::field_window(app, ctx, plugin).map(|r| pos2(r.min.x - 248.0, r.min.y)),
        _ => None,
    };
    let open = beside.map(Open::At).unwrap_or_else(|| {
        let top = app.ui.picker_button.map_or(140.0, |r| r.min.y);
        // Beside the Design panel, on its canvas side (its right in a mirrored window).
        let panel = app.ui.right_panel_rect.map_or_else(
            || {
                if app.ui.mirrored {
                    egui::Rangef::new(screen.min.x, screen.min.x + app.ui.right_width)
                } else {
                    egui::Rangef::new(screen.max.x - app.ui.right_width, screen.max.x)
                }
            },
            |r| r.x_range(),
        );
        if app.ui.mirrored { Open::AnchorMirrored(pos2(panel.max + 8.0, top)) } else { Open::Anchor(pos2(panel.min - 8.0, top)) }
    });
    // The colour picker's own header — title 16 in, the X 8 in from the right — is the shared
    // popover's chrome now; the picker keeps its body. Its area id is [`window_id`], the one the
    // style picker looks up to tell a press in here from one outside.
    let mut pop = app.ui.picker_popover.take().unwrap_or_else(|| Popover::with_id(window_id(), tr("Custom")).pad(0.0));
    // Sampling a colour from the canvas — the eyedropper this picker's own button started, or the
    // colour it just sampled — is the press, not a press outside: neither may close the picker.
    let picking = app.session.tool.pick_only || app.session.tool.picked.is_some();
    // Nor may a press on the open gradient's handles or stops on the canvas: they are this picker's
    // controls, and the canvas took the press before the picker drew (#1852).
    let own = picking || crate::handles::gradient_dragging(ctx);
    pop = pop.close_on_outside(!own);
    // Only the opening control is held out: any other sidebar or canvas press dismisses it.
    let mut held: Vec<Rect> = app.ui.picker_button.into_iter().collect();
    // The canvas selects before this panel draws. Image adjustment remains available while
    // selecting or dragging an image; a changed/empty non-image selection was closed above.
    if matches!(paint.kind, PaintKind::Image(_)) {
        held.push(app.canvas.rect);
        if previous_target.as_ref() != Some(&target)
            && let Some(at) = ctx.input(|i| i.pointer.interact_pos())
        {
            held.push(Rect::from_center_size(at, vec2(2.0, 2.0)));
        }
    }
    held.extend(crate::variable_ui::picker_rect(ctx));
    let closed = pop.show(ctx, open, Hold { rects: &held, escape: false }, |ui| {
        ui.push_id(field_epoch, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            // The eyedropper's colour goes to this picker's paint (a stroke, an effect, a gradient stop...).
            if let Some(c) = app.session.tool.picked.take() {
                set_color(app, &target, &mut paint, c, stop_id(), ui);
            }
            // Paint type.
            if !matches!(
                target,
                PickerTarget::Effect { .. } | PickerTarget::PageBackground | PickerTarget::Variable { .. } | PickerTarget::PluginField { .. }
            ) {
                paint_type_row(ui, app, &target, &mut paint);
            }
            // The paint's own blend mode, with the paints and layers under it.
            if !matches!(
                target,
                PickerTarget::Effect { .. } | PickerTarget::PageBackground | PickerTarget::Variable { .. } | PickerTarget::PluginField { .. }
            ) {
                blend_row(ui, app, &target, &mut paint);
            }
            // Gradient stops.
            let is_image = matches!(paint.kind, PaintKind::Image(_));
            if is_image {
                set_image_pending(ui.ctx(), None);
            }
            // Until a file is chosen the Image tab shows its placeholder, not the paint it will replace.
            let placeholder = image_pending(ui.ctx(), &target);
            let mut edit_color: Option<Color> = match &paint.kind {
                PaintKind::Solid { color } if !placeholder => Some(color.with_alpha(color.a * paint.opacity)),
                _ => None,
            };
            let sel_stop_id = stop_id();
            if let (PaintKind::Gradient(g), false) = (paint.kind.clone(), placeholder) {
                edit_color = gradient_controls(app, ui, &target, &mut paint, g, &t, sel_stop_id);
            }
            if is_image || placeholder {
                image_well(app, ui, &target, &paint, &t);
            }
            if let PaintKind::Image(img) = paint.kind.clone() {
                image_controls(app, ui, &target, &mut paint, img, &t);
            }
            if let PaintKind::Pattern(pt) = paint.kind.clone() {
                pattern_controls(app, ui, &target, &mut paint, pt);
            }
            if let Some(color) = edit_color {
                let mut hsb = ui
                    .data(|d| d.get_temp::<(f32, f32, f32, [u8; 4])>(Id::unique("cp_hsb")))
                    .filter(|(_, _, _, rgba)| *rgba == color.to_rgba8())
                    .map(|(h, s, b, _)| Hsb { h, s, b })
                    .unwrap_or_else(|| Hsb::from_color(color));
                let (sq, _) = ui.allocate_exact_size(vec2(240.0, 240.0), Sense::hover());
                let (c1, d1) = sv_square(ui, sq, &mut hsb);
                let (hr, _) = ui.allocate_exact_size(vec2(240.0, 28.0), Sense::hover());
                let hr = Rect::from_min_max(pos2(hr.min.x + 48.0, hr.min.y + 8.0), pos2(hr.max.x - 16.0, hr.max.y - 8.0));
                if icons::button(
                    &mut ui.new_child(
                        egui::UiBuilder::new().max_rect(Rect::from_min_size(pos2(hr.min.x - 36.0, hr.center().y - 12.0), vec2(24.0, 24.0))),
                    ),
                    Icon::Eyedropper,
                    24.0,
                    false,
                    tr("Pick color (I)"),
                )
                .clicked()
                {
                    app.run("edit.pickColor", json!({"apply": false}));
                    return;
                }
                let (c2, d2) = hue_slider(ui, hr, &mut hsb);
                let (ar, _) = ui.allocate_exact_size(vec2(240.0, 20.0), Sense::hover());
                let ar = Rect::from_min_max(pos2(ar.min.x + 48.0, ar.min.y + 2.0), pos2(ar.max.x - 16.0, ar.max.y - 6.0));
                let mut new_alpha = None;
                if let Some((a, d)) = alpha_slider(ui, ar, color) {
                    new_alpha = Some((a, d));
                }
                let mut nc = hsb.to_color().with_alpha(color.a);
                let mut done = d1 || d2;
                let mut changed = c1 || c2;
                if let Some((a, d)) = new_alpha {
                    nc = nc.with_alpha(a);
                    changed = true;
                    done |= d;
                }
                // Colour model fields (Hex, RGB, HSL, HSB, LCH, OKLCH or CSS) + alpha.
                if let Some((c, d)) = model_fields(ui, nc) {
                    nc = c;
                    hsb = Hsb::from_color(nc);
                    changed = true;
                    done |= d;
                }
                ui.data_mut(|d| d.insert_temp(Id::unique("cp_hsb"), (hsb.h, hsb.s, hsb.b, nc.to_rgba8())));
                // A Display P3 colour sRGB can't show: the canvas (sRGB) shows the nearest one.
                if app.session.doc().is_ok_and(|d| d.color_profile.conversion_to(figcraft_color::ColorProfile::Srgb).clips(nc)) {
                    gamut_warning(ui, &t);
                }
                if changed {
                    match &mut paint.kind {
                        PaintKind::Solid { color } => {
                            *color = nc.with_alpha(1.0);
                            paint.opacity = nc.a;
                        }
                        PaintKind::Gradient(g) => {
                            let sel: usize = ui.data(|d| d.get_temp(sel_stop_id)).unwrap_or(0);
                            if let Some(s) = g.stops.get_mut(sel) {
                                s.color = nc;
                            }
                        }
                        PaintKind::Image(_) | PaintKind::Pattern(_) => {}
                    }
                    write(app, &target, &paint, !done);
                }
                if done {
                    finish(app);
                }
            }
            // Colours used on this page.
            let doc_colors = page_colors(app, ui);
            if !doc_colors.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    widgets::label(ui, tr("On this page"), t.text_secondary);
                });
                // Inset on both sides: `add_space` would only indent the first row of a wrap.
                inset(ui, |ui| {
                    for c in &doc_colors {
                        if widgets::swatch(ui, &Paint::solid(*c), 14.0).on_hover_text(c.to_hex()).clicked() {
                            set_color(app, &target, &mut paint, *c, sel_stop_id, ui);
                        }
                    }
                });
            }
            // Recent colours.
            let recent = app.session.recent_colors.clone();
            if !recent.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    widgets::label(ui, tr("Recent"), t.text_secondary);
                });
                inset(ui, |ui| {
                    for c in recent.iter().take(14) {
                        if widgets::swatch(ui, &Paint::solid(*c), 14.0).clicked() {
                            set_color(app, &target, &mut paint, *c, sel_stop_id, ui);
                        }
                    }
                });
            }
            ui.add_space(12.0);
        });
    });
    if closed {
        set_image_pending(ctx, None);
        close(app);
    } else if app.ui.picker.is_some() {
        // Kept only while the picker is open: a body that closed it (the eyedropper) doesn't leave
        // a stale place for the next picker to inherit.
        app.ui.picker_popover = Some(pop);
    }
}

/// Escape with the picker open closes it (the selection stays); false when it was not open, so the
/// key goes on to the binding that deselects.
pub fn close_if_open(app: &mut FigApp) -> bool {
    if app.ui.picker.is_none() {
        return false;
    }
    close(app);
    true
}

/// The colour picker closes (its X, a press outside it, Escape, or another popover took over):
/// pending edits commit and the picker with its popover go.
pub fn close(app: &mut FigApp) {
    finish(app);
    app.ui.picker_context = None;
    app.ui.picker = None;
    app.ui.picker_popover = None;
    app.ui.picker_button = None;
    app.ui.picker_button_id = None;
}

/// Swatches wrapped in rows inset 16 points from both sides of the picker.
fn inset(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 0)).show(ui, |ui| ui.horizontal_wrapped(add));
}

/// The colour picker's note under the fields for a colour outside sRGB.
fn gamut_warning(ui: &mut Ui, t: &Tokens) {
    use crate::a11y::Tip;
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
        icons::paint(ui.painter(), r, Icon::GamutWarning, t.icon_secondary);
        widgets::label(ui, "Outside sRGB", t.text_secondary);
    })
    .response
    .tip("sRGB screens and files can't show this Display P3 color: the canvas shows the nearest sRGB color, P3 exports keep it");
}

/// Layers a pattern can tile: the file's components and the current page's top-level layers, but
/// the painted layers, what they contain and what contains them (at most 64).
fn pattern_sources(app: &FigApp, target: &PickerTarget) -> Vec<(NodeId, String)> {
    let Ok(doc) = app.session.doc() else { return Vec::new() };
    let painted = match target {
        PickerTarget::Paint { ids, .. } => ids.clone(),
        PickerTarget::SelectionColor { targets, .. } => targets.iter().map(|t| t.id).collect(),
        PickerTarget::TextRange { id, .. } => vec![*id],
        _ => Vec::new(),
    };
    let top = app.session.page().ok().and_then(|p| doc.node(p)).map(|p| p.children.clone()).unwrap_or_default();
    let comps = doc.nodes().filter(|n| matches!(n.kind, NodeKind::Component { .. })).map(|n| n.id);
    let mut out: Vec<(NodeId, String)> = Vec::new();
    for id in comps.chain(top) {
        let related = painted.iter().any(|t| doc.is_ancestor(*t, id) || doc.is_ancestor(id, *t));
        if !related
            && !out.iter().any(|(o, _)| *o == id)
            && let Some(n) = doc.node(id)
        {
            out.push((id, n.name.clone()));
        }
        if out.len() >= 64 {
            break;
        }
    }
    out
}

/// The pattern tab: source, tile type, alignment, scale and spacing.
fn pattern_controls(app: &mut FigApp, ui: &mut Ui, target: &PickerTarget, paint: &mut Paint, mut pt: PatternPaint) {
    use widgets::{FieldEdit, Lead};
    // Some(live): changed, as part of a drag when live.
    let mut changed: Option<bool> = None;
    let name = app.session.doc().ok().and_then(|d| d.node(NodeId(pt.source)).map(|n| n.name.clone())).unwrap_or_else(|| tr("Missing layer").into());
    let sources = pattern_sources(app, target);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let resp = widgets::dropdown(ui, &name, 208.0, None);
        crate::a11y::combo(&resp, tr("Pattern source"), &name, crate::a11y::menu_open(&resp));
        crate::right::menu_popup(&resp).show(|ui| {
            for (id, label) in &sources {
                if crate::right::menu_check(ui, id.0 == pt.source, label, "") {
                    pt.source = id.0;
                    changed = Some(false);
                    ui.close();
                }
            }
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let tiles =
            [(tr("Grid"), PatternTile::Grid), (tr("Horizontal hex"), PatternTile::HorizontalHex), (tr("Vertical hex"), PatternTile::VerticalHex)];
        let label = tiles.iter().find(|t| t.1 == pt.tile).map_or(tr("Grid"), |t| t.0);
        let resp = widgets::dropdown(ui, label, 120.0, Some(Icon::ViewGrid));
        crate::a11y::combo(&resp, tr("Tile type"), label, crate::a11y::menu_open(&resp));
        crate::right::menu_popup(&resp).show(|ui| {
            for (l, v) in tiles {
                if crate::right::menu_check(ui, v == pt.tile, l, "") {
                    pt.tile = v;
                    changed = Some(false);
                    ui.close();
                }
            }
        });
        let aligns = [(tr("Start"), PatternAlign::Start), (tr("Center"), PatternAlign::Center), (tr("End"), PatternAlign::End)];
        let label = aligns.iter().find(|a| a.1 == pt.align).map_or(tr("Start"), |a| a.0);
        let resp = widgets::dropdown(ui, label, 80.0, None);
        crate::a11y::combo(&resp, tr("Pattern alignment"), label, crate::a11y::menu_open(&resp));
        crate::right::menu_popup(&resp).show(|ui| {
            for (l, v) in aligns {
                if crate::right::menu_check(ui, v == pt.align, l, "") {
                    pt.align = v;
                    changed = Some(false);
                    ui.close();
                }
            }
        });
    });
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let field = |ui: &mut Ui, key: &str, lead: Lead, value: f64, suffix: &str| match widgets::number_field(
            ui,
            field_id(ui, key),
            lead,
            Some(value),
            suffix,
            64.0,
            1.0,
        ) {
            Some(FieldEdit::Set(v)) => Some((v, false)),
            Some(FieldEdit::Scrub(v, end)) => Some((v, !end)),
            None => None,
        };
        if let Some((v, live)) = field(ui, "cp_pattern_scale", Lead::Icon(Icon::Scale, tr("Pattern scale")), pt.scale * 100.0, "%") {
            let (lo, hi) = PatternPaint::SCALE_RANGE;
            pt.scale = (v / 100.0).clamp(lo, hi);
            changed = Some(live);
        }
        if let Some((v, live)) = field(ui, "cp_pattern_gap_x", Lead::Icon(Icon::PaddingH, tr("Horizontal spacing")), pt.spacing.0, "") {
            pt.spacing.0 = v.clamp(0.0, PatternPaint::MAX_SPACING);
            changed = Some(live);
        }
        if let Some((v, live)) = field(ui, "cp_pattern_gap_y", Lead::Icon(Icon::PaddingV, tr("Vertical spacing")), pt.spacing.1, "") {
            pt.spacing.1 = v.clamp(0.0, PatternPaint::MAX_SPACING);
            changed = Some(live);
        }
    });
    if let Some(live) = changed {
        paint.kind = PaintKind::Pattern(pt);
        write(app, target, paint, live);
        if !live {
            finish(app);
        }
    }
}

/// What the image preset row was asked to do.
enum PresetAction {
    Apply(String),
    Delete(String),
}

/// A saved preset in the presets menu: click applies it, the trailing button deletes it.
fn saved_preset_row(ui: &mut Ui, name: &str) -> Option<PresetAction> {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width().max(180.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click());
    let del = Rect::from_center_size(pos2(r.max.x - 14.0, r.center().y), vec2(20.0, 20.0));
    let del_resp = ui.interact(del, ui.scope_id().with(("cp_preset_del", name)), Sense::click()).tip(tr("Delete preset"));
    let hot = resp.hovered() || del_resp.hovered();
    if hot {
        ui.painter().rect_filled(r, CornerRadius::same(5), t.menu_hover);
    }
    let col = if hot { Color32::WHITE } else { t.text };
    ui.painter().text(pos2(r.min.x + 8.0, r.center().y), Align2::LEFT_CENTER, name, regular(11.5), col);
    if hot {
        icons::paint(ui.painter(), del.shrink(4.0), Icon::Close, col);
    }
    if del_resp.clicked() {
        Some(PresetAction::Delete(name.to_string()))
    } else if resp.clicked() {
        Some(PresetAction::Apply(name.to_string()))
    } else {
        None
    }
}

/// The image paint's adjustment presets: a menu of the built-in presets and the file's saved ones
/// (named after the one in effect), "Save as preset…" and Reset.
fn image_presets(app: &mut FigApp, ui: &mut Ui, target: &PickerTarget, paint: &mut Paint, filters: ImageFilters) {
    let saved: Vec<(String, ImageFilters)> =
        app.session.doc().map(|d| d.image_presets.iter().map(|p| (p.name.clone(), p.filters)).collect()).unwrap_or_default();
    let built_in = ImageFilters::PRESETS.iter().filter(|(id, ..)| *id != "reset");
    let current = built_in
        .clone()
        .map(|(_, label, f)| (tr(label).to_string(), *f))
        .chain(saved.iter().cloned())
        .find(|(_, f)| *f == filters)
        .map(|(name, _)| name)
        .unwrap_or_else(|| if filters.is_identity() { tr("No preset").into() } else { tr("Custom").into() });
    let name_id = field_id(ui, "cp_preset_name");
    let mut naming: Option<String> = ui.data(|d| d.get_temp(name_id));
    let mut action = None;
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        let resp = widgets::dropdown(ui, &current, 152.0, None).on_hover_text(tr("Adjustment presets"));
        crate::right::menu_popup(&resp).show(|ui| {
            for (id, label, _) in built_in {
                if crate::right::menu_item(ui, tr(label), "") {
                    action = Some(PresetAction::Apply(id.to_string()));
                    ui.close();
                }
            }
            ui.separator();
            for (name, _) in &saved {
                if let Some(a) = saved_preset_row(ui, name) {
                    action = Some(a);
                    ui.close();
                }
            }
            if !saved.is_empty() {
                ui.separator();
            }
            if crate::right::menu_item(ui, tr("Save as preset…"), "") {
                naming = Some(format!("Preset {}", saved.len() + 1));
                ui.close();
            }
        });
        if widgets::button(ui, tr("Reset"), false, !filters.is_identity()).on_hover_text(tr("Reset adjustments")).clicked() {
            action = Some(PresetAction::Apply("reset".into()));
        }
    });
    if let Some(name) = naming.as_mut() {
        let mut save = false;
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let r = ui.add(egui::TextEdit::singleline(name).id(name_id.with("te")).font(regular(11.0)).desired_width(136.0));
            crate::widgets::select_all_on_focus(ui, &r, name);
            if !r.has_focus() && !crate::a11y::field_focused(ui.ctx()) {
                r.request_focus();
            }
            save = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            save |= widgets::button(ui, tr("Save"), true, !name.trim().is_empty()).clicked();
        });
        if save && !name.trim().is_empty() {
            app.run("image.savePreset", json!({"name": name.trim(), "filters": filters}));
            naming = None;
        }
    }
    ui.data_mut(|d| match &naming {
        Some(n) => {
            d.insert_temp(name_id, n.clone());
        }
        None => d.remove::<String>(name_id),
    });
    match action {
        Some(PresetAction::Delete(name)) => {
            app.run("image.deletePreset", json!({"name": name}));
        }
        Some(PresetAction::Apply(name)) => match target {
            // Layer paints go through the command; other targets (a text range, a style) get the
            // preset's values written like any edit.
            PickerTarget::Paint { fills, index, ids } => {
                app.run("image.adjust", json!({"ids": ids, "target": if *fills { "fill" } else { "stroke" }, "index": index, "preset": name}));
            }
            _ => {
                let preset = app.session.doc().ok().and_then(|d| figcraft_engine::cmd::image::find_preset(d, &name));
                if let (Some(f), PaintKind::Image(img)) = (preset, &mut paint.kind) {
                    img.filters = f;
                    write(app, target, paint, false);
                }
            }
        },
        None => {}
    }
}

fn figcraft_format_add(doc: &mut figcraft_doc::Document, bytes: Vec<u8>) -> Option<String> {
    // Images are keyed by content hash; the engine's image.place uses the same scheme.
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(&bytes)).with_guessed_format().ok()?.into_dimensions().ok()?;
    let hash = {
        use std::hash::{Hash, Hasher};
        let mut s = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut s);
        format!("img{:016x}{:08x}", s.finish(), bytes.len())
    };
    doc.images.insert(hash.clone(), figcraft_doc::ImageData { bytes: std::sync::Arc::new(bytes), width: w, height: h });
    Some(hash)
}

trait WithStops {
    fn with_stops(self, stops: Vec<ColorStop>) -> Self;
}

impl WithStops for Gradient {
    fn with_stops(mut self, stops: Vec<ColorStop>) -> Self {
        if stops.len() >= 2 {
            self.stops = stops;
        }
        self
    }
}

#[cfg(test)]
mod selection_color_tests {
    use super::*;
    use crate::NoServices;
    use figcraft_engine::{Session, selection_colors::collect};

    fn setup() -> (FigApp, NodeId, NodeId) {
        let mut s = Session::new();
        let a = NodeId(s.execute("shape.rectangle", &json!({"width":40,"height":40,"props":{"fill":"#ff0000"}})).unwrap()["id"].as_u64().unwrap());
        let b = NodeId(s.execute("shape.rectangle", &json!({"width":40,"height":40,"props":{"fill":"#0000ff"}})).unwrap()["id"].as_u64().unwrap());
        s.execute("select.set", &json!({"ids":[a,b]})).unwrap();
        let mut app = FigApp::new(s, Box::new(NoServices));
        let targets = collect(app.session.doc().unwrap(), &[a, b])[0].targets.clone();
        crate::selection_colors::toggle(&mut app, targets, None);
        (app, a, b)
    }

    #[test]
    fn picker_live_changes_keep_frozen_targets_and_commit_once() {
        let (mut app, a, b) = setup();
        let target = app.ui.picker.clone().unwrap();
        let before = app.session.undo_depth();
        for color in ["#0000ff", "#00ff00"] {
            write(&mut app, &target, &Paint::solid(Color::from_hex(color).unwrap()), true);
        }
        assert!(app.session.recent_colors.is_empty(), "previews must not be recorded");
        finish(&mut app);
        assert_eq!(app.session.recent_colors, vec![Color::from_hex("#00ff00").unwrap()]);
        assert_eq!(app.session.undo_depth().0, before.0 + 1);
        assert_eq!(current_paint(&app, &target).unwrap().solid_color().unwrap().to_hex(), "00FF00");
        assert_eq!(app.session.doc().unwrap().node(b).unwrap().fills[0].solid_color().unwrap().to_hex(), "0000FF");
        app.session.undo().unwrap();
        assert_eq!(app.session.doc().unwrap().node(a).unwrap().fills[0].solid_color().unwrap().to_hex(), "FF0000");
    }

    #[test]
    fn writing_a_variable_keeps_its_alpha() {
        // The picker holds a colour variable's alpha as the paint opacity; writing it back applies it once.
        let (mut app, _, _) = setup();
        app.session.execute("variable.createCollection", &json!({"name":"T"})).unwrap();
        let v = app.session.execute("variable.create", &json!({"collection":"T","name":"tint","type":"color","value":"#0D6EFD80"})).unwrap();
        let var = app.session.doc().unwrap().variables.get(v["id"].as_str().unwrap()).unwrap().clone();
        let target = PickerTarget::Variable { id: var.id.clone(), mode: var.values.keys().next().unwrap().clone() };
        for _ in 0..2 {
            let paint = current_paint(&app, &target).unwrap();
            write(&mut app, &target, &paint, false);
        }
        let c = current_paint(&app, &target).unwrap().solid_color().unwrap();
        assert_eq!(c.to_css_hex().to_uppercase(), "#0D6EFD80");
    }

    #[test]
    fn picker_does_not_follow_selection_or_document_switches() {
        let (mut app, _, _) = setup();
        let target = app.ui.picker.clone().unwrap();
        app.session.execute("select.none", &json!({})).unwrap();
        assert!(current_paint(&app, &target).is_none());
        let (mut app, _, _) = setup();
        let target = app.ui.picker.clone().unwrap();
        app.session.execute("file.new", &json!({})).unwrap();
        assert!(current_paint(&app, &target).is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers_drag_tests::Ui;
    use figcraft_engine::Session;

    #[test]
    fn dragging_records_only_the_final_colour_in_recent_colors() {
        let mut u = Ui::new(Session::new());
        let target = PickerTarget::PageBackground;
        u.app.ui.picker = Some(target.clone());
        for hex in ["#110000", "#220000", "#330000"] {
            let c = Color::from_hex(hex).unwrap();
            write(&mut u.app, &target, &Paint::solid(c), true);
        }
        assert!(u.app.session.recent_colors.is_empty(), "previews must not be recorded: {:?}", u.app.session.recent_colors);
        finish(&mut u.app);
        let last = Color::from_hex("#330000").unwrap();
        assert_eq!(u.app.session.recent_colors, vec![last]);
    }

    #[test]
    fn a_drag_released_with_a_final_write_records_one_colour() {
        let mut u = Ui::new(Session::new());
        let target = PickerTarget::PageBackground;
        u.app.ui.picker = Some(target.clone());
        // Not a whole 8-bit value: the page stores it rounded, so recording both would add two near-identical swatches.
        let c = Color::new(0.3, 0.0, 0.0, 1.0);
        write(&mut u.app, &target, &Paint::solid(Color::new(0.1, 0.0, 0.0, 1.0)), true);
        write(&mut u.app, &target, &Paint::solid(c), false);
        finish(&mut u.app);
        assert_eq!(u.app.session.recent_colors.len(), 1, "{:?}", u.app.session.recent_colors);
    }

    #[test]
    fn a_click_outside_a_drag_still_records_the_colour() {
        let mut u = Ui::new(Session::new());
        let target = PickerTarget::PageBackground;
        u.app.ui.picker = Some(target.clone());
        let c = Color::from_hex("#445566").unwrap();
        write(&mut u.app, &target, &Paint::solid(c), false);
        assert_eq!(u.app.session.recent_colors, vec![c]);
    }

    fn image_layer(session: &mut Session, filters: ImageFilters) -> NodeId {
        image_layer_at(session, filters, 0.0)
    }

    fn image_layer_at(session: &mut Session, filters: ImageFilters, x: f64) -> NodeId {
        let id: NodeId =
            serde_json::from_value(session.execute("shape.rectangle", &json!({"x": x, "y": 0, "width": 20, "height": 20})).unwrap()["id"].clone())
                .unwrap();
        let mut paint = Paint::image("test-image", ImageScaleMode::Fill);
        if let PaintKind::Image(image) = &mut paint.kind {
            image.filters = filters;
        }
        session.execute("fill.set", &json!({"ids": [id], "index": 0, "paint": paint})).unwrap();
        id
    }

    #[test]
    fn image_picker_follows_single_selected_image_and_reads_its_filters() {
        let mut session = Session::new();
        let first = image_layer(&mut session, ImageFilters::default());
        let next_filters =
            ImageFilters { exposure: 0.65, contrast: -0.2, saturation: 0.35, temperature: -0.45, tint: 0.15, highlights: -0.55, shadows: 0.25 };
        let next = image_layer(&mut session, next_filters);
        session.set_selection(vec![first]);
        let mut app = FigApp::new(session, Box::new(crate::NoServices));
        app.ui.picker = Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![first] });

        app.session.set_selection(vec![next]);
        assert!(sync_picker_target(&mut app));
        assert_eq!(app.ui.picker, Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![next] }));
        let PaintKind::Image(image) = current_paint(&app, app.ui.picker.as_ref().unwrap()).unwrap().kind else { panic!("image paint") };
        assert_eq!(image.filters, next_filters);

        let target = app.ui.picker.as_ref().unwrap().clone();
        let mut changed = current_paint(&app, &target).unwrap();
        let PaintKind::Image(image) = &mut changed.kind else { panic!("image paint") };
        image.filters.saturation = -0.8;
        write(&mut app, &target, &changed, false);
        let first_paint = current_paint(&app, &PickerTarget::Paint { fills: true, index: 0, ids: vec![first] }).unwrap();
        let next_paint = current_paint(&app, &target).unwrap();
        let (PaintKind::Image(first_image), PaintKind::Image(next_image)) = (first_paint.kind, next_paint.kind) else { panic!("image paints") };
        assert_eq!(first_image.filters, ImageFilters::default(), "the old image was not edited");
        assert_eq!(next_image.filters.saturation, -0.8, "the new image received the edit");
    }

    #[test]
    fn image_picker_for_two_selected_layers_stays_open_and_edits_both() {
        let mut session = Session::new();
        let first = image_layer(&mut session, ImageFilters::default());
        let second = image_layer_at(&mut session, ImageFilters::default(), 30.0);
        session.set_selection(vec![first, second]);
        let mut app = FigApp::new(session, Box::new(crate::NoServices));
        let target = PickerTarget::Paint { fills: true, index: 0, ids: vec![first, second] };
        app.ui.picker = Some(target.clone());

        assert!(sync_picker_target(&mut app));
        assert_eq!(app.ui.picker, Some(target.clone()));
        let mut changed = current_paint(&app, &target).unwrap();
        let PaintKind::Image(image) = &mut changed.kind else { panic!("image paint") };
        image.filters.saturation = -0.8;
        write(&mut app, &target, &changed, false);
        for id in [first, second] {
            let paint = current_paint(&app, &PickerTarget::Paint { fills: true, index: 0, ids: vec![id] }).unwrap();
            let PaintKind::Image(image) = paint.kind else { panic!("image paint") };
            assert_eq!(image.filters.saturation, -0.8, "both layers were edited");
        }

        app.session.set_selection(vec![first]);
        assert!(sync_picker_target(&mut app));
        assert_eq!(app.ui.picker, Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![first] }), "a different selection retargets");
    }

    #[test]
    fn dragging_a_text_colour_while_editing_records_the_final_colour() {
        let mut session = Session::new();
        let text = NodeId(session.execute("text.create", &json!({"text": "Hello", "x": 0, "y": 0})).unwrap()["id"].as_u64().unwrap());
        session.execute("text.edit.begin", &json!({"id": text})).unwrap();
        let mut app = FigApp::new(session, Box::new(crate::NoServices));
        let target = PickerTarget::TextRange { id: text, start: 0, end: 5 };
        app.ui.picker = Some(target.clone());
        for hex in ["#110000", "#220000", "#330000"] {
            write(&mut app, &target, &Paint::solid(Color::from_hex(hex).unwrap()), true);
        }
        assert!(app.session.recent_colors.is_empty(), "previews are not recorded");
        assert_eq!(app.session.ds().unwrap().interaction.as_ref().map(|interaction| interaction.label.as_str()), Some("Edit text"));

        finish(&mut app);
        assert_eq!(app.session.recent_colors, vec![Color::from_hex("#330000").unwrap()]);
        assert!(app.session.ds().unwrap().text_edit.is_some(), "the text edit goes on");
    }

    #[test]
    fn document_switch_keeps_a_non_image_picker_open() {
        let mut app = FigApp::new(Session::new(), Box::new(crate::NoServices));
        app.ui.picker = Some(PickerTarget::PageBackground);
        assert!(sync_picker_target(&mut app));
        app.session.execute("file.new", &json!({"name": "Other"})).unwrap();

        assert!(sync_picker_target(&mut app));
        assert_eq!(app.ui.picker, Some(PickerTarget::PageBackground));
    }

    #[test]
    fn selection_order_does_not_close_a_multi_layer_picker() {
        let mut u = Ui::new(Session::new());
        let first = image_layer(&mut u.app.session, ImageFilters::default());
        let second = image_layer_at(&mut u.app.session, ImageFilters::default(), 30.0);
        u.app.session.set_selection(vec![first, second]);
        let target = PickerTarget::Paint { fills: true, index: 0, ids: vec![first, second] };
        u.app.ui.picker = Some(target.clone());
        u.frame(vec![]);
        u.app.session.set_selection(vec![second, first]);
        u.frame(vec![]);
        assert_eq!(u.app.ui.picker, Some(target), "the same layers in another order keep the popup open");
    }

    #[test]
    fn closing_a_fill_picker_for_another_selection_records_no_recent_colour() {
        let mut session = Session::new();
        let first = NodeId(session.execute("shape.rectangle", &json!({"width": 20, "height": 20})).unwrap()["id"].as_u64().unwrap());
        let other = NodeId(session.execute("shape.rectangle", &json!({"x": 40, "width": 20, "height": 20})).unwrap()["id"].as_u64().unwrap());
        session.execute("fill.set", &json!({"ids": [first], "color": "#FF0000"})).unwrap();
        session.set_selection(vec![first]);
        let mut u = Ui::new(session);
        u.app.ui.picker = Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![first] });
        u.frame(vec![]);
        // Pressing another layer starts a Move interaction and selects it.
        u.app.session.begin_interaction("Move").unwrap();
        u.app.session.set_selection(vec![other]);
        u.frame(vec![]);
        assert!(u.app.ui.picker.is_none(), "the picker closed");
        assert!(u.app.session.recent_colors.is_empty(), "no colour was picked: {:?}", u.app.session.recent_colors);
    }

    #[test]
    fn image_picker_closes_for_empty_or_non_image_selection() {
        for select_image in [false, true] {
            let mut session = Session::new();
            let image = image_layer(&mut session, ImageFilters::default());
            let other: NodeId = serde_json::from_value(
                session.execute("shape.rectangle", &json!({"x": 30, "y": 0, "width": 20, "height": 20})).unwrap()["id"].clone(),
            )
            .unwrap();
            session.set_selection(vec![image]);
            let mut app = FigApp::new(session, Box::new(crate::NoServices));
            app.ui.picker = Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![image] });
            app.session.set_selection(if select_image { vec![other] } else { Vec::new() });

            assert!(!sync_picker_target(&mut app));
            assert!(app.ui.picker.is_none());
        }
    }

    #[test]
    fn retargeting_does_not_finish_a_canvas_interaction() {
        let mut session = Session::new();
        let first = image_layer(&mut session, ImageFilters::default());
        let next = image_layer(&mut session, ImageFilters::default());
        session.set_selection(vec![first]);
        let mut app = FigApp::new(session, Box::new(crate::NoServices));
        app.ui.picker = Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![first] });
        app.session.begin_interaction("Move").unwrap();
        app.session.set_selection(vec![next]);

        assert!(sync_picker_target(&mut app));
        assert_eq!(app.session.ds().unwrap().interaction.as_ref().map(|interaction| interaction.label.as_str()), Some("Move"));
        assert_eq!(app.ui.picker, Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![next] }));
    }

    #[test]
    fn image_picker_uses_first_image_slot_when_old_slot_is_missing() {
        let mut session = Session::new();
        let first = image_layer(&mut session, ImageFilters::default());
        session.execute("fill.set", &json!({"ids": [first], "index": 0, "color": "#FF0000"})).unwrap();
        session.execute("fill.add", &json!({"ids": [first], "paint": Paint::image("first-image-second-slot", ImageScaleMode::Fill)})).unwrap();
        let next = image_layer(&mut session, ImageFilters { exposure: -0.35, ..ImageFilters::default() });
        let second = Paint::image("second-image", ImageScaleMode::Fill);
        session.execute("fill.set", &json!({"ids": [next], "index": 0, "paint": second})).unwrap();
        session.execute("stroke.add", &json!({"ids": [next], "paint": Paint::image("stroke-image", ImageScaleMode::Fill)})).unwrap();
        session.set_selection(vec![first]);
        let mut app = FigApp::new(session, Box::new(crate::NoServices));
        app.ui.picker = Some(PickerTarget::Paint { fills: true, index: 1, ids: vec![first] });
        assert!(sync_picker_target(&mut app));

        app.session.set_selection(vec![next]);
        assert!(sync_picker_target(&mut app));
        assert_eq!(app.ui.picker, Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![next] }));
        let PaintKind::Image(image) = current_paint(&app, app.ui.picker.as_ref().unwrap()).unwrap().kind else { panic!("image paint") };
        assert_eq!(image.image, "second-image");
    }

    #[test]
    fn image_picker_closes_when_document_changes_even_with_reused_node_ids() {
        let mut session = Session::new();
        let image = image_layer(&mut session, ImageFilters::default());
        session.set_selection(vec![image]);
        let mut app = FigApp::new(session, Box::new(crate::NoServices));
        app.ui.picker = Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![image] });
        assert!(sync_picker_target(&mut app));

        app.session.execute("file.new", &json!({"name": "Other"})).unwrap();
        let collision: NodeId = serde_json::from_value(
            app.session.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 20, "height": 20})).unwrap()["id"].clone(),
        )
        .unwrap();
        assert_eq!(collision, image);
        app.session.execute("fill.set", &json!({"ids": [collision], "color": "#FF0000"})).unwrap();

        assert!(!sync_picker_target(&mut app));
        assert!(app.ui.picker.is_none());
        assert!(app.ui.picker_context.is_none());
    }

    #[test]
    fn document_switch_finishes_color_edit_in_owner_without_touching_new_document() {
        let mut session = Session::new();
        let image = image_layer(&mut session, ImageFilters::default());
        session.set_selection(vec![image]);
        let mut app = FigApp::new(session, Box::new(crate::NoServices));
        let target = PickerTarget::Paint { fills: true, index: 0, ids: vec![image] };
        app.ui.picker = Some(target.clone());
        assert!(sync_picker_target(&mut app));

        let mut paint = current_paint(&app, &target).unwrap();
        if let PaintKind::Image(image) = &mut paint.kind {
            image.filters.exposure = 0.8;
        }
        write(&mut app, &target, &paint, true);
        let owner_uid = app.session.ds().unwrap().uid;
        app.session.execute("file.new", &json!({"name": "Other"})).unwrap();
        let new_uid = app.session.ds().unwrap().uid;
        app.session.begin_interaction("Move").unwrap();

        assert!(!sync_picker_target(&mut app));
        assert!(app.ui.picker.is_none());
        assert!(app.session.docs[0].interaction.is_none(), "old Color interaction was committed");
        assert_eq!(app.session.docs[0].history.undo.last().map(|entry| entry.label.as_str()), Some("Color"));
        assert_ne!(new_uid, owner_uid);
        assert_eq!(app.session.ds().unwrap().uid, new_uid);
        assert_eq!(app.session.ds().unwrap().interaction.as_ref().map(|interaction| interaction.label.as_str()), Some("Move"));

        app.session.active = 0;
        app.session.execute("edit.undo", &json!({})).unwrap();
        let PaintKind::Image(restored) = app.session.doc().unwrap().node(image).unwrap().fills[0].kind.clone() else { panic!("image paint") };
        assert_eq!(restored.filters.exposure, 0.0, "undo restored only the old image adjustment");
        app.session.active = 1;
        assert_eq!(app.session.ds().unwrap().interaction.as_ref().map(|interaction| interaction.label.as_str()), Some("Move"));
    }

    #[test]
    fn page_switch_preserves_non_image_picker() {
        for paint in [Paint::solid(Color::BLACK), Paint::image("test-image", ImageScaleMode::Fill)] {
            let mut session = Session::new();
            let created = session.execute("style.create", &json!({"type": "paint", "name": "Test paint", "value": [paint]})).unwrap();
            let target = PickerTarget::Style { id: created["id"].as_str().unwrap().into(), index: 0 };
            let mut app = FigApp::new(session, Box::new(crate::NoServices));
            app.ui.picker = Some(target.clone());
            assert_eq!(current_paint(&app, &target), Some(paint.clone()));
            assert!(sync_picker_target(&mut app));
            let page = serde_json::from_value::<NodeId>(app.session.execute("page.add", &json!({})).unwrap()["id"].clone()).unwrap();
            app.session.execute("page.select", &json!({"id": page})).unwrap();

            assert!(sync_picker_target(&mut app));
            assert_eq!(app.ui.picker, Some(target.clone()));
            assert_eq!(current_paint(&app, &target), Some(paint));
        }
    }

    #[test]
    fn selecting_and_dragging_an_image_layer_keeps_its_move_undoable() {
        use egui::Event;

        let mut session = Session::new();
        let first = image_layer(&mut session, ImageFilters { exposure: 0.4, ..ImageFilters::default() });
        let next = image_layer_at(&mut session, ImageFilters { exposure: -0.4, ..ImageFilters::default() }, 300.0);
        session.set_selection(vec![first]);
        let mut ui = Ui::new(session);
        ui.app.ui.picker = Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![first] });
        ui.frame(vec![]);

        let before = ui.app.session.doc().unwrap().world_bounds(next).unwrap().center();
        let start = ui.app.to_screen(before.x, before.y);
        let end = start + egui::vec2(60.0, 24.0);
        ui.button(start, true);
        ui.frame(vec![Event::PointerMoved(start + egui::vec2(30.0, 12.0))]);
        ui.frame(vec![Event::PointerMoved(end)]);
        ui.button(end, false);

        let after = ui.app.session.doc().unwrap().world_bounds(next).unwrap().center();
        assert!(after.x > before.x + 40.0, "the newly selected image moved");
        let first_paint = current_paint(&ui.app, &PickerTarget::Paint { fills: true, index: 0, ids: vec![first] }).unwrap();
        let next_paint = current_paint(&ui.app, &PickerTarget::Paint { fills: true, index: 0, ids: vec![next] }).unwrap();
        let (PaintKind::Image(first_image), PaintKind::Image(next_image)) = (first_paint.kind, next_paint.kind) else { panic!("image paints") };
        assert_eq!(first_image.filters.exposure, 0.4, "the old image stayed unchanged");
        assert_eq!(next_image.filters.exposure, -0.4, "the new image stayed unchanged");

        ui.app.session.execute("edit.undo", &json!({})).unwrap();
        let restored = ui.app.session.doc().unwrap().world_bounds(next).unwrap().center();
        assert!((restored.x - before.x).abs() < 0.01, "one undo restored the new image position");
    }

    #[test]
    fn same_frame_canvas_click_keeps_image_picker_open_on_new_selection() {
        use egui::{Event, PointerButton};

        let mut session = Session::new();
        let first = image_layer(&mut session, ImageFilters { exposure: 0.4, ..ImageFilters::default() });
        let next = image_layer_at(&mut session, ImageFilters { exposure: -0.4, ..ImageFilters::default() }, 300.0);
        session.set_selection(vec![first]);
        let mut ui = Ui::new(session);
        ui.app.ui.picker = Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![first] });
        ui.frame(vec![]);

        let bounds = ui.app.session.doc().unwrap().world_bounds(next).unwrap();
        let at = ui.app.to_screen(bounds.center().x, bounds.center().y);
        ui.frame(vec![
            Event::PointerMoved(at),
            Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Default::default() },
            Event::PointerMoved(at),
            Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ]);

        assert_eq!(ui.app.session.selection(), vec![next], "the click selected the image");
        assert_eq!(ui.app.ui.picker, Some(PickerTarget::Paint { fills: true, index: 0, ids: vec![next] }), "the popup stayed open and followed it");
        let PaintKind::Image(image) = current_paint(&ui.app, ui.app.ui.picker.as_ref().unwrap()).unwrap().kind else { panic!("image paint") };
        assert_eq!(image.filters.exposure, -0.4, "the new image's filter values loaded");
    }
}

#[cfg(test)]
#[path = "colorpicker_image_tests.rs"]
mod image_tests;

#[cfg(test)]
mod document_owner_tests {
    use super::*;
    use figcraft_engine::Session;

    fn native_copied_variables() -> (FigApp, PickerTarget, usize, usize) {
        let mut session = Session::new();
        let collection = session.execute("variable.createCollection", &json!({"name":"Owner tokens"})).unwrap();
        let variable = session
            .execute("variable.create", &json!({"collection":collection["id"],"name":"Owner brand","type":"Color","value":"#FF0000"}))
            .unwrap();
        let id = variable["id"].as_str().unwrap().to_string();
        let mode = session.doc().unwrap().variables.get_by_id(&id).unwrap().values.keys().next().unwrap().clone();
        let a = session.active;
        let native = figcraft_format::save_bytes(session.doc().unwrap(), None).unwrap();
        let (copy, issues) = figcraft_format::load_bytes(&native).unwrap();
        assert!(issues.is_empty());
        session.execute("file.new", &json!({"name":"Other native owner"})).unwrap();
        let b = session.active;
        session.ds_mut().unwrap().doc = std::sync::Arc::new(copy);
        assert_ne!(session.docs[a].uid, session.docs[b].uid);
        assert_eq!(session.docs[a].doc.variables, session.docs[b].doc.variables);
        session.execute("doc.switch", &json!({"index":a})).unwrap();
        (FigApp::new(session, Box::new(crate::NoServices)), PickerTarget::Variable { id, mode }, a, b)
    }

    fn cannot_retarget(first_draw: bool) {
        let (mut app, target, a, b) = native_copied_variables();
        app.open_picker(target.clone(), None);
        if !first_draw {
            assert!(sync_picker_target(&mut app));
        }
        let before_a = app.session.docs[a].doc.clone();
        let before_b = app.session.docs[b].doc.clone();
        let undo_b = app.session.docs[b].history.undo.len();
        app.session.execute("doc.switch", &json!({"index":b})).unwrap();
        // This is the actual callback write path, deliberately attempted before the next paint.
        write(&mut app, &target, &Paint::solid(Color::from_hex("#00FF00").unwrap()), false);
        let kept_open = sync_picker_target(&mut app);
        assert_eq!(app.session.docs[b].doc, before_b, "the old variable colour callback cannot mutate native-copy B");
        assert_eq!(app.session.docs[a].doc, before_a, "discarding an uncommitted popup leaves A intact");
        assert_eq!(app.session.docs[b].history.undo.len(), undo_b);
        assert!(!kept_open && app.ui.picker.is_none(), "an old document-bound popup is dismissed");
    }

    #[test]
    fn variable_colour_picker_cannot_retarget_a_native_copy_before_its_first_draw() {
        cannot_retarget(true);
    }

    #[test]
    fn variable_colour_picker_cannot_retarget_a_native_copy_after_being_drawn() {
        cannot_retarget(false);
    }

    fn cannot_start_foreign_interaction(first_draw: bool) {
        let (mut app, target, a, b) = native_copied_variables();
        app.open_picker(target.clone(), None);
        if !first_draw {
            assert!(sync_picker_target(&mut app));
        }
        app.session.execute("doc.switch", &json!({"index":b})).unwrap();
        let before_a = app.session.docs[a].doc.clone();
        let before_b = app.session.docs[b].doc.clone();
        let undo_b = app.session.docs[b].history.undo.len();
        assert!(app.session.docs[b].interaction.is_none());
        // A live callback must be rejected before opening an interaction, even when the
        // next frame has not yet synchronized the old popup's captured owner.
        write(&mut app, &target, &Paint::solid(Color::from_hex("#00FF00").unwrap()), true);
        assert!(app.session.docs[b].interaction.is_none(), "A's stale live callback cannot open B's Color interaction");
        assert_eq!(app.session.docs[b].doc, before_b);
        assert_eq!(app.session.docs[a].doc, before_a);
        assert_eq!(app.session.docs[b].history.undo.len(), undo_b);
        assert!(!sync_picker_target(&mut app));
        assert!(app.ui.picker.is_none());
        assert!(app.session.docs[b].interaction.is_none(), "discarding A cannot leave or commit a live edit in B");
    }

    #[test]
    fn a_stale_live_picker_callback_cannot_open_the_new_documents_interaction_before_first_draw() {
        cannot_start_foreign_interaction(true);
    }

    #[test]
    fn a_stale_live_picker_callback_cannot_open_the_new_documents_interaction_after_draw() {
        cannot_start_foreign_interaction(false);
    }
}

#[cfg(test)]
mod document_owner_epoch_tests {
    use super::*;
    use crate::layers_drag_tests::Ui as TestUi;
    use egui::{Event, Key, PointerButton, Pos2};
    use figcraft_engine::Session;

    fn variables() -> (Session, PickerTarget, PickerTarget) {
        let mut session = Session::new();
        let coll = session.execute("variable.createCollection", &json!({"name":"Owner tokens"})).unwrap();
        let mut targets = Vec::new();
        for (name, color) in [("Owner red", "#FF0000"), ("Owner blue", "#0000FF")] {
            let made = session.execute("variable.create", &json!({"collection":coll["id"],"name":name,"type":"Color","value":color})).unwrap();
            let id = made["id"].as_str().unwrap().to_string();
            let mode = session.doc().unwrap().variables.get_by_id(&id).unwrap().values.keys().next().unwrap().clone();
            targets.push(PickerTarget::Variable { id, mode });
        }
        (session, targets.remove(0), targets.remove(0))
    }

    fn text_position(shapes: &[egui::epaint::ClippedShape], wanted: &str) -> Option<Pos2> {
        fn point(shape: &egui::Shape, wanted: &str) -> Option<Pos2> {
            match shape {
                egui::Shape::Text(text) if text.galley.text() == wanted => Some(text.pos + text.galley.rect.center().to_vec2()),
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| point(s, wanted)),
                _ => None,
            }
        }
        shapes.iter().find_map(|shape| point(&shape.shape, wanted))
    }

    #[test]
    fn closing_an_old_variable_picker_never_commits_the_new_documents_color_interaction() {
        let (session, red, _) = variables();
        let mut app = FigApp::new(session, Box::new(crate::NoServices));
        let a = app.session.active;
        app.open_picker(red.clone(), None);
        assert!(sync_picker_target(&mut app));
        let native = figcraft_format::save_bytes(app.session.doc().unwrap(), None).unwrap();
        let (copy, issues) = figcraft_format::load_bytes(&native).unwrap();
        assert!(issues.is_empty());
        app.session.execute("file.new", &json!({"name":"Other owner"})).unwrap();
        let b = app.session.active;
        app.session.ds_mut().unwrap().doc = std::sync::Arc::new(copy);
        app.session.begin_interaction("Color").unwrap();
        let PickerTarget::Variable { id, mode } = &red else { panic!("expected the variable picker fixture") };
        app.session.execute("variable.set", &json!({"variable":id,"mode":mode,"value":"#0000FF"})).unwrap();
        let before_a = app.session.docs[a].doc.clone();
        let before_b = app.session.docs[b].doc.clone();
        let undo_b = app.session.docs[b].history.undo.len();
        close(&mut app);
        assert_eq!(
            app.session.docs[b].interaction.as_ref().map(|i| i.label.as_str()),
            Some("Color"),
            "closing A cannot finish B's unrelated live edit"
        );
        assert_eq!(app.session.docs[b].history.undo.len(), undo_b);
        assert_eq!(app.session.docs[b].doc, before_b);
        assert_eq!(app.session.docs[a].doc, before_a);
    }

    #[test]
    fn a_typed_hex_draft_cannot_commit_after_opening_the_next_variables_picker() {
        let (session, red, blue) = variables();
        let mut u = TestUi::new(session);
        u.app.open_picker(red, None);
        for _ in 0..12 {
            u.frame(vec![]);
        }
        let output = u.frame_output(vec![]);
        let at = text_position(&output.shapes, "FF0000").expect("the actual red hex field is drawn");
        u.frame(vec![Event::PointerMoved(at)]);
        for pressed in [true, false] {
            u.frame(vec![Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Default::default() }]);
        }
        u.frame(vec![]);
        let output = u.frame_output(vec![Event::Text("00FF00".into())]);
        assert!(text_position(&output.shapes, "00FF00").is_some(), "an actual uncommitted green draft is visible");
        let before = u.app.session.doc().unwrap().clone();
        u.app.open_picker(blue, None);
        for _ in 0..3 {
            u.frame(vec![]);
        }
        for pressed in [true, false] {
            u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed, repeat: false, modifiers: Default::default() }]);
        }
        let output = u.frame_output(vec![]);
        assert_eq!(u.app.session.doc().unwrap(), &before, "the old field draft must not recolour the next variable");
        assert!(text_position(&output.shapes, "0000FF").is_some(), "the second popup shows its own fresh blue value");
    }
}

// Review-owned append-only fixture for widgets/colorpicker.rs. Root owns Cargo execution.
#[cfg(test)]
mod document_owner_gradient_tests {
    use super::*;
    use figcraft_engine::Session;

    fn switched_gradient(label: &str, should_finish: bool) {
        let mut session = Session::new();
        let shape = session.execute("shape.rectangle", &json!({"width":100,"height":100})).unwrap();
        let mut paint = Paint::solid(Color::BLACK);
        let mut gradient = Gradient::default_for(GradientKind::Linear, Color::BLACK);
        gradient.stops.push(ColorStop { position: 0.5, color: Color::WHITE });
        paint.kind = PaintKind::Gradient(gradient);
        session.execute("fill.set", &json!({"ids":[shape["id"]],"index":0,"paint":paint})).unwrap();
        let a = session.active;
        session.execute("file.duplicate", &json!({})).unwrap();
        let b = session.active;
        session.execute("doc.switch", &json!({"index":a})).unwrap();
        let mut app = FigApp::new(session, Box::new(crate::NoServices));
        app.open_picker(PickerTarget::Paint { fills: true, index: 0, ids: vec![NodeId(shape["id"].as_u64().unwrap())] }, None);
        let before_a = app.session.docs[a].doc.clone();
        let undo_a = app.session.docs[a].history.undo.len();
        app.session.begin_interaction(label).unwrap();
        let PaintKind::Gradient(g) = &mut paint.kind else { panic!("expected the gradient fixture") };
        match label {
            "Add gradient stop" => g.stops.push(ColorStop { position: 0.75, color: Color::WHITE }),
            "Remove gradient stop" => {
                g.stops.pop();
            }
            _ => g.stops[0].position = 0.1,
        }
        app.session.execute("fill.set", &json!({"ids":[shape["id"]],"index":0,"paint":paint})).unwrap();
        assert_eq!(app.session.docs[a].interaction.as_ref().map(|i| i.label.as_str()), Some(label));
        assert_ne!(app.session.docs[a].doc, before_a, "the owned gesture has a real authored preview to finish");
        app.session.execute("doc.switch", &json!({"index":b})).unwrap();
        app.session.begin_interaction("Color").unwrap();
        let before_b = app.session.docs[b].clone();
        close(&mut app);
        if should_finish {
            assert!(app.session.docs[a].interaction.is_none(), "closing the stale popup finishes the old owner's eligible gradient gesture");
            assert_eq!(app.session.docs[a].history.undo.len(), undo_a + 1, "the old gesture becomes one undo step");
        } else {
            assert_eq!(
                app.session.docs[a].interaction.as_ref().map(|i| i.label.as_str()),
                Some(label),
                "an unrelated old-owner interaction stays open"
            );
            assert_eq!(app.session.docs[a].history.undo.len(), undo_a);
        }
        assert!(std::sync::Arc::ptr_eq(&before_b.doc, &app.session.docs[b].doc));
        assert_eq!(app.session.docs[b].history.undo.len(), before_b.history.undo.len());
        assert_eq!(
            app.session.docs[b].interaction.as_ref().map(|i| i.label.as_str()),
            Some("Color"),
            "B's identically named interaction remains B's"
        );
        if should_finish {
            app.session.active = a;
            assert!(app.session.undo().unwrap());
            assert_eq!(app.session.docs[a].doc, before_a, "the owned gradient edit undoes independently of B");
            assert_eq!(app.session.docs[b].interaction.as_ref().map(|i| i.label.as_str()), Some("Color"));
        }
    }

    #[test]
    fn old_owner_gradient_edit_closes_without_finishing_the_new_documents_color_gesture() {
        switched_gradient("Edit gradient", true);
    }
    #[test]
    fn old_owner_gradient_add_closes_as_one_owned_undo_step() {
        switched_gradient("Add gradient stop", true);
    }
    #[test]
    fn old_owner_gradient_remove_closes_as_one_owned_undo_step() {
        switched_gradient("Remove gradient stop", true);
    }
    #[test]
    fn old_owner_unrelated_move_is_not_committed_by_closing_the_picker() {
        switched_gradient("Move", false);
    }
}
