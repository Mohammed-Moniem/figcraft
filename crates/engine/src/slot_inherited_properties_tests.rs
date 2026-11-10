//! Original synthetic public-command fixtures for inherited instance-owned slot properties.

use crate::Session;
use crate::components::tests::{chars, copy_of, ex, healthy, idv, kids, node};
use figcraft_doc::NodeId;
use serde_json::json;

fn definitions() -> (Session, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    let content = idv(&ex(&mut s, "node.create", json!({"type":"component","name":"Content"})));
    let label = idv(&ex(&mut s, "node.create", json!({"type":"text","parent":content,"name":"Label","text":"Default"})));
    ex(&mut s, "component.addProperty", json!({"id":content,"name":"Text","type":"text","layer":label,"default":"Default"}));
    let base = idv(&ex(&mut s, "node.create", json!({"type":"component","name":"Base"})));
    let slot = idv(&ex(&mut s, "node.create", json!({"type":"frame","parent":base,"name":"Body"})));
    ex(&mut s, "component.makeSlot", json!({"id":slot,"name":"Body"}));
    (s, content, label, base)
}

struct Fixture {
    s: Session,
    composition: NodeId,
    root: NodeId,
    content: NodeId,
    label: NodeId,
    source_label: NodeId,
    seed_content: NodeId,
    seed_label: NodeId,
}

fn inherited(exposed: bool) -> Fixture {
    let (mut s, content, source_label, base) = definitions();
    let composition = idv(&ex(&mut s, "node.create", json!({"type":"component","name":"Composition"})));
    let base_instance = idv(&ex(&mut s, "instance.create", json!({"component":base,"parent":composition})));
    let seed_content = idv(&ex(&mut s, "instance.slotInsert", json!({"id":base_instance,"slot":"Body","component":content})));
    let seed_label = kids(&s, seed_content)[0];
    if exposed {
        ex(&mut s, "component.exposeInstance", json!({"id":seed_content}));
        ex(&mut s, "component.exposeInstance", json!({"id":base_instance}));
    }
    let root = idv(&ex(&mut s, "instance.create", json!({"component":composition})));
    let copied_content = copy_of(&s, root, seed_content);
    let label = copy_of(&s, root, seed_label);
    healthy(&s);
    Fixture { s, composition, root, content: copied_content, label, source_label, seed_content, seed_label }
}

fn set_text(s: &mut Session, id: NodeId, value: &str) {
    ex(s, "instance.setProperties", json!({"ids":[id],"properties":{"Text":value}}));
}

fn effective(s: &mut Session, content: NodeId, label: NodeId, expected: &str) {
    let property = ex(s, "component.properties", json!({"id":content}));
    assert_eq!(property["properties"][0]["value"], expected);
    assert_eq!(ex(s, "node.get", json!({"id":label}))["characters"], expected);
    let code = ex(s, "inspect.code", json!({"id":content,"language":"html"}));
    assert!(code["code"].as_str().unwrap().contains(&format!(">{expected}</p>")));
    healthy(s);
}

#[test]
fn issue_2124_exposed_inherited_slot_text_uses_the_nested_instance_property() {
    let mut f = inherited(true);
    set_text(&mut f.s, f.content, "Custom");
    effective(&mut f.s, f.content, f.label, "Custom");
    assert_eq!(chars(&f.s, f.source_label), "Default");
    assert_eq!(chars(&f.s, f.seed_label), "Default");
}

#[test]
fn issue_2124_unexposed_inherited_slot_does_not_require_a_parent_property_binding() {
    let mut f = inherited(false);
    set_text(&mut f.s, f.content, "Custom");
    effective(&mut f.s, f.content, f.label, "Custom");
}

#[test]
fn issue_2124_direct_slot_instance_remains_an_independent_control() {
    let (mut s, content, source_label, base) = definitions();
    let root = idv(&ex(&mut s, "instance.create", json!({"component":base})));
    let nested = idv(&ex(&mut s, "instance.slotInsert", json!({"id":root,"slot":"Body","component":content})));
    let label = copy_of(&s, nested, source_label);
    set_text(&mut s, nested, "Custom");
    effective(&mut s, nested, label, "Custom");
}

#[test]
fn issue_2124_same_property_name_in_the_outer_main_has_an_independent_namespace() {
    let mut f = inherited(false);
    let header = idv(&ex(&mut f.s, "node.create", json!({"type":"text","parent":f.composition,"name":"Outer label","text":"Outer default"})));
    ex(&mut f.s, "component.addProperty", json!({"id":f.composition,"name":"Text","type":"text","layer":header,"default":"Outer default"}));
    let outer_label = copy_of(&f.s, f.root, header);
    set_text(&mut f.s, f.root, "Outer custom");
    assert_eq!(chars(&f.s, outer_label), "Outer custom");
    set_text(&mut f.s, f.content, "Inner custom");
    effective(&mut f.s, f.content, f.label, "Inner custom");
    assert_eq!(chars(&f.s, outer_label), "Outer custom", "the same property name belongs to two distinct components");
    healthy(&f.s);
}

#[test]
fn issue_2124_explicit_character_override_stays_above_the_property() {
    let mut f = inherited(false);
    ex(&mut f.s, "text.set", json!({"id":f.label,"characters":"Manual"}));
    assert!(node(&f.s, f.label).overrides.contains("characters"));
    set_text(&mut f.s, f.content, "Custom");
    assert_eq!(chars(&f.s, f.label), "Manual");
    assert!(node(&f.s, f.label).overrides.contains("characters"));
    healthy(&f.s);
}

#[test]
fn issue_2124_public_undo_redo_restores_property_and_effective_text_together() {
    let mut f = inherited(true);
    let before = f.s.doc().unwrap().clone();
    set_text(&mut f.s, f.content, "Custom");
    effective(&mut f.s, f.content, f.label, "Custom");
    let after = f.s.doc().unwrap().clone();
    ex(&mut f.s, "edit.undo", json!({}));
    assert_eq!(f.s.doc().unwrap(), &before);
    ex(&mut f.s, "edit.redo", json!({}));
    assert_eq!(f.s.doc().unwrap(), &after);
    effective(&mut f.s, f.content, f.label, "Custom");
}

#[test]
fn issue_2124_two_outer_copies_keep_independent_nested_text_values() {
    let mut f = inherited(false);
    let other_root = idv(&ex(&mut f.s, "instance.create", json!({"component":f.composition})));
    let other_content = copy_of(&f.s, other_root, f.seed_content);
    let other_label = copy_of(&f.s, other_root, f.seed_label);
    set_text(&mut f.s, f.content, "First");
    assert_eq!(chars(&f.s, other_label), "Default");
    set_text(&mut f.s, other_content, "Second");
    effective(&mut f.s, f.content, f.label, "First");
    effective(&mut f.s, other_content, other_label, "Second");
    assert_eq!(chars(&f.s, f.source_label), "Default");
}
