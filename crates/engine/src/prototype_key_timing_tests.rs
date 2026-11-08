//! Original #555 API controls. Presenter timing is proved separately with real input frames.

use figcraft_doc::NodeId;
use serde_json::{Value, json};

use crate::Session;

fn frame(s: &mut Session) -> NodeId {
    let value = s.execute("frame.create", &json!({"name":"Original key timing screen", "width":200, "height":160})).unwrap();
    NodeId(value["id"].as_u64().unwrap())
}

fn add(s: &mut Session, id: NodeId, trigger: Value) -> crate::Result<Value> {
    s.execute("proto.interaction.add", &json!({"id":id.0, "trigger":trigger, "action":{"type":"back"}}))
}

#[test]
fn key_timing_api_authors_both_phases_and_native_archive_keeps_their_minimum_hold() {
    let mut s = Session::new();
    let id = frame(&mut s);
    for kind in ["keyDown", "keyUp"] {
        let trigger = json!({"type":kind,"key":"Shift+A","ms":500});
        let result = add(&mut s, id, trigger.clone());
        assert!(result.is_ok(), "#555 must author {kind} with a minimum hold: {result:?}");
        let stored = &s.doc().unwrap().node(id).unwrap().interactions.last().unwrap().trigger;
        assert_eq!(serde_json::to_value(stored).unwrap(), trigger);
    }
    let before = s.doc().unwrap().node(id).unwrap().interactions.clone();
    let bytes = figcraft_format::save_bytes(s.doc().unwrap(), None).unwrap();
    let (loaded, notes) = figcraft_format::load_bytes(&bytes).unwrap();
    assert!(notes.is_empty());
    assert_eq!(loaded.node(id).unwrap().interactions, before);
}

#[test]
fn key_timing_api_default_hold_is_zero_and_explicit_timing_is_bounded() {
    let mut s = Session::new();
    let id = frame(&mut s);
    for kind in ["keyDown", "keyUp"] {
        let result = add(&mut s, id, json!({"type":kind,"key":"Gamepad A"}));
        assert!(result.is_ok(), "a missing minimum hold means immediate {kind}: {result:?}");
        let value = serde_json::to_value(&s.doc().unwrap().node(id).unwrap().interactions.last().unwrap().trigger).unwrap();
        assert_eq!(value["ms"], 0);
        assert!(add(&mut s, id, json!({"type":kind,"key":"A","ms":600000})).is_ok());
        for invalid in [json!(-1), json!(0.5), json!(600001), json!("500"), Value::Null] {
            let before = s.doc().unwrap().clone();
            let history = s.undo_depth();
            assert!(add(&mut s, id, json!({"type":kind,"key":"A","ms":invalid})).is_err());
            assert_eq!(s.doc().unwrap(), &before, "invalid timing must leave the document unchanged");
            assert_eq!(s.undo_depth(), history);
        }
    }
}

#[test]
fn key_timing_api_legacy_immediate_keypress_stays_unchanged() {
    let mut s = Session::new();
    let id = frame(&mut s);
    let legacy = json!({"type":"keyPress","key":"Shift+A"});
    add(&mut s, id, legacy.clone()).unwrap();
    assert_eq!(serde_json::to_value(&s.doc().unwrap().node(id).unwrap().interactions[0].trigger).unwrap(), legacy);
}
