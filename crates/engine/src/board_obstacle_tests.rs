//! Automatic elbow clearance, with manual-path and other-routing controls.
use figcraft_doc::NodeId;
use figcraft_geom::{Point, Rect};
use serde_json::json;

use crate::Session;
use crate::board_tests::{board, connector, drawn, id, node};

fn shape(s: &mut Session, x: f64, y: f64, w: f64, h: f64, text: &str) -> NodeId {
    id(&s.execute("board.shape", &json!({"x":x,"y":y,"width":w,"height":h,"text":text})).unwrap(), "id")
}

fn crosses(a: Point, b: Point, r: Rect) -> bool {
    const E: f64 = 1e-6;
    if (a.x - b.x).abs() < E {
        a.x > r.x0 + E && a.x < r.x1 - E && a.y.min(b.y) < r.y1 - E && a.y.max(b.y) > r.y0 + E
    } else if (a.y - b.y).abs() < E {
        a.y > r.y0 + E && a.y < r.y1 - E && a.x.min(b.x) < r.x1 - E && a.x.max(b.x) > r.x0 + E
    } else {
        panic!("an elbow run must be axis aligned: {a:?} -> {b:?}")
    }
}

fn clear(s: &Session, c: NodeId, obstacles: &[NodeId]) {
    let pts = drawn(s, c);
    assert!(pts.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    for &other in obstacles {
        let bounds = s.doc().unwrap().world_bounds(other).unwrap();
        assert!(
            !pts.windows(2).any(|w| crosses(w[0], w[1], bounds)),
            "automatic connector {c:?} crosses unrelated shape {other:?}: {pts:?}, bounds={bounds:?}"
        );
    }
}

#[test]
fn automatic_elbows_avoid_the_unrelated_shapes_in_the_four_shape_report() {
    let mut s = board();
    let left = shape(&mut s, 0.0, 0.0, 320.0, 120.0, "Original source");
    let top = shape(&mut s, 520.0, 180.0, 280.0, 120.0, "Original first target");
    let middle = shape(&mut s, 520.0, 420.0, 280.0, 120.0, "Original second target");
    let bottom = shape(&mut s, 520.0, 660.0, 280.0, 120.0, "Original third target");
    for (to, others) in [(top, vec![middle, bottom]), (middle, vec![top, bottom]), (bottom, vec![top, middle])] {
        let c = id(&s.execute("board.connector", &json!({"from":left,"to":to,"style":"elbow"})).unwrap(), "id");
        assert!(connector(&s, c).path.is_none(), "this is automatic routing");
        clear(&s, c, &others);
    }
}

#[test]
fn moving_an_unrelated_shape_onto_an_elbow_reroutes_in_the_same_undo_step() {
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "From");
    let b = shape(&mut s, 400.0, 0.0, 100.0, 100.0, "To");
    let obstacle = shape(&mut s, 200.0, 200.0, 80.0, 80.0, "Unrelated");
    let c = id(&s.execute("board.connector", &json!({"from":a,"to":b,"style":"elbow"})).unwrap(), "id");
    let before = s.doc().unwrap().clone();
    let old = drawn(&s, c);
    let ends = (old[0], *old.last().unwrap());
    let undo = s.undo_depth().0;
    s.execute("transform.move", &json!({"ids":[obstacle],"dx":0,"dy":-190})).unwrap();
    clear(&s, c, &[obstacle]);
    let moved = drawn(&s, c);
    assert_ne!(moved, old);
    assert_eq!((moved[0], *moved.last().unwrap()), ends, "automatic endpoints retain their exact four-unit setback");
    assert_eq!(s.undo_depth().0, undo + 1, "routing is derived inside the obstacle move");
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before, "one Undo restores both geometry and the obstacle");
}

#[test]
fn manual_run_paths_and_manual_endpoint_coordinates_remain_exact() {
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "From");
    let b = shape(&mut s, 400.0, 200.0, 100.0, 100.0, "To");
    let c = id(&s.execute("board.connector", &json!({"from":a,"to":b,"style":"elbow","fromAt":[1,0.5],"toAt":[0,0.5]})).unwrap(), "id");
    s.execute("board.reshape", &json!({"id":c,"portion":1,"x":250,"y":500})).unwrap();
    assert!(connector(&s, c).path.is_some());
    let expected = node(&s, c);
    let pts = drawn(&s, c);
    assert_eq!(pts.first(), Some(&Point::new(100.0, 50.0)));
    assert_eq!(pts.last(), Some(&Point::new(400.0, 250.0)));
    let obstacle = shape(&mut s, 180.0, 20.0, 100.0, 400.0, "Unrelated");
    s.execute("transform.move", &json!({"ids":[obstacle],"dx":20,"dy":30})).unwrap();
    assert_eq!(node(&s, c), expected, "an unrelated obstacle cannot replace a manually reshaped path");
}

#[test]
fn straight_and_curved_connectors_keep_their_existing_geometry() {
    for style in ["straight", "curved"] {
        let mut s = board();
        let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "From");
        let b = shape(&mut s, 400.0, 0.0, 100.0, 100.0, "To");
        let c = id(&s.execute("board.connector", &json!({"from":a,"to":b,"style":style})).unwrap(), "id");
        let expected = node(&s, c);
        shape(&mut s, 200.0, 10.0, 80.0, 80.0, "Unrelated");
        assert_eq!(node(&s, c), expected, "{style}: this feature is only automatic elbows");
    }
}

#[test]
fn exhausted_shared_search_budget_refreshes_late_connectors_instead_of_retaining_old_detours() {
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "From");
    let b = shape(&mut s, 400.0, 0.0, 100.0, 100.0, "To");
    let obstacle = shape(&mut s, 200.0, 10.0, 80.0, 80.0, "Obstacle");
    let first = id(&s.execute("board.connector", &json!({"from":a,"fromSide":"right","to":b,"toSide":"left","style":"elbow"})).unwrap(), "id");
    clear(&s, first, &[obstacle]);
    assert!(drawn(&s, first).len() > 2);
    let mut saved = s.doc().unwrap().clone();
    let original = saved.node(first).unwrap().clone();
    let mut connectors = vec![first];
    // An authoritative model may already contain more routes than one new settlement searches.
    // Seed all 65 with a validated old detour, so the subsequent real edit must retire each one.
    for _ in 0..64 {
        let id = saved.alloc_id();
        let mut duplicate = original.clone();
        duplicate.id = id;
        saved.insert(duplicate, original.parent.unwrap(), None).unwrap();
        connectors.push(id);
    }
    let mut s = Session::with_document(saved);
    for &c in &connectors {
        clear(&s, c, &[obstacle]);
        assert_eq!(drawn(&s, c), drawn(&s, first));
    }
    let first = drawn(&s, connectors[0]);
    let turn_y = first.iter().find(|p| (p.y - 50.0).abs() > 1.0).unwrap().y;
    // Move the obstacle onto the old detour while clearing the original horizontal route.
    let y = turn_y - 20.0;
    let before = s.doc().unwrap().clone();
    let undo = s.undo_depth().0;
    s.execute("node.set", &json!({"id":obstacle,"props":{"y":y,"height":40}})).unwrap();
    let current_bounds = s.doc().unwrap().world_bounds(obstacle).unwrap();
    assert!(first.windows(2).any(|w| crosses(w[0], w[1], current_bounds)), "the old detour crosses the new obstacle interior");
    for c in connectors {
        assert_eq!(drawn(&s, c), vec![Point::new(104.0, 50.0), Point::new(396.0, 50.0)], "including the 65th connector beyond the shared search cap");
        clear(&s, c, &[obstacle]);
    }
    assert_eq!(s.undo_depth().0, undo + 1);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn hidden_deleted_and_other_page_obstacles_do_not_leave_stale_routes() {
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "From");
    let b = shape(&mut s, 400.0, 0.0, 100.0, 100.0, "To");
    let obstacle = shape(&mut s, 200.0, 10.0, 80.0, 80.0, "Obstacle");
    let c = id(&s.execute("board.connector", &json!({"from":a,"to":b,"style":"elbow","label":"Derived label"})).unwrap(), "id");
    clear(&s, c, &[obstacle]);
    let detour = drawn(&s, c);
    let label = crate::board::label_of(s.doc().unwrap(), c).unwrap();
    let detour_label = node(&s, label);
    let before = s.doc().unwrap().clone();
    s.execute("node.set", &json!({"id":obstacle,"props":{"visible":false}})).unwrap();
    assert_eq!(drawn(&s, c), vec![Point::new(104.0, 50.0), Point::new(396.0, 50.0)]);
    assert_ne!(node(&s, label).transform, detour_label.transform);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    s.execute("edit.delete", &json!({"ids":[obstacle]})).unwrap();
    assert_eq!(drawn(&s, c), vec![Point::new(104.0, 50.0), Point::new(396.0, 50.0)]);
    s.undo().unwrap();
    assert_eq!(drawn(&s, c), detour);
    let page = id(&s.execute("page.add", &json!({"name":"Other board page"})).unwrap(), "id");
    s.execute("object.moveToPage", &json!({"ids":[obstacle],"page":page})).unwrap();
    assert_eq!(drawn(&s, c), vec![Point::new(104.0, 50.0), Point::new(396.0, 50.0)]);
}

#[test]
fn clipped_rotated_and_hidden_ancestor_bounds_are_current() {
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "From");
    let b = shape(&mut s, 400.0, 0.0, 100.0, 100.0, "To");
    let frame = id(&s.execute("frame.create", &json!({"x":190,"y":0,"width":100,"height":100,"props":{"clip":true}})).unwrap(), "id");
    let obstacle = id(&s.execute("board.shape", &json!({"parent":frame,"x":10,"y":10,"width":80,"height":80,"text":"Clipped"})).unwrap(), "id");
    let c = id(&s.execute("board.connector", &json!({"from":a,"to":b,"style":"elbow"})).unwrap(), "id");
    clear(&s, c, &[obstacle]);
    s.execute("node.set", &json!({"id":frame,"props":{"visible":false}})).unwrap();
    assert_eq!(drawn(&s, c), vec![Point::new(104.0, 50.0), Point::new(396.0, 50.0)]);
    s.undo().unwrap();
    clear(&s, c, &[obstacle]);
    s.execute("node.set", &json!({"id":obstacle,"props":{"y":150}})).unwrap();
    assert_eq!(drawn(&s, c), vec![Point::new(104.0, 50.0), Point::new(396.0, 50.0)], "fully clipped obstacle has no visible bounds");
    s.undo().unwrap();
    s.execute("node.set", &json!({"id":frame,"props":{"rotation":20,"clip":false}})).unwrap();
    clear(&s, c, &[obstacle]);
}

#[test]
fn native_open_refreshes_automatic_obstacles_but_generic_and_live_models_keep_saved_authority() {
    use base64::Engine as _;
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "From");
    let b = shape(&mut s, 400.0, 0.0, 100.0, 100.0, "To");
    let obstacle = shape(&mut s, 200.0, 10.0, 80.0, 80.0, "Obstacle");
    let c = id(&s.execute("board.connector", &json!({"from":a,"to":b,"style":"elbow"})).unwrap(), "id");
    let mut old = s.doc().unwrap().clone();
    let n = old.node_mut(c).unwrap();
    n.transform = figcraft_geom::Affine::translate((104.0, 50.0));
    n.width = 292.0;
    n.height = 0.0;
    n.kind = figcraft_doc::NodeKind::Vector { network: figcraft_doc::VectorNetwork::from_points(&[Point::ZERO, Point::new(292.0, 0.0)], false) };
    let saved = node_points(&old, c);
    let generic = Session::with_document(old.clone());
    assert_eq!(drawn(&generic, c), saved, "supplying a model does not migrate its authority");
    let bytes = figcraft_format::save_bytes(&old, None).unwrap();
    let mut opened = Session::empty();
    opened
        .execute("file.open", &json!({"data":base64::engine::general_purpose::STANDARD.encode(&bytes),"name":"Original synthetic.figcraft"}))
        .unwrap();
    clear(&opened, c, &[obstacle]);
    assert!(!opened.ds().unwrap().is_dirty());
    assert_eq!(opened.undo_depth(), (0, 0));
    let reopened_bytes = figcraft_format::save_bytes(opened.doc().unwrap(), None).unwrap();
    let mut again = Session::empty();
    again
        .execute("file.open", &json!({"data":base64::engine::general_purpose::STANDARD.encode(&reopened_bytes),"name":"Saved routing.figcraft"}))
        .unwrap();
    assert_eq!(drawn(&again, c), drawn(&opened, c));
    assert!(!again.ds().unwrap().is_dirty());
    assert_eq!(again.undo_depth(), (0, 0));
    let mut room = crate::collab_live_tests::Room::new(old, 2, 170_700);
    for i in 0..2 {
        assert_eq!(drawn(room.s(i), c), saved);
        assert_eq!(node_points(room.s(i).live().client().unwrap().document(), c), saved);
    }
    room.check();
    room.s(0).execute("transform.move", &json!({"ids":[obstacle],"dx":0,"dy":5})).unwrap();
    room.settle();
    room.check();
    for i in 0..2 {
        clear(room.s(i), c, &[obstacle]);
    }
}

fn node_points(doc: &figcraft_doc::Document, id: NodeId) -> Vec<Point> {
    let figcraft_doc::NodeKind::Vector { network } = &doc.node(id).unwrap().kind else {
        panic!("expected connector vector");
    };
    let w = doc.world_transform(id);
    network.vertices.iter().map(|v| w * v.point()).collect()
}

#[test]
fn pointer_preview_commit_and_escape_use_the_same_obstacle_route() {
    use crate::{PointerEvent, PointerKind};
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "From");
    let b = shape(&mut s, 400.0, 0.0, 100.0, 100.0, "To");
    let obstacle = shape(&mut s, 200.0, 10.0, 80.0, 80.0, "Obstacle");
    s.execute("tool.connector", &json!({"style":"elbow"})).unwrap();
    let before = s.doc().unwrap().clone();
    let undo = s.undo_depth().0;
    s.pointer(&PointerEvent::at(PointerKind::Down, 97.0, 50.0, 1.0)).unwrap();
    s.pointer(&PointerEvent::at(PointerKind::Move, 402.0, 50.0, 1.0)).unwrap();
    let c = s.doc().unwrap().nodes().find(|n| matches!(n.board, Some(figcraft_doc::BoardItem::Connector(_)))).unwrap().id;
    clear(&s, c, &[obstacle]);
    let preview = drawn(&s, c);
    assert_eq!(s.undo_depth().0, undo);
    s.escape().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth().0, undo);
    s.pointer(&PointerEvent::at(PointerKind::Down, 97.0, 50.0, 1.0)).unwrap();
    s.pointer(&PointerEvent::at(PointerKind::Move, 402.0, 50.0, 1.0)).unwrap();
    s.pointer(&PointerEvent::at(PointerKind::Up, 402.0, 50.0, 1.0)).unwrap();
    let c = s.doc().unwrap().nodes().find(|n| matches!(n.board, Some(figcraft_doc::BoardItem::Connector(_)))).unwrap().id;
    assert_eq!(connector(&s, c).from.node(), Some(a));
    assert_eq!(connector(&s, c).to.node(), Some(b));
    assert_eq!(drawn(&s, c), preview);
    assert_eq!(s.undo_depth().0, undo + 1);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
}

#[test]
fn overflowed_obstacle_snapshot_refreshes_from_legacy_instead_of_using_a_partial_set() {
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "From");
    let b = shape(&mut s, 400.0, 0.0, 100.0, 100.0, "To");
    let obstacle = shape(&mut s, 200.0, 10.0, 80.0, 80.0, "Obstacle");
    let c = id(&s.execute("board.connector", &json!({"from":a,"to":b,"style":"elbow"})).unwrap(), "id");
    clear(&s, c, &[obstacle]);
    let old = drawn(&s, c);
    let turn_y = old.iter().find(|p| (p.y - 50.0).abs() > 1.0).unwrap().y;
    let mut supplied = s.doc().unwrap().clone();
    let template = supplied.node(obstacle).unwrap().clone();
    for i in 0..254 {
        let mut shape = template.clone();
        shape.id = supplied.alloc_id();
        shape.children.clear();
        shape.layout = None;
        shape.transform = figcraft_geom::Affine::translate((1000.0 + i as f64 * 100.0, 1000.0));
        supplied.insert(shape, template.parent.unwrap(), None).unwrap();
    }
    assert!(supplied.validate().is_empty());
    let mut s = Session::with_document(supplied);
    s.execute("node.set", &json!({"id":obstacle,"props":{"y":turn_y-20.0,"height":40}})).unwrap();
    let new_bounds = s.doc().unwrap().world_bounds(obstacle).unwrap();
    assert!(old.windows(2).any(|w| crosses(w[0], w[1], new_bounds)), "old detour is invalid after this edit");
    assert_eq!(
        drawn(&s, c),
        vec![Point::new(104.0, 50.0), Point::new(396.0, 50.0)],
        "257 owner records refuse the entire context and compute fresh legacy geometry"
    );
    clear(&s, c, &[obstacle]);
}

#[test]
fn moving_many_avoided_connectors_keeps_each_free_endpoint_at_its_actual_translation() {
    let mut s = board();
    let a = shape(&mut s, 0.0, 0.0, 100.0, 100.0, "Attached source");
    let obstacle = shape(&mut s, 200.0, 10.0, 80.0, 80.0, "Obstacle");
    let c = id(&s.execute("board.connector", &json!({"from":a,"fromSide":"right","to":{"x":400,"y":50},"style":"elbow"})).unwrap(), "id");
    clear(&s, c, &[obstacle]);
    assert!(drawn(&s, c).len() > 2);
    let mut supplied = s.doc().unwrap().clone();
    let original = supplied.node(c).unwrap().clone();
    let mut ids = vec![c];
    for _ in 0..64 {
        let mut n = original.clone();
        n.id = supplied.alloc_id();
        ids.push(n.id);
        supplied.insert(n, original.parent.unwrap(), None).unwrap();
    }
    let mut s = Session::with_document(supplied);
    let before = s.doc().unwrap().clone();
    let undo = s.undo_depth().0;
    s.execute("transform.move", &json!({"ids":ids,"dx":0,"dy":10})).unwrap();
    for c in ids {
        assert_eq!(
            connector(&s, c).to,
            figcraft_doc::End::Point { x: 400.0, y: 60.0 },
            "free endpoint moves10 units regardless of shared avoidance exhaustion: {c:?}"
        );
        assert_eq!(drawn(&s, c).last(), Some(&Point::new(400.0, 60.0)));
    }
    assert_eq!(s.undo_depth().0, undo + 1);
    s.undo().unwrap();
    assert_eq!(s.doc().unwrap(), &before);
}
