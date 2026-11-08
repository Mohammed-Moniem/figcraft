//! Actual egui canvas input and painted original circle for #1563.
use crate::layers_drag_tests::Ui;
use egui::{Event, Shape, vec2};
use figcraft_engine::{Session, params::id_of};
use serde_json::json;
fn scene() -> (Ui, figcraft_doc::NodeId) {
    let mut s = Session::new();
    let id=id_of(&s.execute("text.create",&json!({"text":"Original UI text\nTwo lines","x":100,"y":100,"width":240,"props":{"height":80,"fontFamily":"Inter","fontSize":20,"lineHeight":30,"textAutoResize":"none"}})).unwrap()["id"]).unwrap();
    (Ui::new(s), id)
}
fn circle(shape: &Shape, p: egui::Pos2) -> bool {
    match shape {
        Shape::Circle(c) => c.center.distance(p) < 0.5 && c.radius == 4.0 && c.fill == egui::Color32::WHITE,
        Shape::Vec(v) => v.iter().any(|s| circle(s, p)),
        _ => false,
    }
}
fn size(u: &Ui, id: figcraft_doc::NodeId) -> f64 {
    u.app.session.doc().unwrap().node(id).unwrap().kind.text().unwrap().style.font_size
}
#[test]
fn actual_text_circle_is_painted_and_dragging_it_changes_typography_once() {
    let (mut u, id) = scene();
    let a = u.app.to_screen(220.0, 180.0);
    let out = u.frame_output(vec![]);
    assert!(out.shapes.iter().any(|s| circle(&s.shape, a)), "original bottom-center circle is visible");
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    let z = u.app.session.ds().unwrap().view.zoom as f32;
    u.drag_to(a, a + vec2(0.0, 40.0) * z);
    assert!((size(&u, id) - 30.0).abs() < 1e-5);
    u.button(a + vec2(0.0, 40.0) * z, false);
    assert_eq!(u.app.session.undo_depth().0, history.0 + 1);
    u.app.run("edit.undo", json!({}));
    assert_eq!(u.app.session.doc().unwrap(), &before);
}
#[test]
fn actual_text_size_drag_escape_cancels_and_sidebar_press_never_owns_the_circle() {
    let (mut u, id) = scene();
    let a = u.app.to_screen(220.0, 180.0);
    let before = u.app.session.doc().unwrap().clone();
    let history = u.app.session.undo_depth();
    u.drag_to(a, a + vec2(0.0, 30.0));
    u.frame(vec![
        Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() },
        Event::Key { key: egui::Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Default::default() },
    ]);
    u.button(a + vec2(0.0, 30.0), false);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), history);
    let sidebar = egui::pos2(u.app.canvas.rect.max.x + 20.0, 700.0);
    u.button(sidebar, true);
    u.frame(vec![Event::PointerMoved(a)]);
    u.frame(vec![Event::PointerMoved(a + vec2(0.0, 40.0))]);
    u.button(a + vec2(0.0, 40.0), false);
    assert_eq!(size(&u, id), 20.0);
    assert!(!u.app.session.tool.busy());
}
