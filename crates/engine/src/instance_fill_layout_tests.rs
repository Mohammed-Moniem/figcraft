//! Original public-command #2090 cross-axis Fill/Hug component workflows.
use crate::Session;
use crate::components::tests::{copy_of, ex, idv, node};
use figcraft_doc::{Node, NodeId, Sizing};
use serde_json::json;

struct Scene {
    s: Session,
    main: NodeId,
    bar: NodeId,
    sibling: NodeId,
    instance: NodeId,
    horizontal: bool,
}
fn scene(horizontal: bool) -> Scene {
    let mut s = Session::new();
    let (width, height, props) = if horizontal {
        (200, 60, json!({"layout":"horizontal","padding":6,"gap":12,"sizingV":"hug"}))
    } else {
        (60, 200, json!({"layout":"vertical","padding":6,"gap":12,"sizingH":"hug"}))
    };
    let frame = idv(&ex(&mut s, "frame.create", json!({"width":width,"height":height,"props":props})));
    let bar = idv(&ex(
        &mut s,
        "shape.rectangle",
        json!({"parent":frame,"width":if horizontal {3}else{20},"height":if horizontal {20}else{3},"name":"Original accent"}),
    ));
    let sibling = idv(&ex(
        &mut s,
        "shape.rectangle",
        json!({"parent":frame,"width":if horizontal {50}else{44},"height":if horizontal {44}else{50},"name":"Original fixed sibling"}),
    ));
    let main = idv(&ex(&mut s, "component.create", json!({"ids":[frame]})));
    let instance = idv(&ex(&mut s, "instance.create", json!({"component":main,"x":320,"y":0})));
    Scene { s, main, bar, sibling, instance, horizontal }
}
fn cross_size(n: &Node, horizontal: bool) -> f64 {
    if horizontal { n.height } else { n.width }
}
fn cross_flag(n: &Node, horizontal: bool) -> Sizing {
    if horizontal { n.child_layout.sizing_v } else { n.child_layout.sizing_h }
}
fn fill(scene: &mut Scene) {
    let props = if scene.horizontal { json!({"sizingV":"fill"}) } else { json!({"sizingH":"fill"}) };
    ex(&mut scene.s, "node.set", json!({"ids":[scene.bar],"props":props}));
}
fn expected(scene: &Scene, cross: f64) {
    for root in [scene.main, scene.instance] {
        assert_eq!(cross_flag(&node(&scene.s, root), scene.horizontal), Sizing::Hug, "parent {root} remains Hug");
        assert_eq!(cross_size(&node(&scene.s, root), scene.horizontal), cross + 12.0);
    }
    for id in [scene.bar, copy_of(&scene.s, scene.instance, scene.bar)] {
        assert_eq!(cross_flag(&node(&scene.s, id), scene.horizontal), Sizing::Fill);
        assert_eq!(cross_size(&node(&scene.s, id), scene.horizontal), cross, "main and existing instance stretch on the same edit");
    }
}

#[test]
fn cross_fill_preserves_hug_on_a_horizontal_component_and_existing_instance() {
    let mut x = scene(true);
    fill(&mut x);
    expected(&x, 44.0);
}
#[test]
fn cross_fill_preserves_hug_on_the_symmetric_vertical_component() {
    let mut x = scene(false);
    fill(&mut x);
    expected(&x, 44.0);
}
#[test]
fn cross_fill_existing_instance_height_changes_on_the_main_command_itself() {
    let mut x = scene(true);
    let copy = copy_of(&x.s, x.instance, x.bar);
    assert_eq!(node(&x.s, copy).height, 20.0);
    fill(&mut x);
    assert_eq!(node(&x.s, x.bar).height, 44.0, "original main's local Fill control already stretches");
    assert_eq!(node(&x.s, copy).child_layout.sizing_v, Sizing::Fill);
    assert_eq!(node(&x.s, copy).height, 44.0, "do not require a redundant command on the copy");
}
#[test]
fn cross_fill_tracks_a_nonfill_siblings_growth_and_shrink_without_feedback() {
    for horizontal in [true, false] {
        let mut x = scene(horizontal);
        fill(&mut x);
        for cross in [72.0, 24.0, 44.0] {
            let props = if horizontal { json!({"height":cross}) } else { json!({"width":cross}) };
            ex(&mut x.s, "node.set", json!({"ids":[x.sibling],"props":props}));
            expected(&x, cross);
        }
    }
}
#[test]
fn cross_fill_is_one_undo_redo_and_saves_the_resolved_live_component() {
    let mut x = scene(true);
    let original = x.s.doc().unwrap().clone();
    let history = x.s.undo_depth();
    fill(&mut x);
    expected(&x, 44.0);
    assert_eq!(x.s.undo_depth().0, history.0 + 1);
    let result = x.s.doc().unwrap().clone();
    x.s.undo().unwrap();
    assert_eq!(x.s.doc().unwrap(), &original);
    x.s.redo().unwrap();
    assert_eq!(x.s.doc().unwrap(), &result);
    let (back, _) = figcraft_format::load_bytes(&figcraft_format::save_bytes(&result, None).unwrap()).unwrap();
    for id in [x.main, x.bar, x.sibling, x.instance, copy_of(&x.s, x.instance, x.bar)] {
        assert_eq!(back.node(id), result.node(id));
    }
}
#[test]
fn cross_fill_main_axis_and_grid_parent_rules_are_preserved_controls() {
    for mode in ["horizontal", "grid"] {
        let mut s = Session::new();
        let props = if mode == "horizontal" {
            json!({"layout":mode,"padding":6,"sizingH":"hug"})
        } else {
            json!({"layout":mode,"padding":6,"sizingV":"hug"})
        };
        let frame = idv(&ex(&mut s, "frame.create", json!({"width":120,"height":80,"props":props})));
        let child = idv(&ex(&mut s, "shape.rectangle", json!({"parent":frame,"width":20,"height":30})));
        let props = if mode == "horizontal" { json!({"sizingH":"fill"}) } else { json!({"sizingV":"fill"}) };
        ex(&mut s, "node.set", json!({"ids":[child],"props":props}));
        assert_eq!(if mode == "horizontal" { node(&s, frame).child_layout.sizing_h } else { node(&s, frame).child_layout.sizing_v }, Sizing::Fixed);
    }
}
#[test]
fn cross_fill_outside_flow_and_absolute_children_keep_existing_controls() {
    let mut s = Session::new();
    let loose = idv(&ex(&mut s, "shape.rectangle", json!({"width":20,"height":30})));
    let before = s.doc().unwrap().clone();
    let history = s.undo_depth();
    assert!(s.execute("node.set", &json!({"ids":[loose],"props":{"sizingV":"fill"}})).is_err());
    assert_eq!(s.doc().unwrap(), &before);
    assert_eq!(s.undo_depth(), history);
    let frame = idv(&ex(&mut s, "frame.create", json!({"width":120,"height":80,"props":{"layout":"horizontal"}})));
    let child = idv(&ex(&mut s, "shape.rectangle", json!({"parent":frame,"width":20,"height":30,"props":{"absolute":true}})));
    ex(&mut s, "node.set", json!({"ids":[child],"props":{"sizingV":"fill"}}));
    assert_eq!(node(&s, child).child_layout.sizing_v, Sizing::Fixed);
    assert_eq!(node(&s, child).height, 30.0);
    assert_eq!(node(&s, frame).height, 80.0);
}
