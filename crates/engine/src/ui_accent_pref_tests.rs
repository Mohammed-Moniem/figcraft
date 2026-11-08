//! A UI accent is an app preference: it survives saved settings without editing any document.
use crate::{Prefs, Session};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn accent_command_normalizes_persists_and_leaves_document_history_alone() {
    let mut s = Session::new();
    s.execute("shape.rectangle", &json!({"width":40,"height":20})).unwrap();
    let doc = s.ds().unwrap().doc.clone();
    let history = s.undo_depth();
    let selection = s.selection().to_vec();
    assert_eq!(s.execute("pref.accentColor", &json!({"color":" #0d99ff "})).unwrap(), json!({"color":"#0D99FF"}));
    assert_eq!(s.execute("pref.accentColor", &json!({})).unwrap(), json!({"color":"#0D99FF"}));
    let saved = serde_json::to_value(&s.prefs).unwrap();
    assert_eq!(saved["accent"], "#0D99FF");
    let restored: Prefs = serde_json::from_value(saved).unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap()["accent"], "#0D99FF");
    assert!(Arc::ptr_eq(&doc, &s.ds().unwrap().doc));
    assert_eq!(s.undo_depth(), history);
    assert_eq!(s.selection(), selection);
}

#[test]
fn accent_presets_and_reset_have_explicit_canonical_colors() {
    let mut s = Session::new();
    for (preset, color) in [("green", "#02DA96"), ("blue", "#0D99FF"), ("purple", "#9747FF"), ("orange", "#E67E22")] {
        assert_eq!(s.execute("pref.accentColor", &json!({"preset":preset})).unwrap(), json!({"color":color}));
    }
    assert_eq!(s.execute("pref.accentColor", &json!({"reset":true})).unwrap(), json!({"color":"#02DA96"}));
}

#[test]
fn invalid_accent_changes_are_atomic() {
    let mut s = Session::new();
    s.execute("pref.accentColor", &json!({"color":"#9747FF"})).unwrap();
    let prefs = s.prefs.clone();
    let doc = s.ds().unwrap().doc.clone();
    let history = s.undo_depth();
    for input in [
        json!({"color":"#abc"}),
        json!({"color":"#11223344"}),
        json!({"color":"112233"}),
        json!({"color":"#12ZZ00"}),
        json!({"color":"#１２３４５６"}),
        json!({"color":17}),
        json!({"color":null}),
        json!({"preset":"pink"}),
        json!({"preset":false}),
        json!({"reset":false}),
        json!({"reset":"true"}),
        json!({"color":"#000000","reset":true}),
        json!({"color":"#000000","preset":"blue"}),
    ] {
        assert!(s.execute("pref.accentColor", &input).is_err(), "{input}");
        assert_eq!(s.prefs, prefs, "{input}");
        assert!(Arc::ptr_eq(&doc, &s.ds().unwrap().doc));
        assert_eq!(s.undo_depth(), history);
    }
}

#[test]
fn old_or_malformed_saved_accents_keep_other_preferences() {
    for value in [Value::Null, json!(false), json!(123), json!("red"), json!("#xyzxyz"), json!([1, 2, 3])] {
        let prefs: Prefs = serde_json::from_value(json!({"accent":value,"nudge_small":2.5,"theme":"dark"})).unwrap();
        let saved = serde_json::to_value(prefs).unwrap();
        assert_eq!(saved["accent"], "#02DA96");
        assert_eq!(saved["nudge_small"], 2.5);
        assert_eq!(saved["theme"], "dark");
    }
    let old: Prefs = serde_json::from_value(json!({"theme":"light"})).unwrap();
    assert_eq!(serde_json::to_value(old).unwrap()["accent"], "#02DA96");
    let prefs: Prefs = serde_json::from_value(json!({"accent":"#0d99ff"})).unwrap();
    assert_eq!(serde_json::to_value(prefs).unwrap()["accent"], "#0D99FF");
}
