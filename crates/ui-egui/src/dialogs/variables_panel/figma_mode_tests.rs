use super::figma_mode_params;
use figcraft_engine::Session;
use serde_json::json;

fn prepared() -> (Session, u64, String, String) {
    let mut session = Session::new();
    session.execute("variable.createCollection", &json!({"name":"Theme","modes":["Day","Night"]})).unwrap();
    let owner = session.ds().unwrap().uid;
    let coll = &session.doc().unwrap().variables.collections[0];
    let (id, mode) = (coll.id.clone(), coll.modes[1].id.clone());
    (session, owner, id, mode)
}

#[test]
fn figma_picker_retains_selected_mode_and_preserves_other_values() {
    let (mut s, owner, coll, mode) = prepared();
    s.execute("variable.create", &json!({"collection":coll,"name":"space","type":"number","values":{"Day":4,"Night":8}})).unwrap();
    let mut params = figma_mode_params(&s, owner, &coll, &mode, false).unwrap();
    params["variables"] = json!({"space":{"$type":"number","$value":24}});
    s.execute("variable.import", &params).unwrap();
    let output = s.execute("variable.export", &figma_mode_params(&s, owner, &coll, &mode, true).unwrap()).unwrap();
    let tokens: serde_json::Value = serde_json::from_str(output["text"].as_str().unwrap()).unwrap();
    assert_eq!(tokens["space"]["$value"], 24.0);
    let v = s.doc().unwrap().variables.get("space").unwrap();
    let day = &s.doc().unwrap().variables.collection_by_id(&coll).unwrap().modes[0].id;
    assert_eq!(v.values[day], figcraft_doc::VariableValue::Number(4.0));
}

#[test]
fn figma_picker_rejects_reused_ids_in_another_live_document() {
    let (s, owner, coll, mode) = prepared();
    assert!(figma_mode_params(&s, owner, &coll, &mode, false).is_some());
    let mut copied = Session::new();
    copied.doc_mut().unwrap().variables = s.doc().unwrap().variables.clone();
    assert_ne!(copied.ds().unwrap().uid, owner);
    assert!(figma_mode_params(&copied, owner, &coll, &mode, false).is_none());
    assert!(figma_mode_params(&copied, owner, &coll, &mode, true).is_none());
}

#[test]
fn figma_picker_refuses_deleted_or_extended_targets() {
    let (mut s, owner, coll, mode) = prepared();
    s.doc_mut().unwrap().variables.collections[0].modes.retain(|m| m.id != mode);
    assert!(figma_mode_params(&s, owner, &coll, &mode, false).is_none());
    assert!(figma_mode_params(&s, owner, &coll, &mode, true).is_none());
    let mode = s.doc().unwrap().variables.collections[0].modes[0].id.clone();
    s.doc_mut().unwrap().variables.collections[0].extends = Some("read-only-source".into());
    assert!(figma_mode_params(&s, owner, &coll, &mode, false).is_none());
    assert!(figma_mode_params(&s, owner, &coll, &mode, true).is_none());
}
