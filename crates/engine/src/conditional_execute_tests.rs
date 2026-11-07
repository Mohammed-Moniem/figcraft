//! A live revision condition is checked before any command work, including grouped commands.
use crate::{Session, inspect};
use serde_json::json;

#[test]
fn doc_list_queries_preserve_the_inspected_token_until_a_guarded_edit() {
    let mut session = Session::new();
    let before = inspect::document(&session, 30, true);
    let uid = session.ds().unwrap().uid;
    let revision = session.ds().unwrap().revision;
    let events = session.events.latest();
    for _ in 0..4 {
        let listed = session.execute("doc.list", &json!({})).unwrap();
        assert_eq!(listed[0]["id"], uid);
        assert_eq!(inspect::document(&session, 30, true), before);
        assert_eq!(session.events.latest(), events);
    }
    session.execute_as_if_revision(None, "shape.rectangle", &json!({"width":40,"height":20}), uid, revision).unwrap();
    assert_eq!(session.undo_depth().0, 1);
}

#[test]
fn matching_live_token_applies_once_and_stale_token_preserves_all_state() {
    let mut session = Session::new();
    let (uid, revision) = {
        let current = session.ds().unwrap();
        (current.uid, current.revision)
    };
    session.execute_as_if_revision(None, "shape.rectangle", &json!({"width":40,"height":20}), uid, revision).unwrap();
    let before = inspect::document(&session, 30, true);
    let document = session.ds().unwrap().doc.clone();
    let before_revision = session.ds().unwrap().revision;
    let history = session.undo_depth();
    let events = session.events.latest();
    let error =
        session.execute_as_if_revision(Some("guarded agent"), "shape.rectangle", &json!({"width":50,"height":25}), uid, revision).unwrap_err();
    assert!(error.to_string().starts_with("CONTROL_REVISION_CONFLICT:"));
    assert_eq!(inspect::document(&session, 30, true), before);
    assert_eq!(session.ds().unwrap().doc, document);
    assert_eq!(session.ds().unwrap().revision, before_revision);
    assert_eq!(session.undo_depth(), history);
    assert_eq!(session.events.latest(), events);
}

#[test]
fn another_open_document_identity_never_runs_a_batch() {
    let mut session = Session::new();
    let uid = session.ds().unwrap().uid;
    session.execute("file.new", &json!({"name":"Fresh file"})).unwrap();
    let before = inspect::document(&session, 30, true);
    let revision = session.ds().unwrap().revision;
    let history = session.undo_depth();
    let error = session
        .execute_as_if_revision(
            None,
            "command.batch",
            &json!({"commands":[{"command":"shape.rectangle","params":{"width":40,"height":20}}]}),
            uid,
            revision,
        )
        .unwrap_err();
    assert!(error.to_string().starts_with("CONTROL_DOCUMENT_CONFLICT:"));
    assert_eq!(inspect::document(&session, 30, true), before);
    assert_eq!(session.undo_depth(), history);
    assert_eq!(session.ds().unwrap().revision, revision);
}

#[test]
fn reopening_the_same_native_bytes_never_reuses_an_old_live_token() {
    let mut session = Session::new();
    let bytes = session.execute("file.saveAs", &json!({"data":true})).unwrap()["data"].clone();
    let old_uid = session.ds().unwrap().uid;
    session.execute("file.close", &json!({"force":true})).unwrap();
    session.execute("file.open", &json!({"name":"reopened.figcraft","data":bytes})).unwrap();
    let revision = session.ds().unwrap().revision;
    assert_ne!(session.ds().unwrap().uid, old_uid);
    let before = inspect::document(&session, 30, true);
    let error = session.execute_as_if_revision(None, "shape.rectangle", &json!({"width":5,"height":5}), old_uid, revision).unwrap_err();
    assert!(error.to_string().starts_with("CONTROL_DOCUMENT_CONFLICT:"));
    assert_eq!(inspect::document(&session, 30, true), before);
}

#[test]
fn matching_condition_keeps_an_ordinary_grouped_edit_one_undo_step() {
    let mut session = Session::new();
    let current = inspect::document(&session, 0, false);
    session.execute_as_if_revision(None, "command.batch", &json!({"commands":[{"command":"shape.rectangle","params":{"width":40,"height":20}},{"command":"shape.ellipse","params":{"width":10,"height":10}}]}), current["id"].as_u64().unwrap(), current["revision"].as_u64().unwrap()).unwrap();
    assert_eq!(session.undo_depth().0, 1);
}
