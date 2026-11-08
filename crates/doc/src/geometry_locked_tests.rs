use super::*;
use crate::{AutoLayout, BoolOp};

fn rect(doc: &mut Document, parent: NodeId, name: &str, x: f64, y: f64, size: f64) -> NodeId {
    doc.create(parent, NodeKind::Rectangle, name, x, y, size, size).unwrap()
}

fn owner(doc: &Document, page: NodeId, x: f64, y: f64) -> Option<NodeId> {
    locked_owner_at(doc, page, Point::new(x, y), 0.0, None, None)
}

#[test]
fn canvas_unlock_query_keeps_topmost_visibility_and_normal_selection_rules() {
    let mut doc = Document::new("Locks");
    let page = doc.first_page().unwrap();
    let back = rect(&mut doc, page, "Locked back", 0.0, 0.0, 100.0);
    doc.node_mut(back).unwrap().locked = true;
    assert_eq!(owner(&doc, page, 20.0, 20.0), Some(back));
    assert_eq!(hit_leaf(&doc, page, Point::new(20.0, 20.0), 0.0, None), None, "ordinary selection still skips it");
    let front = rect(&mut doc, page, "Unlocked front", 0.0, 0.0, 100.0);
    assert_eq!(owner(&doc, page, 20.0, 20.0), None, "an unlocked foreground occludes the locked layer");
    doc.node_mut(front).unwrap().visible = false;
    assert_eq!(owner(&doc, page, 20.0, 20.0), Some(back));
    assert!(doc.node(back).unwrap().locked, "query never edits");
}

#[test]
fn canvas_unlock_query_identifies_outermost_explicit_ancestor_lock() {
    let mut doc = Document::new("Inherited lock");
    let page = doc.first_page().unwrap();
    let frame = doc.create(page, NodeKind::Frame, "Parent", 0.0, 0.0, 200.0, 200.0).unwrap();
    let child = rect(&mut doc, frame, "Child", 20.0, 20.0, 100.0);
    doc.node_mut(frame).unwrap().locked = true;
    assert_eq!(owner(&doc, page, 40.0, 40.0), Some(frame));
    assert!(!doc.node(child).unwrap().locked);
    doc.node_mut(child).unwrap().locked = true;
    assert_eq!(owner(&doc, page, 40.0, 40.0), Some(frame));
    doc.node_mut(frame).unwrap().locked = false;
    assert_eq!(owner(&doc, page, 40.0, 40.0), Some(child));
}

#[test]
fn canvas_unlock_query_respects_clips_hidden_parents_and_reversed_paint_order() {
    let mut doc = Document::new("Clip and order");
    let page = doc.first_page().unwrap();
    let frame = doc.create(page, NodeKind::Frame, "Clip", 0.0, 0.0, 50.0, 50.0).unwrap();
    let child = rect(&mut doc, frame, "Outside", 70.0, 0.0, 20.0);
    doc.node_mut(frame).unwrap().clip = true;
    doc.node_mut(child).unwrap().locked = true;
    assert_eq!(owner(&doc, page, 75.0, 5.0), None);
    doc.node_mut(frame).unwrap().clip = false;
    assert_eq!(owner(&doc, page, 75.0, 5.0), Some(child));
    doc.node_mut(frame).unwrap().visible = false;
    assert_eq!(owner(&doc, page, 75.0, 5.0), None);
    doc.node_mut(frame).unwrap().visible = true;
    let front = rect(&mut doc, frame, "Front", 70.0, 0.0, 20.0);
    assert_eq!(owner(&doc, page, 75.0, 5.0), None);
    doc.node_mut(frame).unwrap().layout = Some(AutoLayout { reverse_z: true, ..Default::default() });
    assert_eq!(owner(&doc, page, 75.0, 5.0), Some(child));
    assert!(!doc.node(front).unwrap().locked);
}

#[test]
fn canvas_unlock_query_respects_mask_runs_and_boolean_result_holes() {
    let mut doc = Document::new("Masked lock");
    let page = doc.first_page().unwrap();
    let mask = rect(&mut doc, page, "Mask", 0.0, 0.0, 50.0);
    doc.node_mut(mask).unwrap().is_mask = true;
    let covered = rect(&mut doc, page, "Covered", 0.0, 0.0, 100.0);
    doc.node_mut(covered).unwrap().locked = true;
    assert_eq!(owner(&doc, page, 25.0, 25.0), Some(covered));
    assert_eq!(owner(&doc, page, 75.0, 25.0), None);
    let boolean = doc.create(page, NodeKind::BooleanOp { op: BoolOp::Subtract }, "Ring", 200.0, 0.0, 100.0, 100.0).unwrap();
    doc.node_mut(boolean).unwrap().locked = true;
    doc.node_mut(mask).unwrap().is_mask = false;
    let path = BezPath::from_svg("M0 0 H100 V100 H0 Z M25 25 V75 H75 V25 Z").unwrap();
    let result = |id, _: &Node| (id == boolean).then(|| path.clone());
    assert_eq!(locked_owner_at(&doc, page, Point::new(210.0, 10.0), 0.0, Some(&result), None), Some(boolean));
    assert_eq!(locked_owner_at(&doc, page, Point::new(250.0, 50.0), 0.0, Some(&result), None), None, "hole is not visible geometry");
}

#[test]
fn canvas_unlock_query_rejects_nonfinite_input() {
    let mut doc = Document::new("Finite");
    let page = doc.first_page().unwrap();
    let id = rect(&mut doc, page, "Locked", 0.0, 0.0, 100.0);
    doc.node_mut(id).unwrap().locked = true;
    for (x, y, tol) in [(f64::NAN, 0.0, 0.0), (0.0, f64::INFINITY, 0.0), (0.0, 0.0, f64::NAN), (0.0, 0.0, -1.0)] {
        assert_eq!(locked_owner_at(&doc, page, Point::new(x, y), tol, None, None), None);
    }
}
