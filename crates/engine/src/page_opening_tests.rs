//! `page.setOpening` (#1817): the page a file opens on, for everyone who opens or joins it, written
//! as a document change (so it syncs and undoes) without moving anyone's own view.

use figcraft_doc::NodeId;
use serde_json::{Value, json};

use crate::Session;

fn ex(s: &mut Session, cmd: &str, p: Value) -> Value {
    s.execute(cmd, &p).unwrap_or_else(|e| panic!("{cmd}: {e}"))
}

fn id_of(v: &Value) -> NodeId {
    NodeId(v["id"].as_u64().unwrap())
}

/// Setting the opening page writes the document, leaves this viewer where they are, is one undo
/// step, and a file opened from that document lands on it.
#[test]
fn the_opening_page_is_written_without_moving_the_view_and_a_reopened_file_lands_on_it() {
    let mut s = Session::new();
    let first = s.page().unwrap();
    let cover = id_of(&ex(&mut s, "page.add", json!({"name": "Cover"})));
    ex(&mut s, "page.select", json!({"id": first}));
    let depth = s.undo_depth().0;
    ex(&mut s, "page.setOpening", json!({"id": cover}));
    assert_eq!(s.doc().unwrap().current_page, Some(cover), "written to the document");
    assert_eq!(s.page().unwrap(), first, "the view stays where it was");
    assert_eq!(s.undo_depth().0, depth + 1, "one undo step");
    let reopened = Session::with_document(s.doc().unwrap().clone());
    assert_eq!(reopened.page().unwrap(), cover, "a file opened from this document lands on it");
    ex(&mut s, "edit.undo", json!({}));
    assert_ne!(s.doc().unwrap().current_page, Some(cover), "undone");
    assert_eq!(Session::with_document(s.doc().unwrap().clone()).page().unwrap(), first);
}

/// Without an id the current page is meant; a divider and a layer are refused and change nothing.
#[test]
fn the_current_page_is_the_default_and_dividers_and_layers_are_refused() {
    let mut s = Session::new();
    let two = id_of(&ex(&mut s, "page.add", json!({"name": "Two"})));
    assert_eq!(s.page().unwrap(), two, "page.add goes to the new page");
    ex(&mut s, "page.setOpening", json!({}));
    assert_eq!(s.doc().unwrap().current_page, Some(two), "the current page when none is named");
    let divider = id_of(&ex(&mut s, "page.add", json!({"name": "-"})));
    assert!(s.execute("page.setOpening", &json!({"id": divider})).is_err(), "a divider isn't a page to open on");
    let r = id_of(&ex(&mut s, "shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10})));
    assert!(s.execute("page.setOpening", &json!({"id": r})).is_err(), "a layer isn't a page");
    assert_eq!(s.doc().unwrap().current_page, Some(two), "refusals change nothing");
}

/// Saving from another viewed page preserves the explicit choice in actual native archive bytes.
#[test]
fn explicit_opening_survives_file_encoding_and_undo_while_legacy_saves_use_the_view() {
    let mut s = Session::new();
    let first = s.page().unwrap();
    let cover = id_of(&ex(&mut s, "page.add", json!({"name":"Cover"})));
    ex(&mut s, "page.setOpening", json!({"id":cover}));
    ex(&mut s, "page.select", json!({"id":first}));
    let bytes = crate::cmd::file::file_bytes(s.ds().unwrap()).unwrap();
    let (loaded, _) = figcraft_format::load_bytes(&bytes).unwrap();
    assert_eq!(loaded.current_page, Some(cover));
    assert_eq!(loaded.opening_page, Some(cover));
    assert_eq!(s.page().unwrap(), first);
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc().unwrap().opening_page, None);
    ex(&mut s, "page.select", json!({"id":first}));
    let legacy = crate::cmd::file::file_bytes(s.ds().unwrap()).unwrap();
    let (loaded, _) = figcraft_format::load_bytes(&legacy).unwrap();
    assert_eq!(loaded.current_page, Some(first));
    assert_eq!(loaded.opening_page, None);
}

#[test]
fn deleting_or_converting_the_explicit_opening_page_clears_the_choice_and_undo_restores_it() {
    let mut s = Session::new();
    let cover = id_of(&ex(&mut s, "page.add", json!({"name":"Cover"})));
    ex(&mut s, "page.setOpening", json!({"id":cover}));
    ex(&mut s, "page.rename", json!({"id":cover,"name":"- divider"}));
    assert_eq!(s.doc().unwrap().opening_page, None);
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc().unwrap().opening_page, Some(cover));
    ex(&mut s, "page.delete", json!({"id":cover}));
    assert_eq!(s.doc().unwrap().opening_page, None);
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc().unwrap().opening_page, Some(cover));
}

/// The actual authored choice undoes and redoes independently of the local viewed page,
/// and the redone state survives actual native archive bytes rather than only inspection.
#[test]
fn opening_page_redo_restores_the_explicit_choice_without_moving_the_view() {
    let mut s = Session::new();
    let first = s.page().unwrap();
    let cover = id_of(&ex(&mut s, "page.add", json!({"name":"Redo cover"})));
    ex(&mut s, "page.select", json!({"id":first}));
    let before = s.undo_depth();
    ex(&mut s, "page.setOpening", json!({"id":cover}));
    assert_eq!(s.doc().unwrap().opening_page, Some(cover));
    assert_eq!(s.page().unwrap(), first);
    assert_eq!(s.undo_depth().0, before.0 + 1);
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(s.doc().unwrap().opening_page, None);
    assert_eq!(s.page().unwrap(), first);
    ex(&mut s, "edit.redo", json!({}));
    assert_eq!(s.doc().unwrap().opening_page, Some(cover));
    assert_eq!(s.page().unwrap(), first, "redo changes the shared opening choice, never this viewer's page");
    assert_eq!(s.undo_depth().0, before.0 + 1);
    let bytes = crate::cmd::file::file_bytes(s.ds().unwrap()).unwrap();
    let (loaded, _) = figcraft_format::load_bytes(&bytes).unwrap();
    assert_eq!((loaded.opening_page, loaded.current_page), (Some(cover), Some(cover)));
    assert_eq!(Session::with_document(loaded).page().unwrap(), cover);
}

/// A supplied integer may be a valid NodeId yet name no node. Refusal must precede
/// changes to authored data, viewed page, selection, revision and undo/redo history.
#[test]
fn nonexistent_opening_page_is_refused_before_any_document_view_or_history_change() {
    let mut s = Session::new();
    let first = s.page().unwrap();
    let cover = id_of(&ex(&mut s, "page.add", json!({"name":"Existing cover"})));
    ex(&mut s, "page.setOpening", json!({"id":cover}));
    ex(&mut s, "page.select", json!({"id":first}));
    ex(&mut s, "shape.rectangle", json!({"width":40,"height":30}));
    let missing = u64::MAX;
    assert!(s.doc().unwrap().node(NodeId(missing)).is_none());
    let before = s.ds().unwrap().doc.clone();
    let revision = s.ds().unwrap().revision;
    let view = s.page().unwrap();
    let selection = s.selection();
    let history = s.undo_depth();
    let bytes = crate::cmd::file::file_bytes(s.ds().unwrap()).unwrap();
    assert!(s.execute("page.setOpening", &json!({"id":missing})).is_err());
    assert!(std::sync::Arc::ptr_eq(&before, &s.ds().unwrap().doc));
    assert_eq!(s.ds().unwrap().revision, revision);
    assert_eq!((s.page().unwrap(), s.selection(), s.undo_depth()), (view, selection, history));
    assert_eq!(s.doc().unwrap().opening_page, Some(cover));
    assert_eq!(crate::cmd::file::file_bytes(s.ds().unwrap()).unwrap(), bytes);
}
