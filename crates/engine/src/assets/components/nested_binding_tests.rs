use serde_json::json;

use super::rebase_instance;
use super::tests::{button, copy_of, ex, fill, idv, node};
use crate::{NodeId, Session};

fn bound_shape_in_button() -> (Session, NodeId, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    let (_, shape, button, nested) = button(&mut s);
    ex(&mut s, "variable.createCollection", json!({"name": "T"}));
    ex(&mut s, "variable.create", json!({"collection": "T", "name": "ink", "type": "color", "value": "#111111"}));
    let inner = copy_of(&s, nested, shape);
    ex(&mut s, "variable.bind", json!({"ids": [inner.0], "field": "fill", "variable": "ink"}));
    assert!(node(&s, inner).overrides.contains("fills"));
    (s, shape, button, nested, inner)
}

#[test]
fn relinking_an_instance_keeps_a_binding_its_main_lacks_as_an_override() {
    let (mut s, shape, button, nested, inner) = bound_shape_in_button();
    let doc = s.ds_mut().unwrap().doc_mut();
    doc.get_mut(inner).unwrap().overrides.clear();
    rebase_instance(doc, nested);
    assert!(node(&s, inner).overrides.contains("fills"), "the same colour from a variable the main doesn't bind");
    ex(&mut s, "node.set", json!({"id": shape, "props": {"fill": "#FF0000"}}));
    assert_eq!(fill(&s, inner), "111111", "an edit of the icon's main leaves the bound colour");
    let placed = idv(&ex(&mut s, "instance.create", json!({"component": button, "x": 400, "y": 400})));
    let copy = copy_of(&s, placed, inner);
    assert_eq!(node(&s, copy).bound.values().collect::<Vec<_>>(), [&node(&s, inner).bound["fill"]]);
}

#[test]
fn hiding_and_showing_a_bound_fill_keeps_its_override() {
    let (mut s, _, _, _, inner) = bound_shape_in_button();
    ex(&mut s, "fill.set", json!({"ids": [inner.0], "index": 0, "visible": false}));
    ex(&mut s, "fill.set", json!({"ids": [inner.0], "index": 0, "visible": true}));
    let n = node(&s, inner);
    assert!(n.bound.contains_key("fill"));
    assert!(n.overrides.contains("fills"), "the binding still differs from the main's");
}

#[test]
fn equivalent_binding_keys_follow_the_main_across_mode_changes() {
    let (mut s, shape, _, nested, inner) = bound_shape_in_button();
    let variable = node(&s, inner).bound["fill"].clone();
    let doc = s.ds_mut().unwrap().doc_mut();
    let main = doc.get_mut(shape).unwrap();
    main.bound.insert("fills/0/color".into(), variable);
    // An inherited colour binding can have a different resolved value in the placed instance.
    let copy = doc.get_mut(inner).unwrap();
    copy.fills[0] = figcraft_color::Paint::solid(figcraft_color::Color::parse_css("#336699").unwrap());
    copy.overrides.clear();
    rebase_instance(doc, nested);
    assert!(!node(&s, inner).overrides.contains("fills"));
}

#[test]
fn a_top_level_instance_s_own_resizing_binding_is_not_an_override() {
    let mut s = Session::new();
    let (_, _, button, _) = button(&mut s);
    let placed = idv(&ex(&mut s, "instance.create", json!({"component": button, "x": 400, "y": 400})));
    let doc = s.ds_mut().unwrap().doc_mut();
    doc.get_mut(placed).unwrap().bound.insert("minWidth".into(), "VariableID:1:1".into());
    rebase_instance(doc, placed);
    assert!(!node(&s, placed).overrides.contains("resizing"), "a top-level instance's resizing is its own");
}
