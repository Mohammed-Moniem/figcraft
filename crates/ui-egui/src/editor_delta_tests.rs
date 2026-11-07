//! Real headless UI contracts for adapted editor features, in addition to upstream scenario tests.
use crate::layers_drag_tests::Ui;
use crate::live_tests::{flat, has_text, text_center};
use egui::{Event, Modifiers, PointerButton, Pos2};
use figcraft_doc::{NodeId, StrokeCap};
use figcraft_engine::Session;
use serde_json::json;

fn click(u: &mut Ui, p: Pos2) {
    u.button(p, true);
    u.button(p, false);
    u.frame(vec![]);
}
fn scene() -> (Ui, NodeId, NodeId) {
    let mut s = Session::new();
    let mut ids = vec![];
    for (name, x) in [("Highlighted A", 0), ("Highlighted B", 100)] {
        ids.push(NodeId(
            s.execute("shape.rectangle", &json!({"x":x,"y":0,"width":40,"height":40,"props":{"name":name}})).unwrap()["id"].as_u64().unwrap(),
        ));
    }
    s.set_selection(ids.clone());
    let mut u = Ui::new(s);
    for _ in 0..3 {
        u.frame(vec![]);
    }
    (u, ids[0], ids[1])
}

#[test]
fn highlight_menu_preserves_multiselection_and_uses_saved_commands() {
    let (mut u, a, b) = scene();
    let at = u.row_at(a);
    for pressed in [true, false] {
        u.frame(vec![
            Event::PointerMoved(at),
            Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE },
        ]);
    }
    assert_eq!(u.app.session.selection(), vec![a, b]);
    let menu = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&menu, "Highlight color"));
    let at = text_center(&menu, "Highlight color");
    click(&mut u, at);
    for _ in 0..12 {
        u.frame(vec![]);
    }
    let colors = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&colors, "Blue"));
    let blue = text_center(&colors, "Blue");
    let undo = u.app.session.undo_depth().0;
    click(&mut u, blue);
    for id in [a, b] {
        assert_eq!(u.app.session.doc().unwrap().node(id).unwrap().highlight_color.unwrap().to_rgba8(), [0x3B, 0x82, 0xF6, 255]);
    }
    assert_eq!(u.app.session.undo_depth().0, undo + 1);
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    for id in [a, b] {
        assert!(u.app.session.doc().unwrap().node(id).unwrap().highlight_color.is_none());
    }
}

#[test]
fn width_profile_remains_inline_when_advanced_controls_are_closed() {
    let (mut u, a, _) = scene();
    u.app.session.execute("stroke.add", &json!({"ids":[a]})).unwrap();
    u.app.session.set_selection(vec![a]);
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(has_text(&shapes, "Width profile"));
    assert!(u.app.ui.stroke_settings.is_none());
}

#[test]
fn physical_connector_caps_update_opposite_directions_together_in_one_undo() {
    let mut s = Session::new();
    s.execute("file.newBoard", &json!({})).unwrap();
    let mut ids = vec![];
    for (from, to) in [(json!({"x":0,"y":0}), json!({"x":400,"y":100})), (json!({"x":400,"y":200}), json!({"x":0,"y":100}))] {
        ids.push(NodeId(s.execute("board.connector", &json!({"from":from,"to":to,"style":"straight"})).unwrap()["id"].as_u64().unwrap()));
    }
    let mut u = Ui::new(s);
    let undo = u.app.session.undo_depth().0;
    crate::board_bar::set_physical_cap(&mut u.app, &ids, true, StrokeCap::CircleFilled);
    let d = u.app.session.doc().unwrap();
    assert_eq!(d.node(ids[0]).unwrap().stroke.cap_start, StrokeCap::CircleFilled);
    assert_eq!(d.node(ids[1]).unwrap().stroke.cap_end, StrokeCap::CircleFilled);
    assert_eq!(u.app.session.undo_depth().0, undo + 1);
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    let d = u.app.session.doc().unwrap();
    assert_eq!(d.node(ids[0]).unwrap().stroke.cap_start, StrokeCap::None);
    assert_eq!(d.node(ids[1]).unwrap().stroke.cap_end, StrokeCap::TriangleArrow);
}

#[test]
fn vertical_connector_end_order_uses_top_and_bottom() {
    let mut s = Session::new();
    s.execute("file.newBoard", &json!({})).unwrap();
    for reverse in [false, true] {
        let (a, b) = if reverse { (400, 0) } else { (0, 400) };
        let id = NodeId(s.execute("board.connector", &json!({"from":{"x":0,"y":a},"to":{"x":0,"y":b}})).unwrap()["id"].as_u64().unwrap());
        assert_eq!(crate::board_bar::physical_start_is_left(s.doc().unwrap(), id), Some(!reverse));
    }
}

#[test]
fn switching_files_clears_motion_targets_and_finishes_only_the_original_gesture() {
    use figcraft_doc::anim::motion::{AnimProperty, AnimValue};
    let (mut u, a, _) = scene();
    crate::motion::set_open(&mut u.app, true);
    u.frame(vec![]);
    let uid = u.app.session.ds().unwrap().uid;
    u.app.session.execute("file.new", &json!({})).unwrap();
    let other = NodeId(u.app.session.execute("shape.rectangle", &json!({"x":0,"y":0,"width":40,"height":40})).unwrap()["id"].as_u64().unwrap());
    let newer = u.app.session.active;
    let old = u.app.session.docs.iter().position(|d| d.uid == uid).unwrap();
    u.app.session.active = old;
    crate::motion::sync_owner(&mut u.app);
    u.app.session.begin_interaction("Edit animation clip").unwrap();
    u.app.session.execute("motion.keyframe", &json!({"ids":[a],"property":"Opacity","value":AnimValue::Scalar(0.4)})).unwrap();
    u.app.ui.motion.selected = Some(crate::motion::SelectedKey { node: a, property: AnimProperty::Opacity, time_ms: 0 });
    u.app.ui.motion.clip_popover = Some((a, 0));
    u.app.ui.motion.playing = true;
    u.app.session.active = newer;
    u.frame(vec![]);
    assert!(u.app.ui.motion.selected.is_none() && u.app.ui.motion.clip_popover.is_none());
    assert!(!u.app.ui.motion.playing);
    assert!(u.app.session.docs[old].interaction.is_none());
    assert!(u.app.session.doc().unwrap().node(other).unwrap().motion.is_empty());
    assert!(!u.app.session.docs[old].doc.node(a).unwrap().motion.is_empty());
}
