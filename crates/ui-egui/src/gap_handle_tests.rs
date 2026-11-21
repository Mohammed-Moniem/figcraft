//! The on-canvas gap handles of an auto layout frame whose gap is automatic (the lines of a
//! wrapping layout spread across it, or the items spread along it): the handle shows the distance
//! the layout gives the gap, and dragging it makes the gap a fixed one from there, as typing a gap
//! in the panel does. Driven through egui headless.

use egui::{Event, Modifiers, PointerButton, vec2};
use figcraft_doc::{ContentAlign, MainAlign, NodeId};
use figcraft_engine::Session;
use serde_json::{Value, json};

use crate::layers_drag_tests::Ui;

fn id(v: Value) -> NodeId {
    figcraft_engine::params::id_of(&v["id"]).unwrap()
}

/// A 300 × 300 frame of three 120 × 60 rectangles with `props` on top of a horizontal layout
/// without padding (gap 10): selected, on screen.
fn scene(props: Value) -> (Ui, NodeId) {
    let mut s = Session::new();
    let mut all = json!({"layout": "horizontal", "padding": 0, "gap": 10, "crossGap": 10});
    all.as_object_mut().unwrap().extend(props.as_object().unwrap().clone());
    let f = id(s.execute("frame.create", &json!({"x": 100, "y": 100, "width": 300, "height": 300, "props": all})).unwrap());
    for _ in 0..3 {
        s.execute("shape.rectangle", &json!({"parent": f, "x": 0, "y": 0, "width": 120, "height": 60})).unwrap();
    }
    s.execute("select.set", &json!({"ids": [f]})).unwrap();
    (Ui::new(s), f)
}

/// Press at page point `from`, move by `by` page units in steps, release.
fn drag(u: &mut Ui, from: (f64, f64), by: (f32, f32)) {
    drag_with(u, from, by, Modifiers::default());
}

fn drag_with(u: &mut Ui, from: (f64, f64), by: (f32, f32), modifiers: Modifiers) {
    let p = u.app.to_screen(from.0, from.1);
    let zoom = u.app.session.ds().unwrap().view.zoom as f32;
    u.frame_with(vec![Event::PointerMoved(p)], modifiers);
    u.frame_with(vec![Event::PointerMoved(p)], modifiers);
    u.frame_with(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers }], modifiers);
    for k in 1..=4 {
        u.frame_with(vec![Event::PointerMoved(p + vec2(by.0, by.1) * zoom * (k as f32 / 4.0))], modifiers);
    }
    let pos = p + vec2(by.0, by.1) * zoom;
    u.frame_with(vec![Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers }], modifiers);
    u.frame(vec![]);
}

#[test]
fn dragging_the_gap_between_spread_rows_fixes_it_from_where_it_was() {
    // Two rows of 60 in 300: spread, they are 180 apart (the stored row gap of 10 doesn't count).
    let (mut u, f) = scene(json!({"wrap": true, "contentAlign": "spaceBetween"}));
    // Between the rows, at the frame's middle: page (250, 250). 20 down: 180 + 20.
    drag(&mut u, (250.0, 250.0), (0.0, 20.0));
    let l = u.app.session.doc().unwrap().node(f).unwrap().layout.clone().unwrap();
    assert_eq!((l.content_align, l.cross_gap), (ContentAlign::Auto, 200.0));
    // One undo step for the whole drag.
    u.app.run("edit.undo", json!({}));
    let l = u.app.session.doc().unwrap().node(f).unwrap().layout.clone().unwrap();
    assert_eq!((l.content_align, l.cross_gap), (ContentAlign::SpaceBetween, 10.0));
}

#[test]
fn dragging_an_auto_gap_between_items_fixes_it_too_and_fixed_gaps_drag_as_before() {
    // Three 60-wide items spread along 300: (300 - 3 × 60) / 2 = 60 apart.
    let (mut u, f) = scene(json!({"mainAlign": "spaceBetween"}));
    for k in u.app.session.doc().unwrap().node(f).unwrap().children.clone() {
        u.app.run("node.set", json!({"id": k, "props": {"width": 60}}));
    }
    u.frame(vec![]);
    // Between the first two items: x from 160 to 220 on the page, in the items' height.
    drag(&mut u, (190.0, 130.0), (-10.0, 0.0));
    let l = u.app.session.doc().unwrap().node(f).unwrap().layout.clone().unwrap();
    assert_eq!((l.main_align, l.gap), (MainAlign::Min, 50.0));
    // A fixed gap: the drag changes the value and nothing else.
    drag(&mut u, (185.0, 130.0), (6.0, 0.0));
    let l = u.app.session.doc().unwrap().node(f).unwrap().layout.clone().unwrap();
    assert_eq!((l.main_align, l.gap), (MainAlign::Min, 56.0));
}

#[test]
fn shift_snaps_padding_to_big_nudge_and_alt_links_edges() {
    let original = [9.0, 5.0, 2.0, 7.0];
    for (mods, expected) in [
        (Modifiers::default(), [22.0, 5.0, 2.0, 7.0]),
        (Modifiers::SHIFT, [24.0, 5.0, 2.0, 7.0]),
        (Modifiers::ALT, [22.0, 5.0, 22.0, 7.0]),
        (Modifiers { shift: true, alt: true, ..Default::default() }, [24.0; 4]),
    ] {
        let (mut u, f) = scene(json!({"paddingTop": 9, "paddingRight": 5, "paddingBottom": 2, "paddingLeft": 7}));
        u.app.run("pref.nudge", json!({"big": 8}));
        drag_with(&mut u, (250.0, 104.0), (0.0, 13.0), mods);
        let padding = |u: &Ui| u.app.session.doc().unwrap().node(f).unwrap().layout.as_ref().unwrap().padding;
        assert_eq!(padding(&u), expected, "{mods:?}");
        u.app.run("edit.undo", json!({}));
        assert_eq!(padding(&u), original);
    }
}

#[test]
fn shift_snaps_item_and_automatic_row_gaps_to_the_preference() {
    let (mut u, f) = scene(json!({"wrap": true, "contentAlign": "spaceBetween"}));
    u.app.run("pref.nudge", json!({"big": 7.5}));
    drag_with(&mut u, (225.0, 130.0), (8.0, 0.0), Modifiers::SHIFT);
    assert_eq!(u.app.session.doc().unwrap().node(f).unwrap().layout.as_ref().unwrap().gap, 15.0);
    drag_with(&mut u, (250.0, 250.0), (0.0, 10.0), Modifiers::SHIFT);
    let layout = u.app.session.doc().unwrap().node(f).unwrap().layout.as_ref().unwrap();
    assert_eq!((layout.content_align, layout.cross_gap), (ContentAlign::Auto, 187.5));
}
