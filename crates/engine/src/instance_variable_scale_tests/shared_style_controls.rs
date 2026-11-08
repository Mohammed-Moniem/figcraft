//! Contextual shared-style workflows; every mutation under test uses the public command API.

use super::*;
use figcraft_doc::{Effect, EffectKind, LetterSpacing, LineHeight, NodeKind, TextStyle};

fn text_style(s: &Session, id: NodeId) -> TextStyle {
    node(s, id).kind.text().unwrap().style.clone()
}

fn make_text(s: &mut Session, parent: NodeId, text: &str) -> NodeId {
    idv(&ex(s, "text.create", json!({"parent":parent,"text":text,"props":{"fontSize":32,"lineHeight":40,"letterSpacing":2}})))
}

fn make_style(s: &mut Session, kind: &str, from: NodeId, name: &str) -> String {
    ex(s, "style.create", json!({"type":kind,"from":from,"name":name}))["id"].as_str().unwrap().to_owned()
}

#[test]
fn instance_variable_scale_style_apply_after_k_last_binding_removal_and_literal_updates_keep_context() {
    let (mut s, main, _inner, instance) = scene(false);
    let source = make_text(&mut s, main, "Original style source");
    let sid = make_style(&mut s, "text", source, "Original apply-after body");
    ex(&mut s, "style.bind", json!({"style":sid,"field":"fontSize","variable":"Space"}));
    let target = make_text(&mut s, main, "Original style target");
    let copy = copy_of(&s, instance, target);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    ex(&mut s, "style.apply", json!({"ids":[copy],"style":sid}));
    assert_eq!(s.undo_depth().0, history + 1);
    assert_eq!(text_style(&s, copy).font_size, 16.0);
    assert_eq!(text_style(&s, copy).line_height, LineHeight::Pixels(20.0));
    assert_eq!(text_style(&s, copy).letter_spacing, LetterSpacing::Pixels(1.0));
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.redo().unwrap();
    ex(&mut s, "style.bind", json!({"style":sid,"field":"fontSize"}));
    assert!(s.doc().unwrap().style(&sid).unwrap().bound.is_empty());
    assert_eq!(node(&s, copy).styles.text, Some(sid.clone()));
    assert_eq!(text_style(&s, copy).font_size, 16.0, "last binding removal does not switch scale conventions");
    ex(&mut s, "style.update", json!({"style":sid,"value":{"fontSize":64,"lineHeight":80,"letterSpacing":4}}));
    assert_eq!(text_style(&s, copy).font_size, 32.0);
    assert_eq!(text_style(&s, copy).line_height, LineHeight::Pixels(40.0));
    assert_eq!(text_style(&s, source).font_size, 64.0);
    assert_eq!(node(&s, copy).styles.text, Some(sid));
}

#[test]
fn instance_variable_scale_style_text_units_decorations_and_bound_clamp_order() {
    let (mut s, main, _inner, instance) = scene(false);
    let source = make_text(&mut s, main, "Original unit body");
    {
        let n = s.doc_mut().unwrap().get_mut(source).unwrap();
        let NodeKind::Text(text) = &mut n.kind else { panic!("text fixture") };
        text.style.line_height = LineHeight::Percent(150.0);
        text.style.letter_spacing = LetterSpacing::Percent(12.0);
        text.style.set_look(|look| {
            look.thickness = LineHeight::Pixels(4.0);
            look.offset = LineHeight::Pixels(6.0);
        });
    }
    let sid = make_style(&mut s, "text", source, "Original unit style");
    ex(&mut s, "style.bind", json!({"style":sid,"field":"fontSize","variable":"Space"}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":10000}));
    let raw = s.doc().unwrap().style(&sid).unwrap().clone();
    let vars = s.doc().unwrap().variables.clone();
    let copy = copy_of(&s, instance, source);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let shown = text_style(&s, copy);
    assert_eq!(shown.font_size, 5000.0, "raw10000×0.5 clamps after multiplication; not2500");
    assert_eq!(shown.line_height, LineHeight::Percent(150.0));
    assert_eq!(shown.letter_spacing, LetterSpacing::Percent(12.0));
    assert_eq!(shown.look().thickness, LineHeight::Pixels(2.0));
    assert_eq!(shown.look().offset, LineHeight::Pixels(3.0));
    assert_eq!(s.doc().unwrap().style(&sid), Some(&raw));
    assert_eq!(s.doc().unwrap().variables, vars);
    assert_eq!(node(&s, copy).styles.text, Some(sid));
}

fn original_effects() -> Vec<Effect> {
    use figcraft_doc::{Blur, BlurType, Glass, Noise, Shadow, Texture};
    vec![
        Effect { kind: EffectKind::DropShadow(Shadow { radius: 12.0, spread: 4.0, offset: (8.0, -6.0), ..Default::default() }), visible: true },
        Effect {
            kind: EffectKind::LayerBlur(Blur {
                radius: 20.0,
                start_radius: 4.0,
                blur_type: BlurType::Progressive,
                start: (0.2, 0.3),
                end: (0.8, 0.9),
            }),
            visible: true,
        },
        Effect { kind: EffectKind::Noise(Noise { size: 6.0, size_y: Some(8.0), density: 0.4, ..Default::default() }), visible: true },
        Effect { kind: EffectKind::Texture(Texture { size: 12.0, size_y: Some(16.0), radius: 10.0, ..Default::default() }), visible: true },
        Effect {
            kind: EffectKind::Glass(Glass { depth: 20.0, frost: 10.0, light_angle: 40.0, light_intensity: 0.5, ..Default::default() }),
            visible: true,
        },
    ]
}

#[test]
fn instance_variable_scale_style_mixed_effects_scale_lengths_and_preserve_normalized_values() {
    let (mut s, _main, inner, instance) = scene(false);
    s.doc_mut().unwrap().get_mut(inner).unwrap().effects = original_effects();
    let sid = make_style(&mut s, "effect", inner, "Original mixed effects");
    ex(&mut s, "style.bind", json!({"style":sid,"field":"effects/0/radius","variable":"Space"}));
    ex(&mut s, "style.bind", json!({"style":sid,"field":"effects/4/lightAngle","variable":"Space"}));
    ex(&mut s, "style.bind", json!({"style":sid,"field":"effects/4/lightIntensity","variable":"Space"}));
    let raw = s.doc().unwrap().style(&sid).unwrap().clone();
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let effects = node(&s, copy).effects;
    let EffectKind::DropShadow(shadow) = &effects[0].kind else { panic!("shadow") };
    assert_eq!((shadow.radius, shadow.spread, shadow.offset), (16.0, 2.0, (4.0, -3.0)));
    let EffectKind::LayerBlur(blur) = &effects[1].kind else { panic!("blur") };
    assert_eq!((blur.radius, blur.start_radius, blur.start, blur.end), (10.0, 2.0, (0.2, 0.3), (0.8, 0.9)));
    let EffectKind::Noise(noise) = &effects[2].kind else { panic!("noise") };
    assert_eq!((noise.size, noise.size_y, noise.density), (3.0, Some(4.0), 0.4));
    let EffectKind::Texture(texture) = &effects[3].kind else { panic!("texture") };
    assert_eq!((texture.size, texture.size_y, texture.radius), (6.0, Some(8.0), 5.0));
    let EffectKind::Glass(glass) = &effects[4].kind else { panic!("glass") };
    assert_eq!((glass.depth, glass.frost, glass.light_angle, glass.light_intensity), (10.0, 5.0, 32.0, 0.32));
    assert_eq!(node(&s, copy).styles.effect, Some(sid.clone()));
    assert_eq!(s.doc().unwrap().style(&sid), Some(&raw));
    let reopened = reopen_native_command(&s);
    assert_eq!(node(&reopened, copy).effects, effects);
    assert_eq!(node(&reopened, copy).styles.effect, Some(sid));
}

#[test]
fn instance_variable_scale_style_grid_keeps_count_and_scales_unbound_gutter_and_offset() {
    let (mut s, _main, inner, instance) = scene(false);
    s.doc_mut().unwrap().get_mut(inner).unwrap().grids =
        vec![figcraft_doc::LayoutGrid { size: 24.0, count: 6, gutter: 8.0, offset: -4.0, ..Default::default() }];
    let sid = make_style(&mut s, "grid", inner, "Original contextual grid");
    ex(&mut s, "style.bind", json!({"style":sid,"field":"layoutGrids/0/size","variable":"Space"}));
    ex(&mut s, "style.bind", json!({"style":sid,"field":"layoutGrids/0/count","variable":"Space"}));
    let raw = s.doc().unwrap().style(&sid).unwrap().clone();
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let grid = &node(&s, copy).grids[0];
    assert_eq!((grid.size, grid.count, grid.gutter, grid.offset), (16.0, 32, 4.0, -2.0));
    assert_eq!(s.doc().unwrap().style(&sid), Some(&raw));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
    let grid = &node(&s, copy).grids[0];
    assert_eq!((grid.size, grid.count, grid.gutter, grid.offset), (24.0, 48, 4.0, -2.0));
    assert_eq!(node(&s, copy).styles.grid, Some(sid));
}

#[test]
fn instance_variable_scale_style_mixed_paint_keeps_color_binding_and_tiled_image_scale() {
    use figcraft_color::{Color, ImageScaleMode, Paint, PaintKind};
    let (mut s, _main, inner, instance) = scene(false);
    ex(&mut s, "variable.create", json!({"collection":"Original dimensions","name":"Original ink","type":"color","value":"#123456"}));
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 40, 60, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let (hash, _, _) = figcraft_format::add_image(s.doc_mut().unwrap(), png).unwrap();
    let mut tile = Paint::image(hash, ImageScaleMode::Tile);
    let PaintKind::Image(image) = &mut tile.kind else { panic!("tile") };
    image.scale = 3.0;
    s.doc_mut().unwrap().get_mut(inner).unwrap().fills = vec![Paint::solid(Color::BLACK), tile];
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"fills/0/color","variable":"Original ink"}));
    let sid = make_style(&mut s, "paint", inner, "Original mixed paint");
    let raw = s.doc().unwrap().style(&sid).unwrap().clone();
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let painted = node(&s, copy);
    let PaintKind::Image(image) = &painted.fills[1].kind else { panic!("tile") };
    assert_eq!(image.scale, 1.5);
    assert_eq!(painted.styles.fill, Some(sid.clone()));
    assert_eq!(s.doc().unwrap().style(&sid), Some(&raw));
    ex(&mut s, "variable.set", json!({"variable":"Original ink","value":"#FF00FF"}));
    let painted = node(&s, copy);
    assert_eq!(painted.fills[0].solid_color().unwrap(), Color::new(1.0, 0.0, 1.0, 1.0));
    let PaintKind::Image(image) = &painted.fills[1].kind else { panic!("tile") };
    assert_eq!(image.scale, 1.5);
    assert_eq!(painted.styles.fill, Some(sid));
}

#[test]
fn instance_variable_scale_style_mixed_text_runs_keep_their_independent_links_and_edits_detach() {
    let (mut s, main, _inner, _) = scene(false);
    let a = make_text(&mut s, main, "Original run A");
    let sa = make_style(&mut s, "text", a, "Original run style A");
    ex(&mut s, "style.bind", json!({"style":sa,"field":"fontSize","variable":"Space"}));
    let b = make_text(&mut s, main, "Original run B");
    ex(&mut s, "node.set", json!({"ids":[b],"props":{"fontSize":20}}));
    let sb = make_style(&mut s, "text", b, "Original run style B");
    let text = make_text(&mut s, main, "ABCDEF");
    let style_a = text_style(&s, a);
    let style_b = text_style(&s, b);
    let n = s.doc_mut().unwrap().get_mut(text).unwrap();
    let NodeKind::Text(data) = &mut n.kind else { panic!("text") };
    data.link_text_style(0, 3, &style_a, &sa);
    data.link_text_style(3, 6, &style_b, &sb);
    n.styles.text = None;
    let instance = idv(&ex(&mut s, "instance.create", json!({"component":main,"x":700,"y":0})));
    let copy = copy_of(&s, instance, text);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let n = node(&s, copy);
    let data = n.kind.text().unwrap();
    let runs = data.spans();
    let left = runs.iter().find(|(a, b, _)| *a == 0 && *b == 3).unwrap().2;
    let right = runs.iter().find(|(a, b, _)| *a == 3 && *b == 6).unwrap().2;
    assert_eq!((left.font_size, left.style_id.as_deref()), (16.0, Some(sa.as_str())));
    assert_eq!((right.font_size, right.style_id.as_deref()), (10.0, Some(sb.as_str())));
    ex(&mut s, "node.set", json!({"ids":[copy],"props":{"fontSize":27}}));
    let n = node(&s, copy);
    assert_eq!(n.styles.text, None);
    assert!(n.kind.text().unwrap().spans().iter().all(|(_, _, style)| style.font_size == 27.0 && style.style_id.is_none()));
}

#[test]
fn instance_variable_scale_style_context_overflow_refuses_before_any_geometry_or_history_mutation() {
    let (mut s, main, _inner, instance) = scene(false);
    let text = make_text(&mut s, main, "Original finite style");
    make_style(&mut s, "text", text, "Original finite definition");
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    assert!(s.execute("transform.scale", &json!({"ids":[instance],"factor":1e308})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
}

#[test]
fn instance_variable_scale_style_ordinary_unbound_native_values_are_not_reapplied_without_k_context() {
    for context in [None, Some(1.0)] {
        let (mut s, main, _inner, _) = scene(false);
        let text = make_text(&mut s, main, "Original legacy cached body");
        let sid = make_style(&mut s, "text", text, "Original legacy definition");
        assert!(s.doc().unwrap().style(&sid).unwrap().bound.is_empty());
        // Native files can retain cached styled values until normal edit reconciliation. This
        // feature must not turn an ordinary unscaled open into a style reapplication operation.
        let n = s.doc_mut().unwrap().get_mut(text).unwrap();
        let NodeKind::Text(data) = &mut n.kind else { panic!("text") };
        data.style.font_size = 48.0;
        data.style.style_id = Some(sid.clone());
        n.styles.text = Some(sid.clone());
        n.variable_scale = context;
        assert_eq!(crate::variable_scale::factor(s.doc().unwrap(), text), Some(1.0));
        let reopened = reopen_native_command(&s);
        assert_eq!(text_style(&reopened, text).font_size, 48.0, "ordinary native values survive open: {context:?}");
        assert_eq!(node(&reopened, text).styles.text, Some(sid));
    }
}

#[test]
fn instance_variable_scale_style_ordinary_import_callback_preserves_unbound_values_without_k_context() {
    for context in [None, Some(1.0)] {
        let (mut s, main, _inner, _) = scene(false);
        let text = make_text(&mut s, main, "Original imported cached body");
        let sid = make_style(&mut s, "text", text, "Original imported definition");
        let bound_text = make_text(&mut s, main, "Original bound imported body");
        let bound_style = make_style(&mut s, "text", bound_text, "Original bound imported definition");
        ex(&mut s, "style.bind", json!({"style":bound_style,"field":"fontSize","variable":"Space"}));
        let n = s.doc_mut().unwrap().get_mut(text).unwrap();
        let NodeKind::Text(data) = &mut n.kind else { panic!("text") };
        data.style.font_size = 48.0;
        data.style.style_id = Some(sid.clone());
        n.styles.text = Some(sid.clone());
        n.variable_scale = context;
        let NodeKind::Text(data) = &mut s.doc_mut().unwrap().get_mut(bound_text).unwrap().kind else { panic!("bound text") };
        data.style.font_size = 48.0;
        let mut imported = s.doc().unwrap().clone();
        // Foreign file import calls this callback before component-source reconciliation. At
        // effective scale1 its legacy contract reapplies only variable-bound shared styles.
        crate::style_vars::after_load(&mut imported);
        let shown = imported.get(text).unwrap();
        assert_eq!(shown.kind.text().unwrap().style.font_size, 48.0, "ordinary imported values survive: {context:?}");
        assert_eq!(shown.styles.text, Some(sid));
        assert_eq!(imported.get(bound_text).unwrap().kind.text().unwrap().style.font_size, 32.0, "bound import refresh still applies");
    }
}

#[test]
fn instance_variable_scale_style_selected_range_capture_uses_presented_values_only_for_selected_chars() {
    let (mut s, main, _inner, instance) = scene(false);
    let text = make_text(&mut s, main, "ABCDEF");
    let copy = copy_of(&s, instance, text);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    ex(&mut s, "text.edit.begin", json!({"id":copy,"caret":0}));
    ex(&mut s, "text.edit.caret", json!({"index":3,"extend":true}));
    let sid = make_style(&mut s, "text", copy, "Original captured range");
    ex(&mut s, "text.edit.end", json!({}));
    let n = node(&s, copy);
    let spans = n.kind.text().unwrap().spans();
    let left = spans.iter().find(|(a, b, _)| *a == 0 && *b == 3).unwrap().2;
    let right = spans.iter().find(|(a, b, _)| *a == 3 && *b == 6).unwrap().2;
    assert_eq!(
        (left.font_size, left.line_height, left.letter_spacing, left.style_id.as_deref()),
        (16.0, LineHeight::Pixels(20.0), LetterSpacing::Pixels(1.0), Some(sid.as_str()))
    );
    assert_eq!((right.font_size, right.line_height, right.style_id.as_deref()), (16.0, LineHeight::Pixels(20.0), None));
    assert_eq!(n.styles.text, None, "a range does not retarget the remaining chars");
    let figcraft_doc::StyleKind::Text { style: raw } = &s.doc().unwrap().style(&sid).unwrap().kind else { panic!("raw text") };
    assert_eq!((raw.font_size, raw.line_height), (32.0, LineHeight::Pixels(40.0)));
}

#[test]
fn instance_variable_scale_style_effect_capture_after_k_keeps_unbound_offsets_and_native_links() {
    let (mut s, _main, inner, instance) = scene(false);
    s.doc_mut().unwrap().get_mut(inner).unwrap().effects = original_effects();
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"effects/0/radius","variable":"Space"}));
    let copy = copy_of(&s, instance, inner);
    assert_eq!(node(&s, copy).effects.len(), 5);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let before = node(&s, copy).effects;
    let sid = make_style(&mut s, "effect", copy, "Original captured effects");
    assert_eq!(node(&s, copy).effects, before);
    assert_eq!(node(&s, copy).styles.effect, Some(sid.clone()));
    let figcraft_doc::StyleKind::Effect { effects } = &s.doc().unwrap().style(&sid).unwrap().kind else { panic!("raw effects") };
    let EffectKind::DropShadow(shadow) = &effects[0].kind else { panic!("raw shadow") };
    assert_eq!((shadow.radius, shadow.spread, shadow.offset), (32.0, 4.0, (8.0, -6.0)));
    let mut reopened = reopen_native_command(&s);
    ex(&mut reopened, "variable.set", json!({"variable":"Space","value":48}));
    let shown = node(&reopened, copy);
    let EffectKind::DropShadow(shadow) = &shown.effects[0].kind else { panic!("shown shadow") };
    assert_eq!((shadow.radius, shadow.spread, shadow.offset), (24.0, 2.0, (4.0, -3.0)));
    assert_eq!(shown.styles.effect, Some(sid));
}

#[test]
fn instance_variable_scale_style_grid_and_tile_capture_after_k_preserve_physical_values() {
    use figcraft_color::{Color, ImageScaleMode, Paint, PaintKind};
    let (mut s, _main, inner, instance) = scene(false);
    ex(&mut s, "variable.create", json!({"collection":"Original dimensions","name":"Original capture ink","type":"color","value":"#123456"}));
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 40, 60, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let (hash, _, _) = figcraft_format::add_image(s.doc_mut().unwrap(), png).unwrap();
    let mut tile = Paint::image(hash, ImageScaleMode::Tile);
    let PaintKind::Image(image) = &mut tile.kind else { panic!("tile") };
    image.scale = 3.0;
    let n = s.doc_mut().unwrap().get_mut(inner).unwrap();
    n.fills = vec![Paint::solid(Color::BLACK), tile];
    n.grids = vec![figcraft_doc::LayoutGrid { size: 24.0, count: 6, gutter: 8.0, offset: -4.0, ..Default::default() }];
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"fills/0/color","variable":"Original capture ink"}));
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"layoutGrids/0/size","variable":"Space"}));
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let shown = node(&s, copy);
    let grid = make_style(&mut s, "grid", copy, "Original captured grid");
    let paint = make_style(&mut s, "paint", copy, "Original captured paint");
    assert_eq!(node(&s, copy).grids, shown.grids);
    assert_eq!(node(&s, copy).fills, shown.fills);
    let figcraft_doc::StyleKind::Grid { grids } = &s.doc().unwrap().style(&grid).unwrap().kind else { panic!("raw grid") };
    assert_eq!((grids[0].size, grids[0].gutter, grids[0].offset, grids[0].count), (32.0, 8.0, -4.0, 6));
    let figcraft_doc::StyleKind::Paint { paints } = &s.doc().unwrap().style(&paint).unwrap().kind else { panic!("raw paint") };
    let PaintKind::Image(image) = &paints[1].kind else { panic!("raw tile") };
    assert_eq!(image.scale, 3.0);
    assert_eq!((node(&s, copy).styles.grid, node(&s, copy).styles.fill), (Some(grid), Some(paint)));
}

#[test]
fn instance_variable_scale_style_noninvertible_capture_refuses_without_creating_a_style_or_history() {
    let (mut s, _main, inner, instance) = scene(false);
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    // At this context a literal frost100 would require a global base above the native100 cap.
    s.doc_mut().unwrap().get_mut(copy).unwrap().effects =
        vec![Effect { kind: EffectKind::Glass(figcraft_doc::Glass { frost: 100.0, ..Default::default() }), visible: true }];
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let result = s.execute("style.create", &json!({"type":"effect","from":copy,"name":"Original unrepresentable capture"}));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("scale context"));
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
}

#[test]
fn instance_variable_scale_style_selected_range_apply_presents_context_and_keeps_other_chars_undo_and_revalue() {
    for selected_editor_range in [false, true] {
        let (mut s, main, _inner, instance) = scene(false);
        let source = make_text(&mut s, main, "Original range source");
        let sid = make_style(&mut s, "text", source, "Original contextual range definition");
        ex(&mut s, "style.bind", json!({"style":sid,"field":"fontSize","variable":"Space"}));
        let target = make_text(&mut s, main, "ABCDEF");
        let copy = copy_of(&s, instance, target);
        ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
        if selected_editor_range {
            ex(&mut s, "text.edit.begin", json!({"id":copy,"caret":1}));
            ex(&mut s, "text.edit.caret", json!({"index":4,"extend":true}));
        }
        let before = s.doc().unwrap().clone();
        let history = s.undo_depth().0;
        let p = if selected_editor_range { json!({"ids":[copy],"style":sid}) } else { json!({"ids":[copy],"style":sid,"start":1,"end":4}) };
        ex(&mut s, "style.apply", p);
        let n = node(&s, copy);
        let data = n.kind.text().unwrap();
        assert_eq!(
            (data.style_at(1).font_size, data.style_at(1).line_height, data.style_at(1).letter_spacing),
            (16.0, LineHeight::Pixels(20.0), LetterSpacing::Pixels(1.0)),
            "range uses the layer context: {selected_editor_range}"
        );
        assert_eq!(data.style_at(1).style_id.as_deref(), Some(sid.as_str()));
        for at in [0, 4, 5] {
            assert_eq!((data.style_at(at).font_size, data.style_at(at).style_id.as_deref()), (16.0, None));
        }
        assert_eq!(n.styles.text, None);
        // Text editing commits its one undo step only when the interaction ends.
        if selected_editor_range {
            ex(&mut s, "text.edit.end", json!({}));
        }
        assert_eq!(s.undo_depth().0, history + 1);
        s.undo().unwrap();
        assert_eq!(s.doc().unwrap(), &before);
        s.redo().unwrap();
        ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
        let n = node(&s, copy);
        let data = n.kind.text().unwrap();
        assert_eq!((data.style_at(1).font_size, data.style_at(1).style_id.as_deref()), (24.0, Some(sid.as_str())));
        for at in [0, 4, 5] {
            assert_eq!((data.style_at(at).font_size, data.style_at(at).style_id.as_deref()), (16.0, None));
        }
    }
}

#[test]
fn instance_variable_scale_style_update_from_k_text_keeps_capture_and_explicit_value_global_with_undo() {
    let (mut s, main, _inner, instance) = scene(false);
    let source = make_text(&mut s, main, "Original redefine source");
    let sid = make_style(&mut s, "text", source, "Original redefine definition");
    let copy = copy_of(&s, instance, source);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    ex(&mut s, "style.update", json!({"style":sid,"from":copy}));
    assert_eq!(
        (text_style(&s, copy).font_size, text_style(&s, copy).line_height, text_style(&s, copy).letter_spacing),
        (16.0, LineHeight::Pixels(20.0), LetterSpacing::Pixels(1.0)),
        "redefine from the K copy captures an unscaled definition"
    );
    assert_eq!(node(&s, copy).styles.text.as_deref(), Some(sid.as_str()));
    assert_eq!(s.doc().unwrap(), &before, "same physical capture is a document no-op");
    // Existing style.update records one action even when its resulting data is equal.
    assert_eq!(s.undo_depth(), (history.0 + 1, 0));
    let value_history = s.undo_depth().0;
    ex(&mut s, "style.update", json!({"style":sid,"value":{"fontSize":48,"lineHeight":60,"letterSpacing":4}}));
    assert_eq!((text_style(&s, source).font_size, text_style(&s, copy).font_size), (48.0, 24.0));
    assert_eq!((text_style(&s, copy).line_height, text_style(&s, copy).letter_spacing), (LineHeight::Pixels(30.0), LetterSpacing::Pixels(2.0)));
    assert_eq!(s.undo_depth().0, value_history + 1);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn instance_variable_scale_style_update_from_k_effect_and_grid_preserves_definitions_and_physical_values() {
    for kind in ["effect", "grid"] {
        let (mut s, _main, inner, instance) = scene(false);
        let n = s.doc_mut().unwrap().get_mut(inner).unwrap();
        n.effects = original_effects();
        n.grids = vec![figcraft_doc::LayoutGrid { size: 24.0, count: 6, gutter: 8.0, offset: -4.0, ..Default::default() }];
        let sid = make_style(&mut s, kind, inner, "Original redefine list");
        let copy = copy_of(&s, instance, inner);
        ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
        let before = s.doc().unwrap().clone();
        let history = s.undo_depth();
        ex(&mut s, "style.update", json!({"style":sid,"from":copy}));
        assert_eq!(s.doc().unwrap().style(&sid), before.style(&sid), "global {kind} definition remains unscaled");
        assert_eq!(node(&s, copy), before.node(copy).unwrap().clone(), "physical {kind} values and links remain unchanged");
        assert_eq!(s.doc().unwrap(), &before);
        assert_eq!(s.undo_depth(), (history.0 + 1, 0));
        s.undo().unwrap();
        assert_eq!(s.doc().unwrap(), &before);
    }
}

#[test]
fn instance_variable_scale_style_update_noninvertible_from_refuses_before_definition_or_history_changes() {
    let (mut s, _main, inner, instance) = scene(false);
    s.doc_mut().unwrap().get_mut(inner).unwrap().effects = original_effects();
    let sid = make_style(&mut s, "effect", inner, "Original guarded redefinition");
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    s.doc_mut().unwrap().get_mut(copy).unwrap().effects =
        vec![Effect { kind: EffectKind::Glass(figcraft_doc::Glass { frost: 100.0, ..Default::default() }), visible: true }];
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    let result = s.execute("style.update", &json!({"style":sid,"from":copy,"name":"Original refused new name"}));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("scale context"));
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
}

#[test]
fn instance_variable_scale_style_update_from_same_saturated_presentation_keeps_existing_global_definition() {
    let (mut s, _main, inner, instance) = scene(false);
    s.doc_mut().unwrap().get_mut(inner).unwrap().effects =
        vec![Effect { kind: EffectKind::Glass(figcraft_doc::Glass { frost: 80.0, ..Default::default() }), visible: true }];
    let sid = make_style(&mut s, "effect", inner, "Original saturated definition");
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":2}));
    let EffectKind::Glass(shown) = node(&s, copy).effects[0].kind.clone() else { panic!("glass") };
    assert_eq!(shown.frost, 100.0);
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    ex(&mut s, "style.update", json!({"style":sid,"from":copy}));
    assert_eq!(
        s.doc().unwrap().style(&sid),
        before.style(&sid),
        "equivalent contextual presentation must keep base80, not overwrite it with100 or inverse50"
    );
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), (history.0 + 1, 0));
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn instance_variable_scale_style_legacy_history_control_records_update_action_and_ends_text_group_once() {
    let mut s = Session::new();
    let page = s.page().unwrap();
    let source = make_text(&mut s, page, "Original ordinary history source");
    let sid = make_style(&mut s, "text", source, "Original ordinary history definition");
    assert!(s.doc().unwrap().nodes().all(|n| n.variable_scale.is_none()));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    ex(&mut s, "style.update", json!({"style":sid,"from":source}));
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth().0, history + 1, "data equality does not suppress the existing Arc-based command action");
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    let target = make_text(&mut s, page, "ABCDEF");
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    ex(&mut s, "text.edit.begin", json!({"id":target,"caret":1}));
    ex(&mut s, "text.edit.caret", json!({"index":4,"extend":true}));
    ex(&mut s, "style.apply", json!({"ids":[target],"style":sid}));
    assert_eq!(node(&s, target).kind.text().unwrap().style_at(1).style_id.as_deref(), Some(sid.as_str()));
    assert_eq!(s.undo_depth().0, history, "active text interaction has not committed yet");
    ex(&mut s, "text.edit.end", json!({}));
    assert_eq!(s.undo_depth().0, history + 1);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
}

// Original public workflows for a changed style reference with equal physical values.
// Append inside instance_variable_scale_tests/shared_style_controls.rs after the owner freezes.
fn review_style_reference_radius(s: &Session, id: NodeId) -> f64 {
    match node(s, id).effects.first().map(|effect| &effect.kind) {
        Some(EffectKind::DropShadow(shadow)) => shadow.radius,
        other => panic!("original single-shadow fixture: {other:?}"),
    }
}

fn review_style_reference_ownership(detach: bool) {
    let (mut s, _main, inner, instance) = scene(false);
    let effects = vec![Effect {
        kind: EffectKind::DropShadow(figcraft_doc::Shadow { radius: 10.0, spread: 0.0, offset: (0.0, 0.0), ..Default::default() }),
        visible: true,
    }];
    ex(&mut s, "node.set", json!({"ids":[inner],"props":{"effects":effects}}));
    let inherited = make_style(&mut s, "effect", inner, "Original inherited shadow");
    let other = ex(&mut s, "style.create", json!({"type":"effect","name":"Original equal independent shadow","value":effects}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let globals = s.doc().unwrap().styles.clone();
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":2}));
    assert_eq!(review_style_reference_radius(&s, inner), 10.0);
    assert_eq!(review_style_reference_radius(&s, copy), 20.0);
    assert_eq!(node(&s, copy).styles.effect.as_deref(), Some(inherited.as_str()));
    assert!(node(&s, copy).scale_derived_overrides.contains("effects"), "original fixture begins with a K-derived physical group");

    let before_action = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    let chosen = if detach {
        ex(&mut s, "style.detach", json!({"ids":[copy],"which":"effect"}));
        None
    } else {
        ex(&mut s, "style.apply", json!({"ids":[copy],"style":other}));
        Some(other)
    };
    assert_eq!(review_style_reference_radius(&s, copy), 20.0, "the explicit reference action preserves the physical shadow");
    assert_eq!(node(&s, copy).styles.effect, chosen);
    assert_eq!(s.undo_depth().0, history + 1);
    let after_action = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before_action);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &after_action);

    // Round is initially10, so introducing the main binding changes ownership before its value.
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"effects/0/radius","variable":"Round"}));
    assert_eq!(review_style_reference_radius(&s, inner), 10.0);
    ex(&mut s, "variable.set", json!({"variable":"Round","value":24}));
    assert_eq!(review_style_reference_radius(&s, inner), 24.0, "the main still follows its newly introduced variable");
    assert_eq!(review_style_reference_radius(&s, copy), 20.0, "an explicit local style choice does not acquire the main's new radius binding");
    let local = node(&s, copy);
    assert_eq!(local.styles.effect, chosen, "keep the explicitly chosen reference or detached state through main rebuild");
    assert!(!local.bound.contains_key("effects/0/radius"), "the local style action removed ownership from future inherited bindings");
    assert_eq!(s.doc().unwrap().styles, globals, "the public variable edit does not rewrite the two literal global definitions");
    assert!(!local.scale_derived_overrides.contains("effects"), "provenance check follows the observable behavior assertions");
}

#[test]
fn instance_variable_scale_style_reference_equal_effect_replacement_keeps_local_ownership() {
    review_style_reference_ownership(false);
}

#[test]
fn instance_variable_scale_style_reference_detach_keeps_local_ownership() {
    review_style_reference_ownership(true);
}

// Original2px fixture and public K/main-binding workflows. Asset setup uses the existing codec;
// every mutation whose ownership is under test uses public commands. Append in shared_style_controls.
fn review_tile_scale(s: &Session, id: NodeId) -> f64 {
    match node(s, id).fills.first().map(|paint| &paint.kind) {
        Some(figcraft_color::PaintKind::Image(image)) => image.scale,
        other => panic!("original first tiled-image paint: {other:?}"),
    }
}

fn review_tile_color(s: &Session, id: NodeId) -> figcraft_color::Color {
    node(s, id).fills.get(1).and_then(|paint| paint.solid_color()).expect("original second solid-color paint")
}

fn review_tiled_fill_main_binding(local_edit: bool) {
    use figcraft_color::{Color, ImageScaleMode, Paint, PaintKind};
    let (mut s, _main, inner, instance) = scene(false);
    let initial = Color::from_hex("123456").unwrap();
    let local_color = Color::new(0.0, 1.0, 0.0, 1.0);
    let changed = Color::new(1.0, 0.0, 0.0, 1.0);
    ex(&mut s, "variable.create", json!({"collection":"Original dimensions","name":"Original later main ink","type":"color","value":"#123456"}));
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 40, 60, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let (hash, _, _) = figcraft_format::add_image(s.doc_mut().unwrap(), png).unwrap();
    let mut tile = Paint::image(hash, ImageScaleMode::Tile);
    let PaintKind::Image(image) = &mut tile.kind else { panic!("tile") };
    image.scale = 2.0;
    ex(&mut s, "node.set", json!({"ids":[inner],"props":{"fills":[tile,Paint::solid(initial)]}}));
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":2}));
    assert_eq!((review_tile_scale(&s, inner), review_tile_scale(&s, copy)), (2.0, 4.0));
    assert_eq!((review_tile_color(&s, inner), review_tile_color(&s, copy)), (initial, initial));

    if local_edit {
        let before = s.doc().unwrap().clone();
        let history = s.undo_depth().0;
        let mut fills = node(&s, copy).fills;
        fills[1] = Paint::solid(local_color);
        ex(&mut s, "node.set", json!({"ids":[copy],"props":{"fills":fills}}));
        assert_eq!(review_tile_color(&s, copy), local_color);
        assert_eq!(review_tile_scale(&s, copy), 4.0);
        assert_eq!(s.undo_depth().0, history + 1);
        let after = s.doc().unwrap().clone();
        s.undo().unwrap();
        assert_eq!(s.doc().unwrap(), &before);
        s.redo().unwrap();
        assert_eq!(s.doc().unwrap(), &after);
    }

    // The new binding initially leaves the main's color unchanged; its later value must flow
    // to a K-derived paint group, but not through an explicit local paint-list edit.
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"fills/1/color","variable":"Original later main ink"}));
    assert_eq!(review_tile_color(&s, inner), initial);
    ex(&mut s, "variable.set", json!({"variable":"Original later main ink","value":"#FF0000"}));
    assert_eq!(review_tile_color(&s, inner), changed);
    assert_eq!(review_tile_scale(&s, inner), 2.0, "the main's tile never scales with the copy");
    assert_eq!(review_tile_scale(&s, copy), 4.0, "rebuild retains the K-scaled physical tile");
    assert_eq!(review_tile_color(&s, copy), if local_edit { local_color } else { changed });
    let own = node(&s, copy);
    let main = node(&s, inner);
    if local_edit {
        assert!(!own.bound.contains_key("fills/1/color"));
        assert!(!own.scale_derived_overrides.contains("fills"), "true local fill edit retires K provenance");
    } else {
        assert_eq!(own.bound.get("fills/1/color"), main.bound.get("fills/1/color"));
        assert!(own.scale_derived_overrides.contains("fills"), "provenance assertion follows observable color and scale behavior");
    }
}

#[test]
fn instance_variable_scale_tiled_fill_new_main_color_binding_keeps_physical_tile_and_follows_color() {
    review_tiled_fill_main_binding(false);
}

#[test]
fn instance_variable_scale_tiled_fill_true_local_edit_does_not_acquire_new_main_binding() {
    review_tiled_fill_main_binding(true);
}

// Original public workflows: a global style mutation is not an explicit local instance edit.
// Append inside shared_style_controls.rs only after the producer owner seals its active run.
// Reuses review_style_reference_radius from the prior original ownership controls.
fn review_global_style_change_preserves_derived_ownership(operation: &str) {
    let (mut s, _main, inner, instance) = scene(false);
    let shadow = |radius| {
        vec![Effect {
            kind: EffectKind::DropShadow(figcraft_doc::Shadow { radius, spread: 0.0, offset: (0.0, 0.0), ..Default::default() }),
            visible: true,
        }]
    };
    ex(&mut s, "node.set", json!({"ids":[inner],"props":{"effects":shadow(10.0)}}));
    let inherited = make_style(&mut s, "effect", inner, "Original globally edited shadow");
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":2}));
    assert_eq!(review_style_reference_radius(&s, inner), 10.0);
    assert_eq!(review_style_reference_radius(&s, copy), 20.0);
    assert!(node(&s, copy).scale_derived_overrides.contains("effects"), "the fixture starts with a derived shadow group");

    let before = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    let (expected_main, expected_copy) = match operation {
        "delete" => {
            ex(&mut s, "style.delete", json!({"style":inherited}));
            assert!(s.doc().unwrap().style(&inherited).is_none());
            assert_eq!(node(&s, inner).styles.effect, None);
            assert_eq!(node(&s, copy).styles.effect, None);
            (10.0, 20.0)
        }
        "update" => {
            ex(&mut s, "style.update", json!({"style":inherited,"value":shadow(12.0)}));
            (12.0, 24.0)
        }
        "bind" => {
            ex(&mut s, "style.bind", json!({"style":inherited,"field":"effects/0/radius","variable":"Space"}));
            assert!(s.doc().unwrap().style(&inherited).unwrap().bound.contains_key("effects/0/radius"));
            (32.0, 64.0)
        }
        other => panic!("original bounded operation: {other}"),
    };
    assert_eq!(review_style_reference_radius(&s, inner), expected_main, "the global action reaches the main");
    assert_eq!(review_style_reference_radius(&s, copy), expected_copy, "the global action reaches the copy in its K context");
    assert_eq!(s.undo_depth().0, history + 1);
    let after = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &after);
    let globals = s.doc().unwrap().styles.clone();

    // This is a new NODE binding on the main, after the global operation; it is not a style definition edit.
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"effects/0/radius","variable":"Round"}));
    assert_eq!(review_style_reference_radius(&s, inner), 10.0, "the explicit main-node binding takes its initial value");
    ex(&mut s, "variable.set", json!({"variable":"Round","value":24}));
    assert_eq!(review_style_reference_radius(&s, inner), 24.0);
    assert_eq!(
        review_style_reference_radius(&s, copy),
        48.0,
        "a global style operation must not turn the K-derived copy into a locally owned shadow override"
    );
    let main_binding = node(&s, inner).bound.get("effects/0/radius").cloned().unwrap();
    let derived = node(&s, copy);
    assert_eq!(derived.bound.get("effects/0/radius"), Some(&main_binding));
    assert!(derived.scale_derived_overrides.contains("effects"), "provenance assertion follows the public numeric behavior");
    assert_eq!(s.doc().unwrap().styles, globals, "the new node binding does not rewrite global definitions");
}

#[test]
fn instance_variable_scale_global_style_delete_keeps_derived_ownership() {
    review_global_style_change_preserves_derived_ownership("delete");
}

#[test]
fn instance_variable_scale_global_style_update_keeps_derived_ownership() {
    review_global_style_change_preserves_derived_ownership("update");
}

#[test]
fn instance_variable_scale_global_style_bind_keeps_derived_ownership() {
    review_global_style_change_preserves_derived_ownership("bind");
}

// Original compatibility controls for global-vs-local ownership through existing command wrappers.
// Append inside shared_style_controls.rs after the current producer run is terminal.
fn review_global_wrapper_shadow(radius: f64) -> Vec<Effect> {
    vec![Effect {
        kind: EffectKind::DropShadow(figcraft_doc::Shadow { radius, spread: 0.0, offset: (0.0, 0.0), ..Default::default() }),
        visible: true,
    }]
}

#[test]
fn instance_variable_scale_repeated_global_style_update_keeps_derived_ownership() {
    let (mut s, _main, inner, instance) = scene(false);
    ex(&mut s, "node.set", json!({"ids":[inner],"props":{"effects":review_global_wrapper_shadow(10.0)}}));
    let inherited = make_style(&mut s, "effect", inner, "Original repeated global shadow");
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":2}));
    ex(&mut s, "style.update", json!({"style":inherited,"value":review_global_wrapper_shadow(12.0)}));
    assert_eq!(review_style_reference_radius(&s, inner), 12.0);
    assert_eq!(review_style_reference_radius(&s, copy), 24.0);
    ex(&mut s, "edit.repeat", json!({}));
    assert_eq!(review_style_reference_radius(&s, copy), 24.0, "Repeat executes the same global definition edit");
    assert_eq!(node(&s, copy).styles.effect.as_deref(), Some(inherited.as_str()), "the global operation itself preserves the inherited link");
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"effects/0/radius","variable":"Round"}));
    ex(&mut s, "variable.set", json!({"variable":"Round","value":24}));
    assert_eq!(review_style_reference_radius(&s, inner), 24.0);
    assert_eq!(review_style_reference_radius(&s, copy), 48.0);
    assert_eq!(node(&s, copy).styles.effect, None, "a differing explicit node field unlinks the inherited style");
    assert!(node(&s, copy).bound.contains_key("effects/0/radius"));
    assert!(node(&s, copy).scale_derived_overrides.contains("effects"));
}

#[test]
fn instance_variable_scale_batch_global_update_and_local_style_choice_keep_separate_ownership() {
    let (mut s, main, inner, instance) = scene(false);
    ex(&mut s, "node.set", json!({"ids":[inner],"props":{"effects":review_global_wrapper_shadow(10.0)}}));
    let inherited = make_style(&mut s, "effect", inner, "Original batch inherited shadow");
    let other = ex(
        &mut s,
        "style.create",
        json!({"type":"effect","name":"Original batch independent shadow","value":review_global_wrapper_shadow(10.0)}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let second = idv(&ex(&mut s, "instance.create", json!({"component":main,"x":900,"y":0})));
    let (copy, local_copy) = (copy_of(&s, instance, inner), copy_of(&s, second, inner));
    ex(&mut s, "transform.scale", json!({"ids":[instance,second],"factor":2}));
    assert_eq!(review_style_reference_radius(&s, copy), 20.0);
    assert_eq!(review_style_reference_radius(&s, local_copy), 20.0);
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    ex(
        &mut s,
        "command.batch",
        json!({"commands":[
            {"command":"style.update","params":{"style":inherited,"value":review_global_wrapper_shadow(12.0)}},
            {"command":"style.apply","params":{"ids":[local_copy],"style":other}}
        ]}),
    );
    assert_eq!(review_style_reference_radius(&s, copy), 24.0);
    assert_eq!(review_style_reference_radius(&s, local_copy), 20.0);
    assert_eq!(s.undo_depth().0, history + 1, "the complete batch is one action");
    let after = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &after);
    assert_eq!(node(&s, copy).styles.effect.as_deref(), Some(inherited.as_str()), "the global operation itself preserves the inherited link");
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"effects/0/radius","variable":"Round"}));
    ex(&mut s, "variable.set", json!({"variable":"Round","value":24}));
    assert_eq!(review_style_reference_radius(&s, inner), 24.0);
    assert_eq!(review_style_reference_radius(&s, copy), 48.0, "global style propagation stays inherited");
    assert_eq!(review_style_reference_radius(&s, local_copy), 20.0, "the batch's explicit local style choice stays owned");
    assert_eq!(node(&s, copy).styles.effect, None, "a differing explicit node field unlinks the inherited style");
    assert_eq!(node(&s, local_copy).styles.effect.as_deref(), Some(other.as_str()));
    assert!(node(&s, copy).bound.contains_key("effects/0/radius"));
    assert!(!node(&s, local_copy).bound.contains_key("effects/0/radius"));
    assert!(node(&s, copy).scale_derived_overrides.contains("effects"));
    assert!(!node(&s, local_copy).scale_derived_overrides.contains("effects"));
}

// Combined K, PR73 V3 and signed phase controls; retained original assertions.
fn composed_text(s: &Session, id: NodeId) -> &figcraft_doc::TextData {
    s.doc().unwrap().node(id).unwrap().kind.text().unwrap()
}

fn composed_wrap_source(s: &mut Session, main: NodeId) -> NodeId {
    let text =
        idv(&ex(s, "text.create", json!({"parent":main,"text":"First\nSecond\n","props":{"fontSize":32,"lineHeight":"150%","letterSpacing":"2px"}})));
    ex(s, "node.set", json!({"ids":[text],"props":{"wrapStyle":"pretty"}}));
    ex(s, "node.set", json!({"ids":[text],"props":{"start":6,"end":12,"wrapStyle":"balance"}}));
    text
}

#[test]
fn composed_k_range_style_capture_and_apply_keep_paragraph_wrap_and_physical_units() {
    use figcraft_doc::{StyleKind, WrapStyle};
    let (mut s, main, _inner, instance) = scene(false);
    let text = composed_wrap_source(&mut s, main);
    let copy = copy_of(&s, instance, text);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    ex(&mut s, "text.edit.begin", json!({"id":copy,"caret":6}));
    ex(&mut s, "text.edit.caret", json!({"index":12,"extend":true}));
    let sid =
        ex(&mut s, "style.create", json!({"type":"text","from":copy,"name":"Original composed range style"}))["id"].as_str().unwrap().to_owned();
    ex(&mut s, "text.edit.end", json!({}));
    let StyleKind::Text { style } = &s.doc().unwrap().style(&sid).unwrap().kind else { panic!("text definition") };
    assert_eq!(
        (style.font_size, style.line_height, style.letter_spacing, style.wrap_style),
        (32.0, LineHeight::Percent(150.0), LetterSpacing::Pixels(2.0), WrapStyle::Balance)
    );
    let shown = composed_text(&s, copy);
    assert_eq!(
        (shown.style_at(6).font_size, shown.style_at(6).line_height, shown.style_at(6).letter_spacing),
        (16.0, LineHeight::Percent(150.0), LetterSpacing::Pixels(1.0))
    );
    assert_eq!(
        (shown.para_wrap_style(0), shown.para_wrap_style(1), shown.para_wrap_style(2)),
        (WrapStyle::Pretty, WrapStyle::Balance, WrapStyle::Pretty)
    );
    assert_eq!(shown.style_at(6).style_id.as_deref(), Some(sid.as_str()));
    assert_eq!(shown.style_at(0).style_id, None);
    assert_eq!(s.undo_depth().0, history + 1);
    let captured = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &captured);
    ex(&mut s, "style.apply", json!({"ids":[copy],"style":sid,"start":0,"end":5}));
    let shown = composed_text(&s, copy);
    assert_eq!(shown.style_at(0).font_size, 16.0, "range apply presents the raw32 definition through K0.5");
    assert_eq!(
        (shown.para_wrap_style(0), shown.para_wrap_style(1), shown.para_wrap_style(2)),
        (WrapStyle::Balance, WrapStyle::Balance, WrapStyle::Pretty)
    );
    let mut reopened = reopen_native_command(&s);
    assert_eq!(node(&reopened, instance).variable_scale, Some(0.5));
    assert_eq!(composed_text(&reopened, copy), composed_text(&s, copy));
    ex(&mut reopened, "style.update", json!({"style":sid,"value":{"fontSize":40,"lineHeight":"150%","letterSpacing":"4px","wrapStyle":"pretty"}}));
    let shown = composed_text(&reopened, copy);
    assert_eq!((shown.style_at(0).font_size, shown.style_at(0).letter_spacing), (20.0, LetterSpacing::Pixels(2.0)));
    assert_eq!(
        (shown.para_wrap_style(0), shown.para_wrap_style(1), shown.para_wrap_style(2)),
        (WrapStyle::Pretty, WrapStyle::Pretty, WrapStyle::Pretty)
    );
}

#[test]
fn composed_k_update_from_whole_text_captures_effective_wrap_without_double_scaling() {
    use figcraft_doc::{StyleKind, WrapStyle};
    let (mut s, main, _inner, instance) = scene(false);
    let text = make_text(&mut s, main, "Original whole-layer style");
    ex(&mut s, "node.set", json!({"ids":[text],"props":{"lineHeight":"150%","letterSpacing":"2px","wrapStyle":"balance"}}));
    let sid = make_style(&mut s, "text", text, "Original composed whole style");
    let copy = copy_of(&s, instance, text);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    ex(&mut s, "node.set", json!({"ids":[copy],"props":{"wrapStyle":"pretty"}}));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    ex(&mut s, "style.update", json!({"style":sid,"from":copy}));
    let StyleKind::Text { style } = &s.doc().unwrap().style(&sid).unwrap().kind else { panic!("text definition") };
    assert_eq!(
        (style.font_size, style.line_height, style.letter_spacing, style.wrap_style),
        (32.0, LineHeight::Percent(150.0), LetterSpacing::Pixels(2.0), WrapStyle::Pretty)
    );
    assert_eq!(composed_text(&s, copy).style.font_size, 16.0);
    assert_eq!(composed_text(&s, text).style.font_size, 32.0);
    assert_eq!(composed_text(&s, text).wrap_style, WrapStyle::Pretty);
    assert_eq!(s.undo_depth().0, history + 1);
    let updated = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &updated);
}

#[test]
fn composed_k_paragraph_wrap_edit_retires_only_its_local_text_group_before_revalue() {
    use figcraft_doc::WrapStyle;
    let (mut s, main, _inner, instance) = scene(false);
    let text = composed_wrap_source(&mut s, main);
    let copy = copy_of(&s, instance, text);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    ex(&mut s, "text.edit.begin", json!({"id":copy,"caret":6}));
    ex(&mut s, "text.edit.caret", json!({"index":12,"extend":true}));
    ex(&mut s, "text.wrapStyle", json!({"style":"auto"}));
    ex(&mut s, "text.edit.end", json!({}));
    assert_eq!(
        (composed_text(&s, copy).para_wrap_style(0), composed_text(&s, copy).para_wrap_style(1), composed_text(&s, copy).para_wrap_style(2)),
        (WrapStyle::Pretty, WrapStyle::Auto, WrapStyle::Pretty)
    );
    assert_eq!(s.undo_depth().0, history + 1);
    let edited = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &edited);
    ex(&mut s, "variable.bind", json!({"ids":[text],"field":"fontSize","variable":"Round"}));
    ex(&mut s, "variable.set", json!({"variable":"Round","value":24}));
    assert_eq!(composed_text(&s, text).style.font_size, 24.0);
    assert_eq!(composed_text(&s, copy).style.font_size, 16.0, "the explicit local paragraph action owns its existing text group");
    assert_eq!(composed_text(&s, copy).para_wrap_style(1), WrapStyle::Auto);
    assert!(!node(&s, copy).scale_derived_overrides.contains("textStyle"));
    assert!(!node(&s, copy).bound.contains_key("fontSize"));
}

#[test]
fn composed_k_manual_signed_dash_phase_and_bound_weight_survive_revalue_native_and_undo() {
    let (mut s, _main, inner, instance) = scene(false);
    ex(&mut s, "node.set", json!({"ids":[inner],"props":{"strokes":["#123456"],"strokeWeight":10,"dashes":[8,4],"dashPhase":-3}}));
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"strokeWeight","variable":"Round"}));
    let copy = copy_of(&s, instance, inner);
    let original = s.doc().unwrap().clone();
    let globals = s.doc().unwrap().variables.clone();
    let history = s.undo_depth().0;
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    assert_eq!((node(&s, inner).stroke.weight, node(&s, inner).stroke.dashes, node(&s, inner).stroke.dash_phase), (10.0, vec![8.0, 4.0], Some(-3.0)));
    assert_eq!((node(&s, copy).stroke.weight, node(&s, copy).stroke.dashes, node(&s, copy).stroke.dash_phase), (5.0, vec![4.0, 2.0], Some(-1.5)));
    assert_eq!(node(&s, inner), original.node(inner).unwrap().clone(), "K does not mutate the raw main");
    assert_eq!(s.doc().unwrap().variables, globals);
    assert_eq!(node(&s, copy).bound, node(&s, inner).bound);
    assert_eq!(s.undo_depth().0, history + 1);
    let scaled = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &original);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &scaled);
    ex(&mut s, "variable.set", json!({"variable":"Round","value":18}));
    assert_eq!((node(&s, inner).stroke.weight, node(&s, inner).stroke.dashes, node(&s, inner).stroke.dash_phase), (18.0, vec![8.0, 4.0], Some(-3.0)));
    assert_eq!((node(&s, copy).stroke.weight, node(&s, copy).stroke.dashes, node(&s, copy).stroke.dash_phase), (9.0, vec![4.0, 2.0], Some(-1.5)));
    let revalued = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &scaled);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &revalued);
    let mut reopened = reopen_native_command(&s);
    assert_eq!(node(&reopened, instance).variable_scale, Some(0.5));
    assert_eq!(node(&reopened, copy).stroke, node(&s, copy).stroke);
    assert_eq!(node(&reopened, copy).bound, node(&s, copy).bound);
    ex(&mut reopened, "variable.set", json!({"variable":"Round","value":22}));
    assert_eq!(node(&reopened, inner).stroke.weight, 22.0);
    assert_eq!(
        (node(&reopened, copy).stroke.weight, node(&reopened, copy).stroke.dashes, node(&reopened, copy).stroke.dash_phase),
        (11.0, vec![4.0, 2.0], Some(-1.5))
    );
}
