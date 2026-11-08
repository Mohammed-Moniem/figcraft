//! A selection-opacity preference changes the editor chrome, never document opacity or history.
use serde_json::{Value, json};

use crate::{Prefs, Session};

fn ex(s: &mut Session, params: Value) -> Value {
    s.execute("pref.selectionOutlineOpacity", &params).unwrap()
}

#[test]
fn selection_outline_opacity_command_is_persisted_without_editing_the_document() {
    let mut s = Session::new();
    s.execute("shape.rectangle", &json!({"width":100,"height":80})).unwrap();
    s.ds_mut().unwrap().mark_saved();
    let doc = s.doc().unwrap().clone();
    let selection = s.selection();
    let history = s.undo_depth();
    s.prefs_unsaved = false;
    assert_eq!(ex(&mut s, json!({}))["opacity"], 100.0);
    assert!(!s.prefs_unsaved, "querying is not a preference edit");
    assert_eq!(ex(&mut s, json!({"opacity":25.5}))["opacity"], 25.5);
    assert!(s.prefs_unsaved);
    assert_eq!(serde_json::to_value(&s.prefs).unwrap()["selection_outline_opacity"], 25.5);
    assert_eq!(&doc, s.doc().unwrap());
    assert_eq!(s.selection(), selection);
    assert_eq!(s.undo_depth(), history);
    assert!(!s.ds().unwrap().is_dirty());
    let prefs: Prefs = serde_json::from_value(serde_json::to_value(&s.prefs).unwrap()).unwrap();
    let mut restored = Session::new();
    restored.prefs = prefs;
    assert_eq!(ex(&mut restored, json!({}))["opacity"], 25.5);
    assert_eq!(ex(&mut restored, json!({"reset":true}))["opacity"], 100.0);
}

#[test]
fn selection_outline_opacity_invalid_requests_are_atomic() {
    let mut s = Session::new();
    ex(&mut s, json!({"opacity":45}));
    let before = s.prefs.clone();
    for p in [
        json!({"opacity":-1}),
        json!({"opacity":101}),
        json!({"opacity":"25"}),
        json!({"opacity":[]}),
        json!({"opacity":true}),
        json!({"opacity":1e300}),
        json!({"reset":"yes"}),
        json!({"opacity":25,"reset":true}),
        json!({"opacity":null}),
        json!({"reset":null}),
        json!({"opacity":25,"reset":false}),
    ] {
        s.prefs_unsaved = false;
        assert!(s.execute("pref.selectionOutlineOpacity", &p).is_err(), "{p}");
        assert_eq!(s.prefs, before, "{p} cannot partly mutate preferences");
        assert!(!s.prefs_unsaved);
    }
}

#[test]
fn selection_outline_opacity_old_and_invalid_preferences_keep_the_visible_default() {
    for value in [Value::Null, json!("low"), json!(-1), json!(101), json!([]), json!({})] {
        let prefs: Prefs = serde_json::from_value(json!({"theme":"dark","selection_outline_opacity":value})).unwrap();
        assert_eq!(serde_json::to_value(&prefs).unwrap()["selection_outline_opacity"], 100.0);
        assert_eq!(prefs.theme, "dark");
    }
    let old: Prefs = serde_json::from_value(json!({"theme":"light"})).unwrap();
    assert_eq!(serde_json::to_value(&old).unwrap()["selection_outline_opacity"], 100.0);
    for value in [0.0, 25.5, 100.0] {
        let prefs: Prefs = serde_json::from_value(json!({"selection_outline_opacity":value})).unwrap();
        assert_eq!(serde_json::to_value(&prefs).unwrap()["selection_outline_opacity"], value);
    }
}

#[test]
fn selection_outline_opacity_applies_to_new_files_without_inheriting_hidden_chrome() {
    let mut s = Session::new();
    s.execute("view.selectionOutlines", &json!({"on":false})).unwrap();
    let first = s.ds().unwrap().uid;
    ex(&mut s, json!({"opacity":30}));
    s.execute("file.new", &json!({})).unwrap();
    assert_ne!(s.ds().unwrap().uid, first);
    assert!(s.ds().unwrap().view.selection_outlines, "new files retain the existing visible default");
    assert_eq!(ex(&mut s, json!({}))["opacity"], 30.0, "the saved opacity applies globally");
    assert!(!s.docs.iter().find(|d| d.uid == first).unwrap().view.selection_outlines, "the older view's temporary hide toggle stays independent");
    s.prefs_unsaved = false;
    ex(&mut s, json!({"opacity":30}));
    assert!(!s.prefs_unsaved, "an idempotent setter needs no preference save");
}

#[test]
fn selection_outline_opacity_keeps_text_editing_and_selection_active() {
    let mut s = Session::new();
    let id = s.execute("node.create", &json!({"type":"text","text":"Editable"})).unwrap()["id"].clone();
    s.execute("text.edit.begin", &json!({"id":id,"caret":3})).unwrap();
    let edit = s.ds().unwrap().text_edit.clone();
    let selection = s.selection();
    ex(&mut s, json!({"opacity":0}));
    assert_eq!(s.ds().unwrap().text_edit, edit);
    assert_eq!(s.selection(), selection);
}
