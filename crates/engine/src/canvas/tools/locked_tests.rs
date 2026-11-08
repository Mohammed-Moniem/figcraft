use crate::{Session, doc::NodeId};
use figcraft_geom::Point;
use serde_json::json;

fn scene() -> (Session, NodeId, NodeId, NodeId) {
    let mut s = Session::new();
    let page = s.page().unwrap();
    let a = s.execute("frame.create", &json!({"x":100,"y":100,"width":200,"height":160})).unwrap();
    let parent = NodeId(a["id"].as_u64().unwrap());
    let child = s.execute("shape.rectangle", &json!({"parent":parent,"x":20,"y":20,"width":100,"height":80})).unwrap();
    let child = NodeId(child["id"].as_u64().unwrap());
    s.execute("object.toggleLock", &json!({"ids":[parent],"locked":true})).unwrap();
    (s, page, parent, child)
}

#[test]
fn canvas_unlock_engine_query_and_explicit_command_preserve_child_and_selection_with_undo() {
    let (mut s, page, parent, child) = scene();
    let other = s.execute("shape.rectangle", &json!({"x":500,"y":100,"width":100,"height":80})).unwrap();
    let other = NodeId(other["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids":[other]})).unwrap();
    assert_eq!(s.locked_at(s.doc().unwrap(), page, Point::new(150.0, 150.0), 0.0), Some(parent));
    let before = s.undo_depth().0;
    s.execute("object.toggleLock", &json!({"ids":[parent],"locked":false})).unwrap();
    assert!(!s.doc().unwrap().node(parent).unwrap().locked);
    assert!(!s.doc().unwrap().node(child).unwrap().locked);
    assert_eq!(s.selection(), [other]);
    assert_eq!(s.undo_depth().0, before + 1);
    s.execute("edit.undo", &json!({})).unwrap();
    assert!(s.doc().unwrap().node(parent).unwrap().locked);
    assert_eq!(s.selection(), [other]);
}

#[test]
fn canvas_unlock_engine_query_uses_the_current_outline_mode() {
    // Frames deliberately hit their interiors in outline mode; use a shape to exercise the
    // actual outline-only behavior without changing that existing frame selection contract.
    let mut s = Session::new();
    let page = s.page().unwrap();
    let made = s.execute("shape.rectangle", &json!({"x":100,"y":100,"width":200,"height":160})).unwrap();
    let shape = NodeId(made["id"].as_u64().unwrap());
    s.execute("object.toggleLock", &json!({"ids":[shape],"locked":true})).unwrap();
    assert_eq!(s.locked_at(s.doc().unwrap(), page, Point::new(150.0, 150.0), 0.0), Some(shape));
    s.ds_mut().unwrap().view.outlines = true;
    assert_eq!(s.locked_at(s.doc().unwrap(), page, Point::new(150.0, 150.0), 0.0), None);
    assert_eq!(s.locked_at(s.doc().unwrap(), page, Point::new(100.0, 150.0), 1.0), Some(shape));
}
