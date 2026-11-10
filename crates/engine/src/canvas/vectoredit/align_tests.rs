//! Original public-command contracts for #2109; no candidate-only helper is used by tests.

use super::tests::{ex, net, pt};
use super::*;
use figcraft_doc::NodeId;
use figcraft_geom::{Point, Vec2};
use serde_json::{Value, json};

const COMMANDS: [&str; 8] =
    ["align.left", "align.hcenter", "align.right", "align.top", "align.vcenter", "align.bottom", "distribute.horizontal", "distribute.vertical"];

fn create(s: &mut Session, points: Value) -> NodeId {
    crate::params::id_of(&ex(s, "vector.create", json!({"points":points,"closed":false}))["id"]).unwrap()
}

fn fixture() -> (Session, NodeId) {
    let mut s = Session::new();
    let parent = crate::params::id_of(&ex(&mut s, "frame.create", json!({"x":100,"y":100,"width":400,"height":400}))["id"]).unwrap();
    let id = crate::params::id_of(
        &ex(&mut s, "vector.create", json!({"parent":parent,"points":[[20,80],[80,20],[140,100],[50,160]],"closed":false}))["id"],
    )
    .unwrap();
    ex(&mut s, "vector.edit.begin", json!({"id":id}));
    (s, id)
}

fn select(s: &mut Session, vertices: &[usize]) {
    ex(s, "vector.edit.select", json!({"vertices":vertices}));
}

fn page(s: &Session, id: NodeId) -> Vec<Point> {
    let world = s.doc().unwrap().world_transform(id);
    net(s, id).vertices.iter().map(|point| world * point.point()).collect()
}

fn near(actual: Point, expected: Point) -> bool {
    (actual - expected).hypot() < 1e-6
}

#[test]
fn issue_2109_all_six_alignments_use_only_the_selected_points_box() {
    for (command, a, b) in [
        ("align.left", pt(120.0, 180.0), pt(120.0, 120.0)),
        ("align.hcenter", pt(150.0, 180.0), pt(150.0, 120.0)),
        ("align.right", pt(180.0, 180.0), pt(180.0, 120.0)),
        ("align.top", pt(120.0, 120.0), pt(180.0, 120.0)),
        ("align.vcenter", pt(120.0, 150.0), pt(180.0, 150.0)),
        ("align.bottom", pt(120.0, 180.0), pt(180.0, 180.0)),
    ] {
        let (mut s, id) = fixture();
        select(&mut s, &[0, 1]);
        let before = page(&s, id);
        ex(&mut s, command, json!({}));
        let after = page(&s, id);
        assert!(near(after[0], a) && near(after[1], b), "{command}: {after:?}");
        assert!(near(after[2], before[2]) && near(after[3], before[3]), "unselected points stay: {command}");
    }
}

#[test]
fn issue_2109_distribute_is_even_between_extremes_and_ignores_layer_spacing() {
    for (command, expected) in [
        ("distribute.horizontal", [pt(120.0, 180.0), pt(200.0, 120.0), pt(240.0, 200.0), pt(160.0, 260.0)]),
        ("distribute.vertical", [pt(120.0, 100.0 + 200.0 / 3.0), pt(180.0, 120.0), pt(240.0, 100.0 + 340.0 / 3.0), pt(150.0, 260.0)]),
    ] {
        let (mut s, id) = fixture();
        select(&mut s, &[0, 1, 2, 3]);
        ex(&mut s, command, json!({"spacing":999}));
        let actual = page(&s, id);
        assert!(actual.iter().zip(expected).all(|(a, b)| near(*a, b)), "{command}: {actual:?}");
    }
}

#[test]
fn issue_2109_small_point_selections_are_true_noops_without_layer_fallback() {
    for count in 0..=2 {
        let (mut s, id) = fixture();
        select(&mut s, &(0..count).collect::<Vec<_>>());
        let before = s.doc().unwrap().clone();
        let depth = s.undo_depth();
        for command in COMMANDS {
            if count == 2 && command.starts_with("align.") {
                continue;
            }
            ex(&mut s, command, json!({}));
            assert_eq!(s.doc().unwrap(), &before, "{count} points: {command} moved/refit a layer");
            assert_eq!(s.undo_depth(), depth, "{count} points: {command} added an Undo");
            assert!(s.doc().unwrap().contains(id));
        }
    }
}

#[test]
fn issue_2109_explicit_scalar_id_aligns_the_layer_and_keeps_local_points() {
    let (mut s, id) = fixture();
    select(&mut s, &[0, 1]);
    let vertices = net(&s, id).vertices;
    let parent = s.doc().unwrap().node(id).unwrap().parent.unwrap();
    ex(&mut s, "align.left", json!({"id":id}));
    assert!((s.doc().unwrap().world_bounds(id).unwrap().x0 - s.doc().unwrap().world_bounds(parent).unwrap().x0).abs() < 1e-6);
    assert_eq!(net(&s, id).vertices, vertices);
}

#[test]
fn issue_2109_explicit_ids_align_and_distribute_layers_without_editing_points() {
    let mut s = Session::new();
    let mut rect = |x| crate::params::id_of(&ex(&mut s, "shape.rectangle", json!({"x":x,"width":40,"height":40}))["id"]).unwrap();
    let (a, b, c) = (rect(0), rect(50), rect(300));
    ex(&mut s, "vector.edit.begin", json!({"id":b}));
    select(&mut s, &[0, 1]);
    let local = net(&s, b).vertices;
    ex(&mut s, "distribute.horizontal", json!({"ids":[a,b,c]}));
    assert!((s.doc().unwrap().world_bounds(b).unwrap().x0 - 150.0).abs() < 1e-6);
    assert_eq!(net(&s, b).vertices, local);
    ex(&mut s, "align.left", json!({"ids":[a,b,c]}));
    let d = s.doc().unwrap();
    assert!((d.world_bounds(a).unwrap().x0 - d.world_bounds(c).unwrap().x0).abs() < 1e-6);
    assert_eq!(net(&s, b).vertices, local);
}

#[test]
fn issue_2109_null_target_keys_are_absent_but_invalid_explicit_ids_keep_errors() {
    for params in [json!({"id":null}), json!({"ids":null}), json!({"id":null,"ids":null})] {
        let (mut s, id) = fixture();
        select(&mut s, &[0, 1]);
        ex(&mut s, "align.left", params);
        let points = page(&s, id);
        assert!(near(points[0], pt(120.0, 180.0)) && near(points[1], pt(120.0, 120.0)));
    }
    for params in [json!({"id":"invalid"}), json!({"ids":"invalid"}), json!({"ids":["invalid"]}), json!({"ids":[]})] {
        let (mut s, _) = fixture();
        select(&mut s, &[0, 1]);
        let before = s.doc().unwrap().clone();
        let history = s.undo_depth();
        assert!(s.execute("align.left", &params).is_err());
        assert_eq!(s.doc().unwrap(), &before);
        assert_eq!(s.undo_depth(), history);
    }
}

#[test]
fn issue_2109_rotated_single_layer_uses_the_existing_point_box_axes() {
    let (mut s, id) = fixture();
    ex(&mut s, "node.set", json!({"id":id,"props":{"rotation":30}}));
    select(&mut s, &[0, 1]);
    let edit = editing(&s).unwrap();
    let b = transform::point_box(s.doc().unwrap(), &edit).unwrap();
    let inverse = b.frame.inverse();
    let before = page(&s, id);
    let local: Vec<_> = before.iter().map(|p| inverse * *p).collect();
    ex(&mut s, "align.left", json!({}));
    let after = page(&s, id);
    for i in 0..2 {
        let p = inverse * after[i];
        assert!(p.x.abs() < 1e-6);
        assert!((p.y - local[i].y).abs() < 1e-6);
    }
    assert!(near(after[2], before[2]) && near(after[3], before[3]));
}

#[test]
fn issue_2109_rotated_multi_edit_uses_page_axes_and_one_public_undo() {
    let mut s = Session::new();
    let a = create(&mut s, json!([[0, 40], [80, 120]]));
    let b = create(&mut s, json!([[240, 0], [320, 100]]));
    ex(&mut s, "node.set", json!({"id":a,"props":{"rotation":-20}}));
    ex(&mut s, "node.set", json!({"id":b,"props":{"rotation":30}}));
    ex(&mut s, "vector.edit.begin", json!({"ids":[a,b]}));
    ex(&mut s, "vector.edit.select", json!({"id":a,"vertices":[0]}));
    ex(&mut s, "vector.edit.select", json!({"id":b,"vertices":[1],"add":true}));
    let (pa, pb) = (page(&s, a), page(&s, b));
    let y = pa[0].y.min(pb[1].y);
    let depth = s.undo_depth().0;
    ex(&mut s, "align.top", json!({}));
    let (qa, qb) = (page(&s, a), page(&s, b));
    assert!(near(qa[0], pt(pa[0].x, y)) && near(qb[1], pt(pb[1].x, y)));
    assert!(near(qa[1], pa[1]) && near(qb[0], pb[0]));
    assert_eq!(s.undo_depth().0, depth + 1);
    ex(&mut s, "edit.undo", json!({}));
    assert!(page(&s, a).iter().zip(&pa).chain(page(&s, b).iter().zip(&pb)).all(|(x, y)| near(*x, *y)));
    ex(&mut s, "edit.redo", json!({}));
    assert!(page(&s, a).iter().zip(&qa).chain(page(&s, b).iter().zip(&qb)).all(|(x, y)| near(*x, *y)));
}

#[test]
fn issue_2109_other_layer_segment_endpoints_are_counted_once_for_distribution() {
    let mut s = Session::new();
    let a = create(&mut s, json!([[0, 0], [70, 60]]));
    let b = create(&mut s, json!([[210, 140], [280, 200]]));
    ex(&mut s, "vector.edit.begin", json!({"ids":[a,b]}));
    ex(&mut s, "vector.edit.select", json!({"id":a,"segments":[0],"vertices":[1]}));
    ex(&mut s, "vector.edit.select", json!({"id":b,"vertices":[0],"add":true}));
    let unchanged = page(&s, b)[1];
    ex(&mut s, "distribute.horizontal", json!({}));
    let (a, b) = (page(&s, a), page(&s, b));
    assert!(near(a[0], pt(0.0, 0.0)) && near(a[1], pt(105.0, 60.0)) && near(b[0], pt(210.0, 140.0)));
    assert!(near(b[1], unchanged));
}

#[test]
fn issue_2109_points_in_another_layer_work_with_an_empty_active_selection() {
    let mut s = Session::new();
    let a = create(&mut s, json!([[10, 40], [70, 100], [100, 140]]));
    let b = create(&mut s, json!([[200, 0], [250, 30]]));
    ex(&mut s, "vector.edit.begin", json!({"ids":[a,b]}));
    ex(&mut s, "vector.edit.select", json!({"id":a,"vertices":[0,1]}));
    ex(&mut s, "vector.edit.select", json!({"id":b,"vertices":[],"add":true}));
    let unchanged = page(&s, b);
    ex(&mut s, "align.right", json!({}));
    let points = page(&s, a);
    assert!(near(points[0], pt(70.0, 40.0)) && near(points[1], pt(70.0, 100.0)));
    assert_eq!(page(&s, b), unchanged);
}

#[test]
fn issue_2109_point_handles_and_corner_mirror_cap_join_metadata_travel_together() {
    let mut s = Session::new();
    let id = crate::params::id_of(&ex(&mut s, "vector.create", json!({"path":"M 0 0 C 0 20 40 60 80 60 L 160 0","x":0,"y":0}))["id"]).unwrap();
    ex(&mut s, "vector.edit.begin", json!({"id":id}));
    select(&mut s, &[0, 1]);
    ex(&mut s, "vector.edit.cornerRadius", json!({"radius":3}));
    ex(&mut s, "vector.edit.mirroring", json!({"mode":"angleAndLength"}));
    ex(&mut s, "vector.edit.strokeCap", json!({"cap":"round"}));
    ex(&mut s, "vector.edit.strokeJoin", json!({"join":"bevel"}));
    let before = net(&s, id);
    let before_page = page(&s, id);
    let world = s.doc().unwrap().world_transform(id);
    let handle = world * (before.vertices[1].point() + Vec2::new(before.segments[0].tb.0, before.segments[0].tb.1));
    ex(&mut s, "align.left", json!({}));
    let after = net(&s, id);
    let points = page(&s, id);
    let w = s.doc().unwrap().world_transform(id);
    let moved = w * (after.vertices[1].point() + Vec2::new(after.segments[0].tb.0, after.segments[0].tb.1));
    assert!(near(points[0], pt(0.0, 0.0)) && near(points[1], pt(0.0, 60.0)) && near(points[2], before_page[2]));
    assert!(near(moved, handle + Vec2::new(-80.0, 0.0)));
    assert_eq!(before.segments, after.segments);
    assert_eq!(before.regions, after.regions);
    for (a, b) in before.vertices.iter().zip(&after.vertices) {
        assert_eq!((a.radius, a.mirror, a.cap, a.join), (b.radius, b.mirror, b.cap, b.join));
    }
}

#[test]
fn issue_2109_single_layer_point_edit_is_one_public_undo_and_redo() {
    let (mut s, id) = fixture();
    select(&mut s, &[0, 1, 2, 3]);
    let before = page(&s, id);
    let history = s.undo_depth().0;
    ex(&mut s, "align.right", json!({}));
    let after = page(&s, id);
    assert_ne!(after, before);
    assert_eq!(s.undo_depth().0, history + 1);
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(page(&s, id), before);
    ex(&mut s, "edit.redo", json!({}));
    assert_eq!(page(&s, id), after);
}

#[test]
fn issue_2109_existing_locked_node_command_eligibility_and_lock_state_are_preserved() {
    let (mut actual, id) = fixture();
    select(&mut actual, &[0, 1]);
    ex(&mut actual, "node.set", json!({"id":id,"props":{"locked":true}}));
    let (mut reference, other) = fixture();
    select(&mut reference, &[0, 1]);
    ex(&mut reference, "node.set", json!({"id":other,"props":{"locked":true}}));
    // Existing programmatic point edits use the selected network despite the canvas lock.
    ex(&mut reference, "vector.edit.moveVertices", json!({"vertices":[1],"dx":-60,"dy":0}));
    ex(&mut actual, "align.left", json!({}));
    assert!(actual.doc().unwrap().node(id).unwrap().locked && reference.doc().unwrap().node(other).unwrap().locked);
    assert!(page(&actual, id).iter().zip(page(&reference, other)).all(|(a, b)| near(*a, b)));
}

#[test]
fn issue_2109_already_aligned_and_zero_extent_distribution_do_not_create_history() {
    let mut s = Session::new();
    let id = create(&mut s, json!([[0, 0], [30, 0], [60, 0]]));
    ex(&mut s, "vector.edit.begin", json!({"id":id}));
    select(&mut s, &[0, 1, 2]);
    let before = s.doc().unwrap().clone();
    let depth = s.undo_depth();
    ex(&mut s, "align.top", json!({}));
    ex(&mut s, "distribute.vertical", json!({}));
    ex(&mut s, "distribute.horizontal", json!({}));
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), depth);
}
