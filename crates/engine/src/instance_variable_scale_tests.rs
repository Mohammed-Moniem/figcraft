//! Original #2090 command workflows. A scale changes presentation while retaining variables.
mod history_controls;
mod shared_style_controls;

use crate::Session;
use crate::components::tests::{copy_of, ex, idv, node};
use figcraft_doc::{NodeId, Sizing};
use serde_json::json;

fn scene(bind: bool) -> (Session, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    ex(&mut s, "variable.createCollection", json!({"name":"Original dimensions"}));
    ex(&mut s, "variable.create", json!({"collection":"Original dimensions","name":"Space","type":"number","value":32}));
    ex(&mut s, "variable.create", json!({"collection":"Original dimensions","name":"Round","type":"number","value":10}));
    let frame = idv(&ex(&mut s, "frame.create", json!({"width":240,"height":180,"name":"Original component"})));
    let inner = idv(&ex(
        &mut s,
        "frame.create",
        json!({"parent":frame,"x":12,"y":12,"width":160,"height":120,"props":{"layout":"vertical","gap":32,"padding":8,"cornerRadius":10}}),
    ));
    for y in [0, 64] {
        ex(&mut s, "shape.rectangle", json!({"parent":inner,"x":0,"y":y,"width":40,"height":20}));
    }
    if bind {
        ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"gap","variable":"Space"}));
        ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"cornerRadius","variable":"Round"}));
    }
    let component = idv(&ex(&mut s, "component.create", json!({"ids":[frame]})));
    let instance = idv(&ex(&mut s, "instance.create", json!({"component":component,"x":320,"y":0})));
    (s, component, inner, instance)
}

#[test]
fn instance_variable_scale_preserves_bound_dimensions_and_one_undo() {
    let (mut s, _component, inner, instance) = scene(true);
    let copy = copy_of(&s, instance, inner);
    let original = s.doc().unwrap().clone();
    let history = s.undo_depth().0;
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let scaled = node(&s, copy);
    assert_eq!(scaled.layout.as_ref().unwrap().gap, 16.0, "a retained gap binding is presented at half scale");
    assert_eq!(scaled.corners.radii, [5.0; 4], "a retained radius binding is presented at half scale");
    assert_eq!(scaled.bound, original.node(copy).unwrap().bound);
    assert_eq!(node(&s, inner), original.node(inner).unwrap().clone(), "the main stays unchanged");
    assert_eq!(s.undo_depth().0, history + 1);
    let scaled_doc = s.doc().unwrap().clone();
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &original);
    s.redo().unwrap();
    assert_eq!(s.doc().unwrap(), &scaled_doc);
}

#[test]
fn instance_variable_scale_follows_new_variable_values_at_the_same_scale() {
    let (mut s, _component, inner, instance) = scene(true);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
    ex(&mut s, "variable.set", json!({"variable":"Round","value":14}));
    let copy = copy_of(&s, instance, inner);
    let scaled = node(&s, copy);
    assert_eq!(scaled.layout.as_ref().unwrap().gap, 24.0);
    assert_eq!(scaled.corners.radii, [7.0; 4]);
    assert_eq!(node(&s, inner).layout.as_ref().unwrap().gap, 48.0);
    assert_eq!(node(&s, inner).corners.radii, [14.0; 4]);
    assert_eq!(scaled.bound, node(&s, inner).bound);
}

#[test]
fn instance_variable_scale_unbound_control_retains_existing_scale() {
    let (mut s, _component, inner, instance) = scene(false);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let scaled = node(&s, copy_of(&s, instance, inner));
    assert_eq!(scaled.layout.as_ref().unwrap().gap, 16.0);
    assert_eq!(scaled.corners.radii, [5.0; 4]);
    assert_eq!(node(&s, inner).layout.as_ref().unwrap().gap, 32.0);
}

#[test]
fn instance_variable_scale_radius_binding_is_not_reset_after_scaling() {
    let (mut s, _component, inner, instance) = scene(true);
    let copy = copy_of(&s, instance, inner);
    let bindings = node(&s, copy).bound;
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    assert_eq!(node(&s, copy).corners.radii, [5.0; 4], "the independently observed radius follows instance scale");
    assert_eq!(node(&s, copy).bound, bindings);
}

#[test]
fn instance_variable_scale_cross_fill_keeps_hug_and_matches_existing_copy() {
    let mut s = Session::new();
    let frame =
        idv(&ex(&mut s, "frame.create", json!({"width":200,"height":60,"props":{"layout":"horizontal","padding":6,"gap":12,"sizingV":"hug"}})));
    let bar = idv(&ex(&mut s, "shape.rectangle", json!({"parent":frame,"width":3,"height":20,"name":"Accent"})));
    ex(&mut s, "shape.rectangle", json!({"parent":frame,"width":50,"height":44,"name":"Tall sibling"}));
    let component = idv(&ex(&mut s, "component.create", json!({"ids":[frame]})));
    let instance = idv(&ex(&mut s, "instance.create", json!({"component":component,"x":320,"y":0})));
    ex(&mut s, "node.set", json!({"ids":[bar],"props":{"sizingV":"fill"}}));
    assert_eq!(node(&s, component).child_layout.sizing_v, Sizing::Hug, "cross-axis Fill keeps a hugging row hugging");
    assert_eq!(node(&s, instance).child_layout.sizing_v, Sizing::Hug);
    let main = node(&s, bar);
    let copy = node(&s, copy_of(&s, instance, bar));
    assert_eq!(main.height, 44.0);
    assert_eq!(copy.height, main.height, "the existing instance must relayout on the same edit");
    assert_eq!(copy.child_layout.sizing_v, Sizing::Fill);
}

fn reopen_native(s: &Session) -> Session {
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (doc, warnings) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    Session::with_document(doc)
}

#[test]
fn instance_variable_scale_native_bytes_keep_bindings_and_accumulated_context() {
    let (mut s, _component, inner, instance) = scene(true);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let before = node(&s, copy_of(&s, instance, inner));
    let mut reopened = reopen_native(&s);
    assert_eq!(node(&reopened, instance).variable_scale, Some(0.5));
    assert_eq!(node(&reopened, copy_of(&reopened, instance, inner)), before);
    ex(&mut reopened, "variable.set", json!({"variable":"Space","value":52}));
    assert_eq!(node(&reopened, copy_of(&reopened, instance, inner)).layout.unwrap().gap, 26.0);
}

#[test]
fn instance_variable_scale_detach_at_one_then_reopen_and_first_scale() {
    let (mut s, _component, inner, instance) = scene(true);
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "component.detach", json!({"ids":[instance]}));
    let mut s = reopen_native(&s);
    assert_eq!(node(&s, instance).variable_scale, None);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
    assert_eq!(node(&s, copy).layout.unwrap().gap, 24.0);
    assert!(!node(&s, copy).bound.is_empty());
}

#[test]
fn instance_variable_scale_repeated_scale_and_ordinary_resize_have_distinct_meanings() {
    let (mut s, _component, inner, instance) = scene(true);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    ex(&mut s, "node.set", json!({"ids":[instance],"props":{"width":180}}));
    assert_eq!(node(&s, copy_of(&s, instance, inner)).layout.unwrap().gap, 16.0);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":2}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":50}));
    assert_eq!(node(&s, instance).variable_scale, Some(1.0));
    assert_eq!(node(&s, copy_of(&s, instance, inner)).layout.unwrap().gap, 50.0);
    assert_eq!(node(&s, inner).variable_scale, None);
}

#[test]
fn instance_variable_scale_modes_and_newly_introduced_bindings_follow_context() {
    let (mut s, _component, inner, instance) = scene(true);
    ex(&mut s, "variable.addMode", json!({"collection":"Original dimensions","name":"Large"}));
    ex(&mut s, "variable.set", json!({"variable":"Space","mode":"Large","value":60}));
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    ex(&mut s, "variable.setMode", json!({"ids":[instance],"collection":"Original dimensions","mode":"Large"}));
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"paddingHorizontal","variable":"Round"}));
    let scaled = node(&s, copy_of(&s, instance, inner));
    assert_eq!(scaled.layout.as_ref().unwrap().gap, 30.0);
    assert_eq!(scaled.layout.as_ref().unwrap().padding[1], 5.0);
    assert_eq!(node(&s, inner).layout.unwrap().padding[1], 10.0);
    assert_eq!(scaled.bound, node(&s, inner).bound);
}

fn bound_gap(s: &Session, root: NodeId) -> f64 {
    s.doc()
        .unwrap()
        .subtree(root)
        .into_iter()
        .find_map(|id| {
            let n = s.doc().unwrap().node(id)?;
            n.bound.contains_key("gap").then(|| n.layout.as_ref().unwrap().gap)
        })
        .unwrap()
}

fn trace_scale_context(s: &Session, root: NodeId, label: &str) {
    for id in s.doc().unwrap().subtree(root) {
        let n = node(s, id);
        eprintln!(
            "{label}: id={id:?} source={:?} scale={:?} overrides={:?} bound={:?} gap={:?}",
            n.source,
            n.variable_scale,
            n.overrides,
            n.bound,
            n.layout.as_ref().map(|l| l.gap)
        );
    }
}

#[test]
fn instance_variable_scale_nested_contexts_follow_main_then_keep_true_local_override_and_reset() {
    let (mut s, component, _inner, _existing) = scene(true);
    let outer = idv(&ex(&mut s, "frame.create", json!({"width":600,"height":400})));
    let nested = idv(&ex(&mut s, "instance.create", json!({"component":component,"parent":outer,"x":0,"y":0})));
    ex(&mut s, "transform.scale", json!({"ids":[nested],"factor":0.5}));
    let outer = idv(&ex(&mut s, "component.create", json!({"ids":[outer]})));
    let copy = idv(&ex(&mut s, "instance.create", json!({"component":outer,"x":700,"y":0})));
    ex(&mut s, "transform.scale", json!({"ids":[copy],"factor":0.5}));
    trace_scale_context(&s, copy, "outer scaled");
    assert_eq!(bound_gap(&s, copy), 8.0, "each nested factor applies exactly once");
    ex(&mut s, "transform.scale", json!({"ids":[nested],"factor":0.5}));
    trace_scale_context(&s, nested, "nested main scaled again");
    trace_scale_context(&s, copy, "copy after main scale");
    assert_eq!(bound_gap(&s, copy), 4.0, "the nested main's scale still propagates");
    let nested_copy = copy_of(&s, copy, nested);
    ex(&mut s, "transform.scale", json!({"ids":[nested_copy],"factor":3}));
    assert_eq!(bound_gap(&s, copy), 12.0);
    assert!(node(&s, nested_copy).overrides.contains("variableScale"));
    ex(&mut s, "transform.scale", json!({"ids":[nested],"factor":2}));
    assert_eq!(bound_gap(&s, copy), 12.0, "a true local context override survives main edits");
    ex(&mut s, "component.resetOverride", json!({"id":nested_copy,"group":"variableScale"}));
    assert_eq!(bound_gap(&s, copy), 8.0);
    assert!(!node(&s, nested_copy).overrides.contains("variableScale"));
}

#[test]
fn instance_variable_scale_lists_scale_lengths_but_preserve_counts_angles_and_opacity() {
    use figcraft_doc::EffectKind;
    let (mut s, _component, inner, instance) = scene(true);
    ex(&mut s, "node.set", json!({"ids":[inner],"props":{"grids":[{}],"effects":[{"type":"dropShadow"},{"type":"glass"}]}}));
    for field in [
        "opacity",
        "layoutGrids/0/count",
        "layoutGrids/0/size",
        "layoutGrids/0/gutter",
        "layoutGrids/0/offset",
        "effects/0/offsetX",
        "effects/0/radius",
        "effects/1/depth",
        "effects/1/lightAngle",
        "effects/1/lightIntensity",
    ] {
        ex(&mut s, "variable.bind", json!({"ids":[inner],"field":field,"variable":"Space"}));
    }
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let scaled = node(&s, copy_of(&s, instance, inner));
    assert_eq!(scaled.opacity, 0.32);
    assert_eq!(scaled.grids[0].count, 32);
    assert_eq!((scaled.grids[0].size, scaled.grids[0].gutter, scaled.grids[0].offset), (16.0, 16.0, 16.0));
    let EffectKind::DropShadow(shadow) = &scaled.effects[0].kind else { panic!("shadow") };
    assert_eq!((shadow.offset.0, shadow.radius), (16.0, 16.0));
    let EffectKind::Glass(glass) = &scaled.effects[1].kind else { panic!("glass") };
    assert_eq!((glass.depth, glass.light_angle, glass.light_intensity), (16.0, 32.0, 0.32));
    assert_eq!(scaled.bound, node(&s, inner).bound);
}

#[test]
fn instance_variable_scale_nonrepresentable_context_is_refused_before_geometry_changes() {
    let (mut s, _component, _inner, instance) = scene(true);
    s.doc_mut().unwrap().get_mut(instance).unwrap().variable_scale = Some(f64::MAX);
    let before = s.doc().unwrap().clone();
    assert!(s.execute("transform.scale", &json!({"ids":[instance],"factor":2})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn instance_variable_scale_descendant_overflow_is_refused_before_any_mutation() {
    let (mut s, _component, inner, instance) = scene(true);
    let copy = copy_of(&s, instance, inner);
    s.doc_mut().unwrap().get_mut(copy).unwrap().variable_scale = Some(f64::MAX);
    let before = s.doc().unwrap().clone();
    assert!(crate::cmd::arrange::scale_node(s.doc_mut().unwrap(), instance, 2.0).is_err());
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn instance_variable_scale_factor_one_is_a_storage_noop_and_cycles_are_refused() {
    let (mut s, _component, _inner, instance) = scene(true);
    let before = s.doc().unwrap().clone();
    crate::cmd::arrange::scale_node(s.doc_mut().unwrap(), instance, 1.0).unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.doc_mut().unwrap().get_mut(instance).unwrap().parent = Some(instance);
    assert_eq!(crate::variable_scale::factor(s.doc().unwrap(), instance), None);
}

#[test]
fn instance_variable_scale_new_main_binding_after_reopen_keeps_scaled_unbound_sides() {
    let (mut s, _component, inner, instance) = scene(true);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let copy = copy_of(&s, instance, inner);
    assert!(node(&s, copy).scale_derived_overrides.contains("padding"));
    let mut s = reopen_native(&s);
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"paddingHorizontal","variable":"Round"}));
    assert_eq!(node(&s, copy).layout.unwrap().padding, [4.0, 5.0, 4.0, 5.0]);
    ex(&mut s, "variable.set", json!({"variable":"Round","value":14}));
    assert_eq!(node(&s, copy).layout.unwrap().padding, [4.0, 7.0, 4.0, 7.0]);
}

#[test]
fn instance_variable_scale_mixed_batch_explicit_padding_remains_local_after_new_main_binding() {
    let (mut s, _component, inner, instance) = scene(true);
    let copy = copy_of(&s, instance, inner);
    ex(
        &mut s,
        "command.batch",
        json!({"commands":[
            {"command":"transform.scale","params":{"ids":[instance],"factor":0.5}},
            {"command":"node.set","params":{"ids":[copy],"props":{"paddingHorizontal":7}}}
        ]}),
    );
    assert!(!node(&s, copy).scale_derived_overrides.contains("padding"));
    let before = node(&s, copy).layout.unwrap().padding;
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"paddingHorizontal","variable":"Round"}));
    assert_eq!(node(&s, copy).layout.unwrap().padding, before);
    assert!(!node(&s, copy).bound.contains_key("paddingHorizontal"));
}

#[test]
fn instance_variable_scale_same_value_local_rebinding_and_unbinding_keep_their_ownership() {
    let (mut s, _component, inner, instance) = scene(true);
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "variable.create", json!({"collection":"Original dimensions","name":"Local space","type":"number","value":32}));
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    ex(&mut s, "variable.bind", json!({"ids":[copy],"field":"gap","variable":"Local space"}));
    assert_eq!(node(&s, copy).layout.unwrap().gap, 16.0);
    assert!(!node(&s, copy).scale_derived_overrides.contains("gap"));
    ex(&mut s, "node.set", json!({"ids":[inner],"props":{"name":"Rebuild main"}}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
    assert_eq!(node(&s, copy).layout.unwrap().gap, 16.0);
    ex(&mut s, "variable.bind", json!({"ids":[copy],"field":"gap"}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":64}));
    assert_eq!(node(&s, copy).layout.unwrap().gap, 16.0);
    assert!(!node(&s, copy).bound.contains_key("gap"));
}

#[test]
fn instance_variable_scale_preexisting_padding_override_is_never_reclassified_as_derived() {
    let (mut s, _component, inner, instance) = scene(true);
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "node.set", json!({"ids":[copy],"props":{"paddingHorizontal":20}}));
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    assert!(!node(&s, copy).scale_derived_overrides.contains("padding"));
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"paddingHorizontal","variable":"Round"}));
    assert_eq!(node(&s, copy).layout.unwrap().padding[1], 10.0);
    assert!(!node(&s, copy).bound.contains_key("paddingHorizontal"));
}

#[test]
fn instance_variable_scale_review_grid_same_value_local_binding_survives_rebuild() {
    let (mut s, component, inner, instance) = scene(true);
    ex(&mut s, "node.set", json!({"ids":[inner],"props":{"grids":[{}]}}));
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"layoutGrids/0/size","variable":"Space"}));
    ex(&mut s, "variable.create", json!({"collection":"Original dimensions","name":"Local grid","type":"number","value":32}));
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "variable.bind", json!({"ids":[copy],"field":"layoutGrids/0/size","variable":"Local grid"}));
    assert_eq!(node(&s, copy).grids[0].size, 16.0, "the local bind initially has the same value");
    ex(&mut s, "shape.rectangle", json!({"parent":component,"width":4,"height":4}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
    assert_eq!(node(&s, copy).grids[0].size, 16.0, "same-value local grid binding remains local after main rebuild");
    ex(&mut s, "variable.set", json!({"variable":"Local grid","value":40}));
    assert_eq!(node(&s, copy).grids[0].size, 20.0);
    ex(&mut s, "variable.bind", json!({"ids":[copy],"field":"layoutGrids/0/size"}));
    ex(&mut s, "shape.rectangle", json!({"parent":component,"width":5,"height":5}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":64}));
    assert_eq!(node(&s, copy).grids[0].size, 20.0, "explicit grid unbind remains unbound across rebuild");
}

#[test]
fn instance_variable_scale_review_repeat_font_edit_survives_new_main_binding() {
    let (mut s, component, _inner, instance) = scene(true);
    let text = idv(&ex(&mut s, "text.create", json!({"parent":component,"text":"Original repeated font","props":{"fontSize":20}})));
    let copy = copy_of(&s, instance, text);
    let other = idv(&ex(&mut s, "text.create", json!({"text":"Other text","props":{"fontSize":20}})));
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    ex(&mut s, "select.set", json!({"ids":[other]}));
    ex(&mut s, "text.sizeUp", json!({}));
    ex(&mut s, "select.set", json!({"ids":[copy]}));
    ex(&mut s, "edit.repeat", json!({}));
    let font = |s: &Session| node(s, copy).kind.text().unwrap().style.font_size;
    assert_eq!(font(&s), 11.0);
    ex(&mut s, "variable.bind", json!({"ids":[text],"field":"fontSize","variable":"Space"}));
    assert_eq!(font(&s), 11.0, "Repeat is an explicit font edit and owns its value after a new main binding");
}

#[test]
fn instance_variable_scale_review_repeat_scale_and_batch_keep_derived_bindings() {
    for batch in [false, true] {
        let (mut s, _component, inner, instance) = scene(true);
        let scale = json!({"ids":[instance],"factor":0.5});
        if batch {
            ex(&mut s, "command.batch", json!({"commands":[{"command":"transform.scale","params":scale}]}));
        } else {
            ex(&mut s, "transform.scale", scale);
        }
        ex(&mut s, "edit.repeat", json!({}));
        let copy = copy_of(&s, instance, inner);
        assert_eq!(node(&s, copy).layout.as_ref().unwrap().gap, 8.0);
        ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
        assert_eq!(node(&s, copy).layout.as_ref().unwrap().gap, 12.0);
        ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"paddingHorizontal","variable":"Round"}));
        assert_eq!(node(&s, copy).layout.as_ref().unwrap().padding[1], 2.5, "repeated K keeps inherited binding ownership, batch={batch}");
    }
}

fn reopen_native_command(s: &Session) -> Session {
    use base64::Engine as _;
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let mut reopened = Session::empty();
    ex(&mut reopened, "file.open", json!({"name":"Original-scale-styles.figcraft","data":base64::engine::general_purpose::STANDARD.encode(bytes)}));
    assert!(!reopened.ds().unwrap().is_dirty());
    assert_eq!(reopened.undo_depth(), (0, 0));
    reopened
}

#[test]
fn instance_variable_scale_shared_text_style_retains_binding_and_pixel_fields() {
    let (mut s, component, _inner, instance) = scene(true);
    let text = idv(&ex(
        &mut s,
        "text.create",
        json!({"parent":component,"text":"Original shared text","props":{"fontSize":32,"lineHeight":40,"letterSpacing":2}}),
    ));
    let sid = ex(&mut s, "style.create", json!({"type":"text","name":"Original shared body","from":text}))["id"].as_str().unwrap().to_string();
    ex(&mut s, "style.bind", json!({"style":sid,"field":"fontSize","variable":"Space"}));
    let copy = copy_of(&s, instance, text);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    assert_eq!(node(&s, copy).styles.text, Some(sid.clone()), "K keeps the shared variable style link");
    let scaled = node(&s, copy);
    let style = &scaled.kind.text().unwrap().style;
    assert_eq!(style.font_size, 16.0);
    assert_eq!(style.line_height, figcraft_doc::LineHeight::Pixels(20.0));
    assert_eq!(style.letter_spacing, figcraft_doc::LetterSpacing::Pixels(1.0));
    let mut reopened = reopen_native_command(&s);
    ex(&mut reopened, "variable.set", json!({"variable":"Space","value":48}));
    assert_eq!(node(&reopened, copy).kind.text().unwrap().style.font_size, 24.0);
    assert_eq!(node(&reopened, text).kind.text().unwrap().style.font_size, 48.0);
    assert_eq!(node(&reopened, copy).styles.text, Some(sid.clone()));
    assert_eq!(reopened.doc().unwrap().style(&sid).unwrap().bound.get("fontSize"), node(&s, _inner).bound.get("gap"));
}

#[test]
fn instance_variable_scale_shared_effect_style_retains_binding_after_native_open() {
    let (mut s, _component, inner, instance) = scene(true);
    ex(&mut s, "effect.add", json!({"ids":[inner],"type":"layerBlur"}));
    let sid = ex(&mut s, "style.create", json!({"type":"effect","name":"Original shared blur","from":inner}))["id"].as_str().unwrap().to_string();
    ex(&mut s, "style.bind", json!({"style":sid,"field":"effects/0/radius","variable":"Space"}));
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    assert_eq!(node(&s, copy).styles.effect, Some(sid.clone()), "K keeps the shared effect binding link");
    assert_eq!(node(&s, copy).effects[0].blur().unwrap().radius, 16.0);
    let mut reopened = reopen_native_command(&s);
    ex(&mut reopened, "variable.set", json!({"variable":"Space","value":48}));
    assert_eq!(node(&reopened, copy).effects[0].blur().unwrap().radius, 24.0);
    assert_eq!(node(&reopened, inner).effects[0].blur().unwrap().radius, 48.0);
    assert_eq!(node(&reopened, copy).styles.effect, Some(sid));
}

#[test]
fn instance_variable_scale_shared_grid_style_scales_lengths_and_keeps_count() {
    let (mut s, _component, inner, instance) = scene(true);
    ex(&mut s, "grid.add", json!({"ids":[inner]}));
    let sid = ex(&mut s, "style.create", json!({"type":"grid","name":"Original shared grid","from":inner}))["id"].as_str().unwrap().to_string();
    ex(&mut s, "style.bind", json!({"style":sid,"field":"layoutGrids/0/size","variable":"Space"}));
    ex(&mut s, "style.bind", json!({"style":sid,"field":"layoutGrids/0/count","variable":"Space"}));
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    assert_eq!(node(&s, copy).styles.grid, Some(sid.clone()), "K keeps the shared grid binding link");
    assert_eq!(node(&s, copy).grids[0].size, 16.0);
    assert_eq!(node(&s, copy).grids[0].count, 32);
    let mut reopened = reopen_native_command(&s);
    ex(&mut reopened, "variable.set", json!({"variable":"Space","value":48}));
    assert_eq!(node(&reopened, copy).grids[0].size, 24.0);
    assert_eq!(node(&reopened, copy).grids[0].count, 48);
    assert_eq!(node(&reopened, inner).grids[0].size, 48.0);
    assert_eq!(node(&reopened, copy).styles.grid, Some(sid));
}

#[test]
fn instance_variable_scale_style_capture_after_k_keeps_bound_and_unbound_text_dimensions() {
    let (mut s, component, _inner, instance) = scene(true);
    let text = idv(&ex(
        &mut s,
        "text.create",
        json!({"parent":component,"text":"Original captured type","props":{"fontSize":32,"lineHeight":40,"letterSpacing":2}}),
    ));
    ex(&mut s, "variable.bind", json!({"ids":[text],"field":"fontSize","variable":"Space"}));
    let copy = copy_of(&s, instance, text);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    assert_eq!(node(&s, copy).kind.text().unwrap().style.font_size, 16.0);
    let sid =
        ex(&mut s, "style.create", json!({"type":"text","name":"Original captured bound body","from":copy}))["id"].as_str().unwrap().to_string();
    let shown = node(&s, copy);
    let style = &shown.kind.text().unwrap().style;
    assert_eq!(style.line_height, figcraft_doc::LineHeight::Pixels(20.0), "capture preserves physical unbound line height");
    assert_eq!(style.letter_spacing, figcraft_doc::LetterSpacing::Pixels(1.0));
    assert_eq!(style.font_size, 16.0, "capture preserves physical bound font size");
    assert_eq!(shown.styles.text, Some(sid.clone()));
    let figcraft_doc::StyleKind::Text { style: raw } = &s.doc().unwrap().style(&sid).unwrap().kind else { panic!("text style") };
    assert_eq!(raw.font_size, 32.0);
    assert_eq!(raw.line_height, figcraft_doc::LineHeight::Pixels(40.0), "global base is independent of the capture layer's context");
    assert_eq!(raw.letter_spacing, figcraft_doc::LetterSpacing::Pixels(2.0));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
    assert_eq!(node(&s, copy).kind.text().unwrap().style.font_size, 24.0);
}

#[test]
fn instance_variable_scale_style_capture_unbound_after_k_keeps_appearance_control() {
    let (mut s, component, _inner, instance) = scene(false);
    let text = idv(&ex(
        &mut s,
        "text.create",
        json!({"parent":component,"text":"Original unbound capture","props":{"fontSize":32,"lineHeight":40,"letterSpacing":2}}),
    ));
    let copy = copy_of(&s, instance, text);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let before = node(&s, copy).kind.text().unwrap().style.clone();
    let sid =
        ex(&mut s, "style.create", json!({"type":"text","name":"Original captured literal body","from":copy}))["id"].as_str().unwrap().to_string();
    let shown = node(&s, copy);
    let style = &shown.kind.text().unwrap().style;
    assert_eq!((style.font_size, style.line_height, style.letter_spacing), (before.font_size, before.line_height, before.letter_spacing));
    assert_eq!(shown.styles.text, Some(sid.clone()));
    assert!(s.doc().unwrap().style(&sid).unwrap().bound.is_empty());
}

#[test]
fn instance_variable_scale_style_capture_existing_unbound_link_follows_k_without_changing_raw_style() {
    let (mut s, component, _inner, instance) = scene(false);
    let text = idv(&ex(
        &mut s,
        "text.create",
        json!({"parent":component,"text":"Original literal style","props":{"fontSize":32,"lineHeight":40,"letterSpacing":2}}),
    ));
    let sid =
        ex(&mut s, "style.create", json!({"type":"text","name":"Original shared literal body","from":text}))["id"].as_str().unwrap().to_string();
    let raw = s.doc().unwrap().style(&sid).unwrap().clone();
    let copy = copy_of(&s, instance, text);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    assert_eq!(node(&s, copy).styles.text, Some(sid.clone()), "native K presentation preserves unbound shared links too");
    assert_eq!(node(&s, copy).kind.text().unwrap().style.font_size, 16.0);
    assert_eq!(s.doc().unwrap().style(&sid), Some(&raw));
}
