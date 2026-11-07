//! Document-aware durable host Save acknowledgment: polling, tab switches, edits and branches.
use crate::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, command: &str, params: Value) -> Value {
    s.execute(command, &params).unwrap()
}

fn edit(s: &mut Session) {
    run(s, "shape.rectangle", json!({"x": 10, "y": 10, "width": 20, "height": 30}));
}

fn exported(s: &mut Session) -> Value {
    let r = run(s, "file.saveAs", json!({"data": true}));
    json!({"document": r["document"], "revision": r["revision"]})
}

#[test]
fn host_ack_only_marks_the_captured_document_after_ack_despite_queries_and_tab_switch() {
    let mut s = Session::new();
    edit(&mut s);
    let first = s.docs[0].uid;
    let saved = exported(&mut s);
    assert!(s.docs[0].is_dirty(), "export/post alone is not persistence");
    run(&mut s, "doc.list", json!({}));
    run(&mut s, "file.new", json!({"name": "Other"}));
    edit(&mut s);
    assert_eq!(run(&mut s, "file.markSaved", saved.clone())["saved"], true);
    assert!(!s.docs.iter().find(|d| d.uid == first).unwrap().is_dirty());
    assert!(s.ds().unwrap().is_dirty(), "the active second tab is not marked saved");
    assert_eq!(run(&mut s, "file.markSaved", saved)["saved"], false, "duplicate consumed");
}

#[test]
fn host_ack_later_edits_remain_dirty_and_stale_ack_is_consumed() {
    let mut s = Session::new();
    edit(&mut s);
    let saved = exported(&mut s);
    run(&mut s, "file.rename", json!({"name": "Newer edit"}));
    assert_eq!(run(&mut s, "file.markSaved", saved.clone())["saved"], false);
    assert!(s.ds().unwrap().is_dirty());
    assert_eq!(run(&mut s, "file.markSaved", saved)["saved"], false);
}

#[test]
fn host_ack_unknown_closed_and_superseded_snapshots_are_ignored() {
    let mut s = Session::new();
    edit(&mut s);
    let saved = exported(&mut s);
    let mut unknown = saved.clone();
    unknown["revision"] = json!(u64::MAX);
    assert_eq!(run(&mut s, "file.markSaved", unknown)["saved"], false);
    run(&mut s, "file.markSaved", json!({}));
    assert_eq!(run(&mut s, "file.markSaved", saved)["saved"], false, "later legacy save supersedes pending save");
    edit(&mut s);
    let closed = exported(&mut s);
    run(&mut s, "file.close", json!({"force": true}));
    assert_eq!(run(&mut s, "file.markSaved", closed)["saved"], false);
}

#[test]
fn host_ack_branch_file_identity_and_branch_mutation_are_preserved() {
    let mut s = Session::new();
    let main = s.docs[0].uid;
    run(&mut s, "branch.create", json!({"name": "Explore"}));
    edit(&mut s);
    let saved = exported(&mut s);
    let again = exported(&mut s);
    assert_eq!(saved["document"], main);
    assert_ne!(saved["revision"], again["revision"], "branch exports have distinct keys");
    let docs = run(&mut s, "doc.list", json!({}));
    assert!(docs.as_array().unwrap().iter().all(|d| d["fileDocument"] == main));
    edit(&mut s);
    assert_eq!(run(&mut s, "file.markSaved", saved)["saved"], false);
    assert!(s.ds().unwrap().is_dirty());
    let latest = exported(&mut s);
    assert_eq!(run(&mut s, "file.markSaved", latest)["saved"], true);
    assert!(s.docs.iter().all(|d| !d.is_dirty()), "main and matching branch snapshots are saved together");
}

#[test]
fn host_ack_history_change_is_not_mistaken_for_an_unchanged_document() {
    let mut s = Session::new();
    edit(&mut s);
    let saved = exported(&mut s);
    run(&mut s, "file.saveVersion", json!({"name": "Later history"}));
    assert_eq!(run(&mut s, "file.markSaved", saved)["saved"], false);
    assert!(s.ds().unwrap().is_dirty());
}

#[test]
fn host_ack_pending_snapshots_are_bounded_and_invalid_pairs_fail() {
    let mut s = Session::new();
    edit(&mut s);
    let old = exported(&mut s);
    for _ in 0..33 {
        exported(&mut s);
    }
    assert_eq!(s.pending_file_saves.len(), 32);
    assert_eq!(run(&mut s, "file.markSaved", old)["saved"], false);
    assert!(s.execute("file.markSaved", &json!({"document": 1})).is_err());
    assert!(s.execute("file.markSaved", &json!({"document": -1, "revision": 2})).is_err());
    assert!(s.ds().unwrap().is_dirty());
}

#[test]
fn host_ack_newer_toolbar_save_retires_older_native_tickets() {
    let mut s = Session::new();
    edit(&mut s);
    let older_native = exported(&mut s);
    let newer_toolbar = exported(&mut s);
    assert_eq!(s.pending_file_saves().len(), 2);
    assert_eq!(run(&mut s, "file.markSaved", newer_toolbar)["saved"], true);
    assert!(s.pending_file_saves().is_empty());
    run(&mut s, "file.rename", json!({"name": "Later edit"}));
    assert_eq!(run(&mut s, "file.markSaved", older_native)["saved"], false);
    assert!(s.ds().unwrap().is_dirty());
}
