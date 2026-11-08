//! Real preferences input, persistence, grid paint and ruler drags with custom cell sizes.
use crate::layers_drag_tests::Ui;
use crate::live_tests::{flat, text_center, texts};
use egui::{Color32, Event, Key, Modifiers, Pos2, Shape};
use figcraft_engine::{Prefs, Session};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

struct SavePrefs(Arc<Mutex<Vec<Value>>>);
impl crate::Services for SavePrefs {
    fn save_prefs(&mut self, prefs: &Prefs) {
        self.0.lock().unwrap().push(serde_json::to_value(prefs).unwrap());
    }
}
fn click(u: &mut Ui, p: Pos2) {
    u.button(p, true);
    u.button(p, false);
}

fn reveal_grid_field(u: &mut Ui) {
    let ctx = u.ctx.clone();
    crate::preferences_test_support::show(&ctx, |events| u.frame_output(events), "Pixel grid size", egui::accesskit::Role::Label);
    crate::preferences_test_support::show(&ctx, |events| u.frame_output(events), "Pixel grid size", egui::accesskit::Role::SpinButton);
}

fn grid_field_center(u: &mut Ui, value: &str) -> Pos2 {
    reveal_grid_field(u);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let label = text_center(&shapes, "Pixel grid size");
    texts(&shapes)
        .into_iter()
        .find(|(text, p, size)| text == value && (p.y + size.y / 2.0 - label.y).abs() < 12.0 && p.x > label.x)
        .map(|(_, p, size)| p + size / 2.0)
        .unwrap_or_else(|| panic!("no grid value {value:?}: {:?}", texts(&shapes)))
}

#[test]
fn custom_grid_menu_opens_named_preferences_field_and_keyboard_edits_save_without_undo() {
    let mut u = Ui::new(Session::new());
    let saved = Arc::new(Mutex::new(Vec::new()));
    u.app.services = Box::new(SavePrefs(saved.clone()));
    let before = u.app.session.doc().unwrap().clone();
    let undo = u.app.session.undo_depth();
    u.app.ui.main_menu = true;
    // New egui areas first size themselves before their presented shapes are visible.
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    click(&mut u, text_center(&shapes, "Preferences"));
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(texts(&shapes).iter().any(|(t, _, _)| t == "Pixel grid size…"), "menu text: {:?}", texts(&shapes));
    let row = text_center(&shapes, "Pixel grid size…");
    click(&mut u, row);
    assert!(u.app.ui.preferences);
    for _ in 0..3 {
        u.frame(vec![]);
    }
    reveal_grid_field(&mut u);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(texts(&shapes).iter().any(|(t, _, _)| t == "Pixel grid size"), "actual Preferences presentation: {:?}", texts(&shapes));
    let label = text_center(&shapes, "Pixel grid size");
    assert!(label.y > 0.0 && label.y < 900.0, "the field is visible");
    let field = grid_field_center(&mut u, "1px");
    click(&mut u, field);
    u.frame(vec![Event::Text("8".into())]);
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert_eq!(u.app.session.prefs.pixel_grid_size, 8.0);
    assert_eq!(saved.lock().unwrap().last().unwrap()["pixel_grid_size"], 8.0);
    assert_eq!(u.app.session.doc().unwrap(), &before);
    assert_eq!(u.app.session.undo_depth(), undo);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let reset = texts(&shapes)
        .into_iter()
        .find(|(text, p, size)| text == "Reset" && (p.y + size.y / 2.0 - label.y).abs() < 12.0)
        .map(|(_, p, size)| p + size / 2.0)
        .unwrap();
    click(&mut u, reset);
    assert_eq!(u.app.session.prefs.pixel_grid_size, 1.0);
    assert_eq!(u.app.session.undo_depth(), undo);
}

fn grid_lines(u: &mut Ui) -> Vec<[Pos2; 2]> {
    let shapes = flat(&u.frame_output(vec![]).shapes);
    let r = u.app.canvas.rect;
    shapes
        .into_iter()
        .filter_map(|shape| match shape {
            Shape::LineSegment { points, stroke }
                if stroke.color == Color32::from_black_alpha(18)
                    && ((points[0].y == r.min.y && points[1].y == r.max.y) || (points[0].x == r.min.x && points[1].x == r.max.x)) =>
            {
                Some(points)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn custom_grid_canvas_paints_origin_multiples_at_threshold_and_hides_independently() {
    let mut s = Session::new();
    s.execute("pref.pixelGridSize", &json!({"size":8})).unwrap();
    s.ds_mut().unwrap().view.zoom = 0.5;
    let mut u = Ui::new(s);
    let lines = grid_lines(&mut u);
    assert!(!lines.is_empty() && lines.len() <= 8192);
    for line in lines {
        let a = u.app.to_page(line[0]);
        let v = if line[0].x == line[1].x { a.x } else { a.y };
        assert!((v / 8.0 - (v / 8.0).round()).abs() < 1e-4, "{v}");
    }
    u.app.session.ds_mut().unwrap().view.zoom = 0.49;
    assert!(grid_lines(&mut u).is_empty());
    u.app.session.ds_mut().unwrap().view.zoom = 0.5;
    u.app.run("view.pixelGrid", json!({"on":false}));
    assert!(grid_lines(&mut u).is_empty());
    assert!(u.app.session.ds().unwrap().view.snap_pixel);
}

#[test]
fn custom_grid_ruler_drag_rounds_free_offsets_but_keeps_fractional_object_targets() {
    for objects in [false, true] {
        let mut s = Session::new();
        s.execute("pref.pixelGridSize", &json!({"size":16})).unwrap();
        s.execute("view.rulers", &json!({"on":true})).unwrap();
        s.execute("view.snapObjects", &json!({"on":objects})).unwrap();
        s.execute("shape.rectangle", &json!({"x":100,"y":100.7,"width":120,"height":80})).unwrap();
        let mut u = Ui::new(s);
        let r = u.app.canvas.rect;
        let from = Pos2::new(r.center().x, r.min.y + 5.0);
        let to = u.app.to_screen(200.0, 100.7);
        u.frame(vec![Event::PointerMoved(from)]);
        u.button(from, true);
        u.frame(vec![Event::PointerMoved(to)]);
        u.button(to, false);
        let ds = u.app.session.ds().unwrap();
        let guides = ds.doc.node(ds.page).unwrap().guides();
        assert_eq!(guides.len(), 1);
        assert!((guides[0].offset - if objects { 100.7 } else { 96.0 }).abs() < 1e-5, "{:?}", guides[0]);
    }
}

#[test]
fn custom_grid_escape_reopen_discards_numeric_draft_in_its_preferences_opening() {
    let mut u = Ui::new(Session::new());
    u.app.ui.preferences = true;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let field = grid_field_center(&mut u, "1px");
    click(&mut u, field);
    u.frame(vec![Event::Text("32".into())]);
    u.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    u.frame(vec![Event::Key { key: Key::Escape, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert!(!u.app.ui.preferences);
    assert_eq!(u.app.session.prefs.pixel_grid_size, 1.0);
    u.app.ui.preferences = true;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let field = grid_field_center(&mut u, "1px");
    click(&mut u, field);
    u.frame(vec![Event::Text("8".into())]);
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    u.frame(vec![Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert_eq!(u.app.session.prefs.pixel_grid_size, 8.0);
}

#[test]
fn custom_grid_fractional_size_keeps_exact_display_and_arrow_wheel_steps() {
    let mut s = Session::new();
    s.execute("pref.pixelGridSize", &json!({"size":0.125})).unwrap();
    s.execute("pref.numberWheel", &json!({"on":true})).unwrap();
    let mut u = Ui::new(s);
    u.app.ui.preferences = true;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let field = grid_field_center(&mut u, "0.125px");
    click(&mut u, field);
    u.frame(vec![Event::Key { key: Key::ArrowUp, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    u.frame(vec![Event::Key { key: Key::ArrowUp, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert_eq!(u.app.session.prefs.pixel_grid_size, 0.25);
    let wheel = |delta| Event::MouseWheel {
        unit: egui::MouseWheelUnit::Line,
        source: egui::MouseWheelSource::Unknown,
        delta: egui::vec2(0.0, delta),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    };
    u.frame(vec![Event::PointerMoved(field), wheel(1.0)]);
    assert_eq!(u.app.session.prefs.pixel_grid_size, 0.375);
    let shapes = flat(&u.frame_output(vec![]).shapes);
    assert!(texts(&shapes).iter().any(|(text, _, _)| text == "0.375"), "draft must keep the accepted fractional value");
    u.frame(vec![Event::PointerMoved(field), wheel(-1.0)]);
    assert_eq!(u.app.session.prefs.pixel_grid_size, 0.25);
}

fn grid_arrow_at_limit(initial: f64, first: Key, reverse: Key, expected: f64) {
    let mut s = Session::new();
    s.execute("pref.pixelGridSize", &json!({"size":initial})).unwrap();
    let mut u = Ui::new(s);
    u.app.ui.preferences = true;
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let field = grid_field_center(&mut u, &format!("{initial}px"));
    click(&mut u, field);
    for (key, pressed) in [(first, true), (first, false)] {
        u.frame(vec![Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE }]);
    }
    assert_eq!(u.app.session.prefs.pixel_grid_size, initial);
    for (key, pressed) in [(reverse, true), (reverse, false)] {
        u.frame(vec![Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: Modifiers::NONE }]);
    }
    assert_eq!(u.app.session.prefs.pixel_grid_size, expected);
}

#[test]
fn custom_grid_boundary_arrow_reverses_immediately_at_the_minimum() {
    grid_arrow_at_limit(0.125, Key::ArrowDown, Key::ArrowUp, 0.25);
}

#[test]
fn custom_grid_boundary_arrow_reverses_immediately_at_the_maximum() {
    grid_arrow_at_limit(10000.0, Key::ArrowUp, Key::ArrowDown, 9999.875);
}
