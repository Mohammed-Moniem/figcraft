//! Public history commands restore K-derived ownership; they do not create local edits.

use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn markers(s: &Session) -> BTreeMap<NodeId, BTreeSet<String>> {
    s.doc().unwrap().nodes().filter(|n| !n.scale_derived_overrides.is_empty()).map(|n| (n.id, n.scale_derived_overrides.clone())).collect()
}

#[test]
fn instance_variable_scale_history_public_redo_restores_exact_derived_snapshot() {
    let (s, _main, inner, instance) = scene(true);
    let mut s = reopen_native(&s);
    assert_eq!(s.undo_depth(), (0, 0), "a clean native round trip starts with no history");
    let before = s.doc().unwrap().clone();
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let copy = copy_of(&s, instance, inner);
    let expected = markers(&s);
    assert!(expected[&copy].contains("padding"));
    let scaled = s.doc().unwrap().clone();
    assert_eq!(s.undo_depth(), (1, 0));
    assert_eq!(ex(&mut s, "edit.undo", json!({}))["undone"], true);
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), (0, 1));
    assert_eq!(ex(&mut s, "edit.redo", json!({}))["redone"], true);
    assert_eq!(node(&s, copy).layout.unwrap().gap, 16.0);
    assert_eq!(markers(&s), expected, "public redo must restore ownership, just like direct Session::redo");
    assert_eq!(s.doc().unwrap(), &scaled);
    assert_eq!(s.undo_depth(), (1, 0));
}

#[test]
fn instance_variable_scale_history_native_sequence_follows_a_new_main_binding() {
    let (s, _main, inner, instance) = scene(true);
    let mut s = reopen_native(&s);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    ex(&mut s, "edit.undo", json!({}));
    ex(&mut s, "edit.redo", json!({}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
    ex(&mut s, "variable.set", json!({"variable":"Round","value":14}));
    let copy = copy_of(&s, instance, inner);
    assert_eq!(node(&s, copy).layout.unwrap().gap, 24.0);
    assert_eq!(node(&s, copy).corners.radii, [7.0; 4]);
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(node(&s, copy).corners.radii, [5.0; 4]);
    assert_eq!(node(&s, copy).layout.unwrap().gap, 24.0);
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(node(&s, copy).layout.unwrap().gap, 16.0);
    let mut s = reopen_native(&s);
    assert_eq!(s.undo_depth(), (0, 0));
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"paddingHorizontal","variable":"Round"}));
    assert_eq!(node(&s, copy).layout.unwrap().padding, [4.0, 5.0, 4.0, 5.0], "a later main binding must replace only the derived horizontal sides");
    ex(&mut s, "variable.set", json!({"variable":"Round","value":14}));
    assert_eq!(node(&s, copy).layout.unwrap().padding, [4.0, 7.0, 4.0, 7.0]);
    assert_eq!(node(&s, instance).variable_scale, Some(0.5));
    assert_eq!(node(&s, copy).bound, node(&s, inner).bound);
    assert!(node(&s, copy).scale_derived_overrides.contains("padding"));
}

#[test]
fn instance_variable_scale_history_variable_undo_restores_derived_ownership() {
    let (s, _main, inner, instance) = scene(true);
    let mut s = reopen_native(&s);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let expected = markers(&s);
    ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
    ex(&mut s, "variable.set", json!({"variable":"Round","value":14}));
    assert_eq!(markers(&s), expected, "revaluation is not an explicit local edit");
    ex(&mut s, "edit.undo", json!({}));
    let copy = copy_of(&s, instance, inner);
    assert_eq!(node(&s, copy).layout.unwrap().gap, 24.0);
    assert_eq!(node(&s, copy).corners.radii, [5.0; 4]);
    assert_eq!(markers(&s), expected, "undoing a variable value retains restored corner ownership");
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(node(&s, copy).layout.unwrap().gap, 16.0);
    assert_eq!(markers(&s), expected);
    ex(&mut s, "edit.redo", json!({}));
    ex(&mut s, "edit.redo", json!({}));
    assert_eq!(node(&s, copy).layout.unwrap().gap, 24.0);
    assert_eq!(node(&s, copy).corners.radii, [7.0; 4]);
    assert_eq!(markers(&s), expected);
}

#[test]
fn instance_variable_scale_history_explicit_edits_and_same_value_rebind_still_retire() {
    let (s, _main, inner, instance) = scene(true);
    let mut s = reopen_native(&s);
    ex(&mut s, "transform.scale", json!({"ids":[instance],"factor":0.5}));
    let copy = copy_of(&s, instance, inner);
    ex(&mut s, "node.set", json!({"ids":[copy],"props":{"paddingHorizontal":7}}));
    assert_eq!(node(&s, copy).layout.unwrap().padding, [4.0, 7.0, 4.0, 7.0]);
    assert!(!node(&s, copy).scale_derived_overrides.contains("padding"));
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(node(&s, copy).layout.unwrap().padding, [4.0; 4]);
    assert!(node(&s, copy).scale_derived_overrides.contains("padding"), "undo restores the earlier derived ownership");
    ex(&mut s, "edit.redo", json!({}));
    assert!(!node(&s, copy).scale_derived_overrides.contains("padding"), "redo restores the explicit local choice");
    ex(&mut s, "variable.create", json!({"collection":"Original dimensions","name":"Local history space","type":"number","value":32}));
    let inherited = node(&s, copy).bound["gap"].clone();
    ex(&mut s, "variable.bind", json!({"ids":[copy],"field":"gap","variable":"Local history space"}));
    let local = node(&s, copy).bound["gap"].clone();
    assert_ne!(local, inherited);
    assert_eq!(node(&s, copy).layout.unwrap().gap, 16.0);
    assert!(!node(&s, copy).scale_derived_overrides.contains("gap"), "same-value rebind is still explicit ownership");
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(node(&s, copy).bound["gap"], inherited);
    assert!(node(&s, copy).scale_derived_overrides.contains("gap"));
    ex(&mut s, "edit.redo", json!({}));
    assert_eq!(node(&s, copy).bound["gap"], local);
    assert!(!node(&s, copy).scale_derived_overrides.contains("gap"));
    let mut s = reopen_native(&s);
    ex(&mut s, "variable.bind", json!({"ids":[inner],"field":"paddingHorizontal","variable":"Round"}));
    ex(&mut s, "variable.set", json!({"variable":"Round","value":14}));
    ex(&mut s, "variable.set", json!({"variable":"Space","value":48}));
    assert_eq!(node(&s, copy).layout.unwrap().padding, [4.0, 7.0, 4.0, 7.0]);
    assert_eq!(node(&s, copy).layout.unwrap().gap, 16.0);
    assert!(!node(&s, copy).bound.contains_key("paddingHorizontal"));
    assert!(!node(&s, copy).scale_derived_overrides.contains("padding"));
    assert!(!node(&s, copy).scale_derived_overrides.contains("gap"));
}
