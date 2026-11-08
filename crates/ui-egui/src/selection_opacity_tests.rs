//! Real Preferences input and Actions routing for selection opacity.
use egui::{Event, Key, Modifiers, Pos2, Rect, pos2};
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::json;

use crate::layers_drag_tests::Ui;

fn setup() -> (Ui, NodeId) {
    let mut s = Session::new();
    let id = NodeId(s.execute("shape.rectangle", &json!({"width":120,"height":80})).unwrap()["id"].as_u64().unwrap());
    let u = Ui::new(s);
    u.ctx.enable_accesskit();
    (u, id)
}

fn named(u: &mut Ui, role: egui::accesskit::Role, label: &str) -> Pos2 {
    let out = u.frame_output(vec![]);
    let update = out.platform_output.accesskit_update.unwrap();
    let n = update
        .nodes
        .iter()
        .map(|(_, n)| n)
        .find(|n| n.role() == role && n.label() == Some(label))
        .unwrap_or_else(|| panic!("{label} must be accessible"));
    let b = n.bounds().unwrap();
    let rect = Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32));
    assert!(u.ctx.content_rect().contains_rect(rect), "{label} must be visible for a real click: {rect:?}");
    rect.center()
}

fn click(u: &mut Ui, at: Pos2) {
    u.button(at, true);
    u.button(at, false);
}

#[test]
fn selection_outline_opacity_preferences_edit_reopen_reset_and_property_undo() {
    let (mut u, id) = setup();
    let doc = u.app.session.ds().unwrap().doc.clone();
    let history = u.app.session.undo_depth();
    let selection = u.app.session.selection();
    u.app.ui.preferences = true;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let at = named(&mut u, egui::accesskit::Role::SpinButton, "Selection outline opacity");
    click(&mut u, at);
    u.frame(vec![Event::Text("35.5".into())]);
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert_eq!(u.app.session.prefs.selection_outline_opacity(), 35.5);
    assert!(std::sync::Arc::ptr_eq(&doc, &u.app.session.ds().unwrap().doc));
    assert_eq!(u.app.session.undo_depth(), history);
    assert_eq!(u.app.session.selection(), selection);
    u.app.ui.preferences = false;
    u.frame(vec![]);
    u.app.set_props(&[id], json!({"topLeftRadius":20}));
    assert_eq!(u.app.session.doc().unwrap().node(id).unwrap().corners.radii[0], 20.0);
    u.app.session.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(u.app.session.doc().unwrap().node(id).unwrap().corners.radii[0], 0.0);
    assert_eq!(u.app.session.prefs.selection_outline_opacity(), 35.5, "document Undo keeps the preference");
    u.app.ui.preferences = true;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    named(&mut u, egui::accesskit::Role::SpinButton, "Selection outline opacity");
    let reset = named(&mut u, egui::accesskit::Role::Button, "Reset selection outline opacity");
    click(&mut u, reset);
    assert_eq!(u.app.session.prefs.selection_outline_opacity(), 100.0);
    assert_eq!(u.app.session.selection(), selection);
}

#[test]
fn selection_outline_opacity_actions_opens_the_editable_preference_without_query_side_effects() {
    let (mut u, _) = setup();
    let doc = u.app.session.ds().unwrap().doc.clone();
    let history = u.app.session.undo_depth();
    crate::menus::set_palette(&mut u.app.ui, true);
    u.app.ui.palette = Some("selection outline op".into());
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let shapes = crate::live_tests::flat(&u.frame_output(vec![]).shapes);
    let at = crate::live_tests::text_center(&shapes, "Selection outline opacity…");
    click(&mut u, at);
    for _ in 0..3 {
        u.frame(vec![]);
    }
    assert!(u.app.ui.preferences);
    assert!(u.app.ui.palette.is_none());
    named(&mut u, egui::accesskit::Role::SpinButton, "Selection outline opacity");
    assert_eq!(u.app.session.prefs.selection_outline_opacity(), 100.0);
    assert!(std::sync::Arc::ptr_eq(&doc, &u.app.session.ds().unwrap().doc));
    assert_eq!(u.app.session.undo_depth(), history);
}

#[test]
fn selection_outline_opacity_escape_retires_an_uncommitted_preferences_draft() {
    let (mut u, _) = setup();
    let selection = u.app.session.selection();
    let history = u.app.session.undo_depth();
    u.app.ui.preferences = true;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let at = named(&mut u, egui::accesskit::Role::SpinButton, "Selection outline opacity");
    click(&mut u, at);
    u.frame(vec![Event::Text("12".into())]);
    let old_focus = u.ctx.memory(|m| m.focused()).expect("the numeric draft owns focus");
    assert_eq!(u.app.session.prefs.selection_outline_opacity(), 100.0, "typing is not a committed preference edit");
    u.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    u.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(!u.app.ui.preferences);
    assert_ne!(u.ctx.memory(|m| m.focused()), Some(old_focus), "a closed Preferences opening retires its field focus");
    u.app.ui.preferences = true;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let at = named(&mut u, egui::accesskit::Role::SpinButton, "Selection outline opacity");
    click(&mut u, at);
    assert_ne!(u.ctx.memory(|m| m.focused()), Some(old_focus), "a new opening owns a fresh editor");
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert_eq!(u.app.session.prefs.selection_outline_opacity(), 100.0, "the old uncommitted draft cannot reappear");
    assert_eq!(u.app.session.selection(), selection);
    assert_eq!(u.app.session.undo_depth(), history);
}
